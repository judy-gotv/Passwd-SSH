//! Ops error types.
//! Author: Passwd Word

use thiserror::Error;

/// Errors from monitoring / snippets / batch execution.
#[derive(Debug, Error)]
pub enum OpsError {
    /// SSH transport failure.
    #[error("ssh error: {0}")]
    Ssh(#[from] opsdeck_ssh::SshError),

    /// Storage failure.
    #[error("storage error: {0}")]
    Storage(#[from] opsdeck_storage::StorageError),

    /// Metric output could not be parsed.
    #[error("metric parse error: {0}")]
    Parse(String),

    /// Refused to run a dangerous command without confirmation.
    #[error("dangerous command blocked: {0}")]
    Dangerous(String),

    /// Snippet not found.
    #[error("snippet not found: {0}")]
    SnippetNotFound(String),
}
