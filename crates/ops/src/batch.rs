//! Batch command execution across multiple sessions.
//! Author: Passwd Word

use std::sync::Arc;

use opsdeck_ssh::{ExecOutput, SessionId, SessionManager, SshError};

use crate::error::OpsError;
use crate::snippets::dangerous_reason;

/// Result of running a command on one session.
#[derive(Debug)]
pub struct BatchResult {
    pub session: SessionId,
    pub label: String,
    pub result: Result<ExecOutput, SshError>,
}

/// Run `command` concurrently on all given sessions.
///
/// Dangerous commands (see [`dangerous_reason`]) are rejected unless
/// `allow_dangerous` is set — the UI must obtain explicit confirmation first.
pub async fn run_on_hosts(
    ssh: &Arc<SessionManager>,
    sessions: &[(SessionId, String)],
    command: &str,
    allow_dangerous: bool,
) -> Result<Vec<BatchResult>, OpsError> {
    if !allow_dangerous {
        if let Some(reason) = dangerous_reason(command) {
            return Err(OpsError::Dangerous(reason));
        }
    }

    let mut handles = Vec::with_capacity(sessions.len());
    for (session, label) in sessions {
        let ssh = ssh.clone();
        let session = *session;
        let label = label.clone();
        let command = command.to_string();
        handles.push(tokio::spawn(async move {
            let result = ssh.exec(session, &command).await;
            BatchResult {
                session,
                label,
                result,
            }
        }));
    }

    let mut out = Vec::with_capacity(handles.len());
    for h in handles {
        out.push(
            h.await
                .map_err(|e| OpsError::Ssh(SshError::Channel(format!("batch join: {e}"))))?,
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_blocks_dangerous_by_default() {
        // No sessions needed: the dangerous check runs before any I/O.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
            let dir = std::env::temp_dir();
            let ssh = Arc::new(SessionManager::new(tx, &dir.join("kh.json")));
            let err = run_on_hosts(&ssh, &[], "rm -rf /", false)
                .await
                .unwrap_err();
            assert!(matches!(err, OpsError::Dangerous(_)));
        });
    }
}
