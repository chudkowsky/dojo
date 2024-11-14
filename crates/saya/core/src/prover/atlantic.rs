use herodotus_sharp_playground::models::{JobResponse, ProverVersion, SharpSdk};
use tracing::trace;
use url::Url;

use crate::{db::sql_lite::SqliteDb, errors::Error};

const LAYOUT_BRIDGE: &[u8; 31478586] =
    include_bytes!("../../../../../bin/saya/programs/layout_bridge.json");
pub struct AtlanticProver {
    pub api_key: String,
    pub url: Url,
    pub db: SqliteDb,
}
pub type QueryId = String;

impl AtlanticProver {
    pub fn new(api_key: String, url: Url,db: SqliteDb) -> Self {
        AtlanticProver { api_key, url, db}
    }
    pub async fn submit_proof_generation(&self, pie: Vec<u8>) -> Result<QueryId, Error> {
        let base_url = "https://atlantic.api.herodotus.cloud";
        let sdk = SharpSdk::new(self.api_key.clone(), base_url)?;
        let is_alive = sdk.get_is_alive().await?;
        if !is_alive {
            return Err(Error::ServerNotAliveError);
        }
        let id =
            sdk.proof_generation(pie, "dynamic", ProverVersion::Starkware).await?.sharp_query_id;
        Ok(id)
    }
    pub async fn submit_atlantic_query(&self, proof: String) -> Result<QueryId, Error> {
        let base_url = "https://atlantic.api.herodotus.cloud";
        let sdk = SharpSdk::new(self.api_key.clone(), base_url)?;
        let is_alive = sdk.get_is_alive().await?;
        if !is_alive {
            return Err(Error::ServerNotAliveError);
        }

        // We need to format the input as a json object because layout bridge expects a json object
        // with {"proof": proof}
        let input = format!("{{\n\t\"proof\": {}\n}}", proof);

        let id = sdk
            .l2_atlantic_query(
                LAYOUT_BRIDGE.to_vec(),
                input.as_bytes().to_vec(),
                ProverVersion::Starkware,
                false,
            )
            .await?
            .sharp_query_id;
        Ok(id)
    }

    pub async fn fetch_proof(&self, query_id: &str) -> Result<String, Error> {
        let base_url = "https://atlantic.api.herodotus.cloud/sharp_queries";
        let proof_path = format!("query_{}/proof.json", query_id);
        let client = reqwest::Client::new();
        let url = format!("{}/{}", base_url, proof_path);
        let response = client.get(&url).send().await?;
        let response_text = response.text().await?;
        Ok(response_text)
    }

    pub async fn check_proof_generation_status(
        &self,
        query_id: &str,
    ) -> Result<JobResponse, Error> {
        let base_url = "https://atlantic.api.herodotus.cloud";
        let sdk = SharpSdk::new(self.api_key.clone(), base_url)?;
        let is_alive = sdk.get_is_alive().await?;
        if !is_alive {
            return Err(Error::ServerNotAliveError);
        }
        trace!("Checking status for query_id {}", query_id);
        let status = sdk.get_sharp_query_jobs(&query_id).await?;
        Ok(status)
    }
    pub async fn check_query_status(&self, id: u32, query_id: &str) -> Result<bool, Error> {
        trace!("Checking status for block {}, query_id {}", id, query_id);
        let job_response = self.check_proof_generation_status(query_id).await?;
        if job_response.jobs.is_empty() {
            trace!("No jobs for block {}, query_id {}", id, query_id);
            return Ok(false);
        }
        let all_completed =
            job_response.jobs.iter().map(|job| job.status == "COMPLETED").all(|x| x);
        Ok(all_completed)
    }
}
