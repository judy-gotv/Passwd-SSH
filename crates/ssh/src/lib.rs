//! OpsDeck SSH engine: russh-based sessions, auth, shells, exec, forwarding.
//!
//! - [`SessionManager`]: connection lifecycle (docs §6.2 state machine).
//! - [`SshEngine`]: engine interface (docs §6.3).
//! - [`CredentialProvider`]: app-supplied secret resolution (vault-backed).
//! - [`HostKeyPrompt`]: UI approval flow for unknown/changed host keys.
//!
//! Author: Passwd Word

mod agent;
mod error;
mod session;

pub use agent::authenticate_agent;
pub use error::SshError;
pub use session::{
    ClientHandle, CredentialProvider, ExecOutput, HostKeyDecision, HostKeyPrompt, LocalForward,
    ManagedSession, SessionHandler, SessionId, SessionInfo, SessionManager, SessionState,
    ShellChannel, ShellCommand, ShellEvent, SshEngine,
};
