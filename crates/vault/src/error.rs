//! Vault error types.
//! Author: Passwd Word

use thiserror::Error;

/// Errors produced by vault operations.
///
/// Note: error messages never contain secret material.
#[derive(Debug, Error)]
pub enum VaultError {
    /// The master password is wrong (verification failed).
    #[error("incorrect master password")]
    WrongPassword,

    /// The encrypted blob is malformed or has been tampered with.
    #[error("failed to decrypt data: invalid blob")]
    InvalidBlob,

    /// Argon2 key derivation failed.
    #[error("key derivation failed")]
    KeyDerivation,

    /// Encryption failed.
    #[error("encryption failed")]
    Encryption,

    /// A vault is already unlocked in this handle.
    #[error("vault is already unlocked")]
    AlreadyUnlocked,

    /// Tried to use the vault before unlocking it.
    #[error("vault is locked")]
    Locked,
}
