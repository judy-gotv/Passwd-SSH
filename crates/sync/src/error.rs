//! Sync error types.
//! Author: Passwd Word

use thiserror::Error;

/// Errors from the sync subsystem.
#[derive(Debug, Error)]
pub enum SyncError {
    /// Remote transport failure.
    #[error("remote error: {0}")]
    Remote(String),

    /// No remote is configured.
    #[error("sync remote is not configured")]
    NotConfigured,

    /// A record failed to decrypt (wrong vault / corrupt data).
    #[error("cannot decrypt sync record {0}")]
    Decrypt(String),

    /// Record payload is malformed.
    #[error("malformed sync record {0}: {1}")]
    Malformed(String, String),

    /// Storage failure.
    #[error("storage error: {0}")]
    Storage(#[from] opsdeck_storage::StorageError),

    /// Vault failure.
    #[error("vault error: {0}")]
    Vault(#[from] opsdeck_vault::VaultError),

    /// JSON failure.
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// Local I/O failure (file remote).
    #[error("i/o error: {0}")]
    Io(String),
}

impl From<std::io::Error> for SyncError {
    fn from(e: std::io::Error) -> Self {
        SyncError::Io(e.to_string())
    }
}
