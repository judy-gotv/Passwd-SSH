//! Storage error types.
//! Author: Passwd Word

use thiserror::Error;

/// Errors from the storage layer.
#[derive(Debug, Error)]
pub enum StorageError {
    /// Underlying database error.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Migration failed.
    #[error("migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    /// Requested row does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// JSON (de)serialization failed.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Invalid data (e.g. bad UUID string).
    #[error("invalid data: {0}")]
    InvalidData(String),
}
