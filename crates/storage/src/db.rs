//! Database handle: connection pool + migrations.
//! Author: Passwd Word

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

use crate::{StorageError, models::now_unix};

/// The storage handle. Clone it freely; it wraps a connection pool.
#[derive(Debug, Clone)]
pub struct Storage {
    pool: SqlitePool,
}

impl Storage {
    /// Open (creating if needed) the SQLite database at `path` and run migrations.
    pub async fn open(path: &Path) -> Result<Self, StorageError> {
        let opts = SqliteConnectOptions::from_str(&format!("sqlite:{}?mode=rwc", path.display()))
            .map_err(|e| StorageError::InvalidData(e.to_string()))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(opts)
            .await?;
        let storage = Self { pool };
        storage.migrate().await?;
        Ok(storage)
    }

    /// Open an in-memory database (used by tests).
    pub async fn open_in_memory() -> Result<Self, StorageError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        let storage = Self { pool };
        storage.migrate().await?;
        Ok(storage)
    }

    /// Run embedded migrations.
    async fn migrate(&self) -> Result<(), StorageError> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    /// Borrow the underlying pool (for DAOs).
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    // -- key/value settings -------------------------------------------------

    /// Read a setting value.
    pub async fn get_setting(&self, key: &str) -> Result<Option<String>, StorageError> {
        let row: Option<(String,)> = sqlx::query_as("SELECT value FROM kv_settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|(v,)| v))
    }

    /// Write a setting value (upsert).
    pub async fn set_setting(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let now = now_unix();
        sqlx::query(
            "INSERT INTO kv_settings (key, value, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(value)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn settings_roundtrip() {
        let db = Storage::open_in_memory().await.unwrap();
        assert_eq!(db.get_setting("theme").await.unwrap(), None);
        db.set_setting("theme", "dark").await.unwrap();
        assert_eq!(db.get_setting("theme").await.unwrap().as_deref(), Some("dark"));
        db.set_setting("theme", "light").await.unwrap();
        assert_eq!(db.get_setting("theme").await.unwrap().as_deref(), Some("light"));
    }
}
