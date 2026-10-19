//! SFTP error types.
//! Author: Passwd Word

use thiserror::Error;

/// Errors from the SFTP engine.
#[derive(Debug, Error)]
pub enum SftpError {
    /// Remote SFTP failure.
    #[error("sftp error: {0}")]
    Sftp(#[from] russh_sftp::client::error::Error),

    /// SSH transport failure.
    #[error("ssh error: {0}")]
    Ssh(#[from] opsdeck_ssh::SshError),

    /// Storage failure.
    #[error("storage error: {0}")]
    Storage(#[from] opsdeck_storage::StorageError),

    /// Local I/O failure.
    #[error("local i/o error: {0}")]
    Io(String),

    /// No active SSH session for the host.
    #[error("no active session for host {0}")]
    NoSession(String),

    /// The transfer was cancelled.
    #[error("transfer cancelled")]
    Cancelled,

    /// The transfer was paused.
    #[error("transfer paused")]
    Paused,
}

impl From<std::io::Error> for SftpError {
    fn from(e: std::io::Error) -> Self {
        SftpError::Io(e.to_string())
    }
}
