//! OpsDeck Ops: monitoring, snippets, batch execution.
//!
//! - [`metrics`]: read-only server monitoring (docs §6.8).
//! - [`snippets`]: saved command library.
//! - [`batch`]: concurrent execution across sessions.
//!
//! Author: Passwd Word

pub mod batch;
mod error;
pub mod metrics;
pub mod snippets;

pub use batch::{BatchResult, run_on_hosts};
pub use error::OpsError;
pub use metrics::{MetricsCollector, ProcessInfo, Snapshot, commands};
pub use snippets::{SnippetService, dangerous_reason};
