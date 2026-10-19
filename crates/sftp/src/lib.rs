//! OpsDeck SFTP: remote file management over russh-sftp.
//!
//! - [`engine`]: [`SftpEngine`][engine::SftpEngine] trait + russh implementation
//!   (docs §6.7), with resume-capable uploads/downloads.
//! - [`queue`]: persistent transfer queue with pause / resume / cancel
//!   (docs §6.6).
//! - [`types`]: [`FileEntry`][types::FileEntry] (docs §9.2).
//!
//! Author: Passwd Word

pub mod engine;
mod error;
pub mod queue;
pub mod types;

pub use engine::{ProgressSender, RusshSftpEngine, SftpEngine, TransferProgress};
pub use error::SftpError;
pub use queue::{QueueEvent, SshPauseOutcome, TransferQueue, progress_channel};
pub use types::{FileEntry, FileKind, sort_entries};
