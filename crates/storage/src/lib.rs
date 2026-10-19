//! OpsDeck Storage: SQLite persistence layer.
//!
//! - [`Storage`]: database handle (pool + migrations + kv settings).
//! - [`models`]: domain types mirroring docs §5 / §9.
//! - [`dao`]: data-access objects (`HostDao`, `CredentialDao`, …).
//! - [`StorageError`]: error type.
//!
//! Author: Passwd Word

mod dao;
mod db;
mod error;
pub mod models;

pub use dao::{CredentialDao, HostDao, HostGroupDao, MetricDao, SnippetDao, TransferDao};
pub use db::Storage;
pub use error::StorageError;
