pub mod db;
pub mod piltover;
pub mod prover;
pub mod macros;
pub mod starknet;
use std::path::Path;

use cairo_vm::types::layout_name::LayoutName;
use db::sql_lite::SqliteDb;
use db::{AtlanticStatus, SayaProvingDb};
use piltover::Piltover;
use prove_block::prove_block;
use prover::Prover;
use starknet::account::StarknetAccountData;
use starknet_types_core::felt::Felt;
use tokio::{fs, join};
use tracing::{info, warn};
use url::Url;
pub struct Saya {
    pub config: SayaConfig,
    pub last_settled_block: u32,
    pub last_sent_for_prove_block: u32,
    pub db: SqliteDb,
    pub piltover: Piltover,
}
#[derive(Debug)]
pub struct SayaConfig {
    pub rpc_url: Url,
    pub prover_url: Url,
    pub prover_key: String,
    pub settlement_contract: Felt,
    pub starknet_account: StarknetAccountData,
}

impl Saya {
    pub async fn new(config: SayaConfig) -> Saya {
        let piltover = Piltover {
            contract: config.settlement_contract,
            account: config.starknet_account.get_starknet_account(),
        };
        let last_settled_block = piltover.get_state().await.block_number;

        let db = SqliteDb::new("blocks.db").await.unwrap();
        let pending_blocks = db.list_pending_blocks().await.unwrap();
        let last_sent_for_prove_block =
            pending_blocks.iter().map(|block| block.id).max().unwrap_or(last_settled_block);

        println!("{:?}", pending_blocks);

        Saya { config, last_settled_block, last_sent_for_prove_block, db, piltover }
    }

    pub async fn start(&mut self) {
        join!(
            self.block_proving_task_step1(),
            self.block_proving_task_step2(),
            self.block_settling_task(),
        );
    }
    async fn block_proving_task_step1(&self) {
        let mut block_number = self.last_sent_for_prove_block + 1;
        let prover = Prover::new(self.config.prover_key.clone(), self.config.prover_url.clone());
        let poll_interval_secs = 10;
        loop {
            if self.db.list_pending_blocks().await.unwrap().len() >= 1 {
                warn!("Too many pending blocks, waiting");
                tokio::time::sleep(std::time::Duration::from_secs(poll_interval_secs)).await;
            } else {
                tokio::time::sleep(std::time::Duration::from_secs(poll_interval_secs)).await;
                // let tempdir = temp_dir();
                // let pie_path = tempdir.join("pie.zip"); //for prod
                let pie_path = format!("pie{}.zip", block_number); // for dev
                let pie_file = Path::new(&pie_path);
                let os = include_bytes!(
                    "/home/mateuszchudkowski/dev/cartdrige_dojo/bin/saya/programs/snos.json"
                );
                let (pie, _) = prove_block(
                    os,
                    block_number.into(),
                    "http://localhost:9545",
                    LayoutName::all_cairo,
                    true,
                )
                .await
                .unwrap();
                pie.write_zip_file(&pie_file).unwrap(); // Optimize this so we wont have to write to disk
                let pie = std::fs::read(pie_file).unwrap();
                let query_id = prover.submit_proof_generation(pie).await;
                info!("Query id: {}", query_id);
                self.db
                    .insert_block(block_number, &query_id, AtlanticStatus::InProgress)
                    .await
                    .unwrap();
                block_number += 1;
            }
        }
    }
    async fn block_settling_task(&self) {
        // TODO: Implement this
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            info!("block settling task");
        }
    }
    async fn block_proving_task_step2(&self) {
        let prover = Prover::new(self.config.prover_key.clone(), self.config.prover_url.clone());
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;

            let pending_blocks = self.db.list_pending_blocks().await.unwrap();
            for block in pending_blocks {
                if block.status == AtlanticStatus::InProgress {
                    info!(
                        "Checking status for block {}, query_id {}",
                        block.id, block.query_id_step1
                    );
                    let x = prover.check_proof_generation_status(&block.query_id_step1).await;
                    println!("{:?}", x);
                    if x.is_none() {
                        continue;
                    }
                    let status = x.unwrap();
                    if status.jobs.len() == 0 {
                        info!("No jobs for block {}, query_id {}", block.id, block.query_id_step1);
                        continue;
                    }
                    if status.jobs.iter().map(|job| job.status == "COMPLETED").all(|x| x) {
                        self.db
                            .update_block_status(block.id, AtlanticStatus::Step1Completed)
                            .await
                            .unwrap();

                        info!(
                            "All jobs completed for block {}, query_id {}",
                            block.id, block.query_id_step1
                        );
                        let proof = prover.fetch_proof(&block.query_id_step1).await;
                        fs::write(&format!("proof{}.json", block.id), proof.clone()).await.unwrap();
                        let query = Prover::submit_atlantic_query(proof).await; //TODO: Implement this
                        self.db.update_query_id_step2(block.id, &query).await.unwrap();
                    } else {
                        info!(
                            "Some jobs are not completed for block {}, query_id {}",
                            block.id, block.query_id_step1
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::db::sql_lite::SqliteDb;
    use crate::db::SayaProvingDb;

    #[tokio::test]
    async fn test_all() -> Result<(), sqlx::Error> {
        let db = SqliteDb::new("/home/mateuszchudkowski/dev/cartdrige_dojo/blocks.db").await?;
        let pending = db.list_pending_blocks().await?;
        // db.insert_block(280124,"01JCK1V9EH5QCZ5JBNGG0FEWRP",AtlanticStatus::InProgress).await?;
        println!("{:?}", pending);
        // println!("{:?}", s);
        Ok(())
    }
}
