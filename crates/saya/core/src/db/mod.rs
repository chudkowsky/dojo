pub mod sql_lite;
use sqlx::Error;

#[allow(async_fn_in_trait)]
pub trait SayaProvingDb {
    async fn insert_block(
        &self,
        block_id: u32,
        query_id: &str,
        status: AtlanticStatus,
    ) -> Result<(), Error>;
    async fn check_status(&self, block: u32) -> Result<Block, sqlx::Error>;
    async fn update_block_status(
        &self,
        block_id: u32,
        status: AtlanticStatus,
    ) -> Result<(), sqlx::Error>;
    async fn update_query_id_step2(&self, block_id: u32, query_id: &str)
    -> Result<(), sqlx::Error>;
    async fn list_pending_blocks(&self) -> Result<Vec<Block>, sqlx::Error>;
}
#[derive(Debug, Clone)]
pub struct Block {
    pub id: u32,
    pub query_id_step1: String,
    pub query_id_step2: String,
    pub status: AtlanticStatus,
}
#[derive(Debug, Clone, PartialEq)]
pub enum AtlanticStatus {
    InProgress,
    Failed,
    Step1Completed,
    Completed,
}
impl AtlanticStatus {
    pub fn as_str(&self) -> &str {
        match self {
            AtlanticStatus::InProgress => "IN_PROGRESS",
            AtlanticStatus::Failed => "FAILED",
            AtlanticStatus::Step1Completed => "STEP1_COMPLETE",
            AtlanticStatus::Completed => "COMPLETED",
        }
    }
}
impl From<&str> for AtlanticStatus {
    fn from(s: &str) -> Self {
        match s {
            "IN_PROGRESS" => AtlanticStatus::InProgress,
            "FAILED" => AtlanticStatus::Failed,
            "STEP1_COMPLETE" => AtlanticStatus::Step1Completed,
            "COMPLETED" => AtlanticStatus::Completed,
            _ => panic!("Invalid status"),
        }
    }
}
