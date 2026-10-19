//! OpsDeck Sync: encrypted cloud sync framework (docs §7.3, M6).
//!
//! - [`model`]: encrypted [`SyncRecord`][model::SyncRecord]s — the remote
//!   only ever sees ciphertext.
//! - [`conflict`]: last-write-wins + manual-merge conflict reporting.
//! - [`client`]: [`Remote`][client::Remote] transports
//!   ([`FileRemote`][client::FileRemote], [`HttpRemote`][client::HttpRemote])
//!   and the [`SyncEngine`][client::SyncEngine].
//! - [`http`]: minimal HTTP/1.1 client (plain + TLS) used by the HTTP remote
//!   and the auto-update check.
//!
//! Author: Passwd Word

pub mod client;
pub mod conflict;
pub mod error;
pub mod http;
pub mod model;

pub use client::{FileRemote, HttpRemote, Remote, SyncEngine, device_id};
pub use conflict::{Conflict, Resolution, resolve};
pub use error::SyncError;
pub use http::{HttpResponse, http_get_text, http_request};
pub use model::{SyncCollection, SyncEntity, SyncRecord, SyncSummary, open_entity};
