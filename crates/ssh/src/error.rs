//! SSH error types.
//! Author: Passwd Word

use thiserror::Error;

/// Errors from the SSH engine.
///
/// Secret material (passwords, keys) is never included in messages.
#[derive(Debug, Error)]
pub enum SshError {
    /// TCP / transport failure.
    #[error("connection failed: {0}")]
    Connection(String),

    /// Authentication was rejected by the server.
    #[error("authentication failed: {0}")]
    Auth(String),

    /// Channel operation failed.
    #[error("channel error: {0}")]
    Channel(String),

    /// Key parsing / handling failed.
    #[error("key error: {0}")]
    Key(String),

    /// SSH agent failure.
    #[error("ssh-agent error: {0}")]
    Agent(String),

    /// The user rejected the server's host key.
    #[error("host key rejected")]
    HostKeyRejected,

    /// Operation timed out.
    #[error("timed out: {0}")]
    Timeout(String),

    /// Session id unknown.
    #[error("session not found")]
    SessionNotFound,

    /// Session is not in a usable state for this operation.
    #[error("session not ready: {0}")]
    NotReady(String),

    /// Credential lookup failed.
    #[error("credential error: {0}")]
    Credential(String),

    /// Underlying russh error.
    #[error("ssh protocol error: {0}")]
    Russh(#[from] russh::Error),

    /// Channel send failure (needed by the agent [`Signer`][russh::Signer]).
    #[error("ssh channel send error")]
    Send(#[from] russh::SendError),

    /// I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}
