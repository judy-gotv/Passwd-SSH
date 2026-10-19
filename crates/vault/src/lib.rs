//! OpsDeck Vault: local encrypted credential storage primitives.
//!
//! - [`Vault`]: master-password protected encryption context.
//! - [`SecretString`] / [`SecretBytes`]: zeroizing secret containers.
//! - [`VaultError`]: error type (never leaks secrets).
//!
//! Author: Passwd Word

mod error;
mod secret;
mod vault;

pub use error::VaultError;
pub use secret::{SecretBytes, SecretString};
pub use vault::{EncryptedBlob, KdfParams, Vault, VaultVerifier};
