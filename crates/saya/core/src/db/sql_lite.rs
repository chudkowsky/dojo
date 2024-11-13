use std::fs;
use std::path::Path;

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Error, Pool, Row, Sqlite};
use tracing::trace;

use super::Block;
use crate::db::{AtlanticStatus, SayaProvingDb};

pub struct SqliteDb {
    pool: Pool<Sqlite>,
}

impl SqliteDb {
    pub async fn new(path: &str) -> Result<Self, Error> {
        // Check if there is a database file at the path
        if !Path::new(path).exists() {
            trace!("Database file not found. A new one will be created at: {}", path);
            fs::File::create(path)?;
        } else {
            trace!("Database file found at: {}", path);
        }

        // Connect to the database
        let pool = SqlitePoolOptions::new().connect(&format!("sqlite:{}", path)).await?;

        // Check if the blocks table exists
        let table_exists =
            sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name='blocks';")
                .fetch_optional(&pool)
                .await?
                .is_some();
        // If the table doesn't exist or doesn't have the correct structure, create it
        if !table_exists || !Self::check_columns(&pool).await? {
            trace!("Creating or updating the 'blocks' table...");
            Self::create_database(&pool).await?;
        } else {
            trace!("Table 'blocks' with correct structure found.");
        }
        Ok(Self { pool })
    }

    // Function to check if the blocks table has the correct columns
    async fn check_columns(pool: &Pool<Sqlite>) -> Result<bool, Error> {
        let columns = sqlx::query("PRAGMA table_info(blocks);").fetch_all(pool).await?;

        // Check if the table has the expected columns: id, query_id, and status
        let mut has_id = false;
        let mut has_query_id_step1 = false;
        let mut has_query_id_step2 = false;
        let mut has_status = false;

        for column in columns {
            let name: String = column.get("name");
            match name.as_str() {
                "id" => has_id = true,
                "query_id_step1" => has_query_id_step1 = true,
                "query_id_step2" => has_query_id_step2 = true,
                "status" => has_status = true,
                _ => {}
            }
        }

        Ok(has_id && has_query_id_step1 && has_query_id_step2 && has_status)
    }

    // Function to create the blocks table with the correct schema
    pub async fn create_database(pool: &Pool<Sqlite>) -> Result<(), Error> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS blocks (
                id INTEGER PRIMARY KEY,
                query_id_step1 TEXT NOT NULL, 
                query_id_step2 TEXT,          
                status TEXT NOT NULL CHECK (status IN ('IN_PROGRESS', 'FAILED', 'STEP1_COMPLETE', \
             'COMPLETED'))
        );",
        )
        .execute(pool)
        .await?;
        Ok(())
    }
}

impl SayaProvingDb for SqliteDb {
    async fn check_status(&self, block: u32) -> Result<Block, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, query_id_step1, query_id_step2, status FROM blocks WHERE id = ?1",
        )
        .bind(block)
        .fetch_all(&self.pool)
        .await?;
        let result = &rows[0];
        let id = result.get("id");
        let query_id_step1 = result.get("query_id_step1");
        let query_id_step2 = result.get("query_id_step2");
        let status: &str = result.get("status");
        Ok(Block { id, query_id_step1, query_id_step2, status: AtlanticStatus::from(status) })
    }
    async fn insert_block(
        &self,
        block_id: u32,
        query_id: &str,
        status: AtlanticStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO blocks (id, query_id_step1, status) VALUES (?1, ?2, ?3)")
            .bind(block_id)
            .bind(query_id)
            .bind(status.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    async fn update_block_status(
        &self,
        block_id: u32,
        status: AtlanticStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE blocks SET status = ?1 WHERE id = ?2")
            .bind(status.as_str())
            .bind(block_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_query_id_step2(
        &self,
        block_id: u32,
        query_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE blocks SET query_id_step2 = ?1 WHERE id = ?2")
            .bind(query_id)
            .bind(block_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    async fn list_pending_blocks(&self) -> Result<Vec<Block>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, query_id_step1, query_id_step2, status FROM blocks WHERE status = \
             'IN_PROGRESS'",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut result = Vec::new();
        for row in rows {
            let id = row.get("id");
            let query_id_step1 = row.get("query_id_step1");
            let query_id_step2 = row.get("query_id_step2");
            let status: &str = row.get("status");
            result.push(Block {
                id,
                query_id_step1,
                query_id_step2,
                status: AtlanticStatus::from(status),
            });
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sqlx::Pool;

    use crate::db::sql_lite::SqliteDb;
    use crate::db::{AtlanticStatus, SayaProvingDb};

    #[tokio::test]
    async fn test_sqlite_db_new() {
        // Initialize the database in memory
        let db = SqliteDb::new(":memory:").await.expect("Failed to create database");
        let pool: Arc<Pool<_>> = Arc::new(db.pool);

        // Verify the database connection and table structure
        let table_exists =
            sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name='blocks';")
                .fetch_optional(pool.as_ref())
                .await
                .expect("Failed to check table existence")
                .is_some();

        assert!(table_exists, "Table 'blocks' should exist");
    }

    #[tokio::test]
    async fn test_insert_block() {
        let db = SqliteDb::new(":memory:").await.expect("Failed to create database");

        // Insert a block and verify it's added
        db.insert_block(1, "query_id_1", AtlanticStatus::InProgress)
            .await
            .expect("Failed to insert block");

        let block = db.check_status(1).await.expect("Failed to check block status");
        assert_eq!(block.id, 1);
        assert_eq!(block.query_id_step1, "query_id_1");
        assert_eq!(block.status, AtlanticStatus::InProgress);
    }

    #[tokio::test]
    async fn test_update_block_status() {
        let db = SqliteDb::new(":memory:").await.expect("Failed to create database");

        // Insert a block to update its status later
        db.insert_block(1, "query_id_1", AtlanticStatus::InProgress)
            .await
            .expect("Failed to insert block");

        // Update the block status
        db.update_block_status(1, AtlanticStatus::Step1Completed)
            .await
            .expect("Failed to update block status");

        let block = db.check_status(1).await.expect("Failed to check block status");
        assert_eq!(block.status, AtlanticStatus::Step1Completed);
    }

    #[tokio::test]
    async fn test_update_query_id_step2() {
        let db = SqliteDb::new(":memory:").await.expect("Failed to create database");

        // Insert a block to update its step2 query_id
        db.insert_block(1, "query_id_1", AtlanticStatus::InProgress)
            .await
            .expect("Failed to insert block");

        // Update the query_id_step2
        db.update_query_id_step2(1, "query_id_2").await.expect("Failed to update query_id_step2");

        let block = db.check_status(1).await.expect("Failed to check block status");
        assert_eq!(block.query_id_step2, "query_id_2");
    }

    #[tokio::test]
    async fn test_list_pending_blocks() {
        let db = SqliteDb::new(":memory:").await.expect("Failed to create database");

        // Insert a few blocks with different statuses
        db.insert_block(1, "query_id_1", AtlanticStatus::InProgress)
            .await
            .expect("Failed to insert block");
        db.insert_block(2, "query_id_2", AtlanticStatus::Failed)
            .await
            .expect("Failed to insert block");
        db.insert_block(3, "query_id_3", AtlanticStatus::InProgress)
            .await
            .expect("Failed to insert block");

        // List only the blocks with 'IN_PROGRESS' status
        let pending_blocks = db.list_pending_blocks().await.expect("Failed to list pending blocks");

        assert_eq!(pending_blocks.len(), 2);
        assert!(pending_blocks.iter().any(|b| b.id == 1));
        assert!(pending_blocks.iter().any(|b| b.id == 3));
    }
}
