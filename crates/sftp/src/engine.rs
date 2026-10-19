//! SFTP engine: browsing + transfers over russh-sftp (docs §6.7).
//!
//! A fresh SFTP subsystem channel is opened per operation — channels are
//! cheap and this keeps the engine free of cross-operation state.
//!
//! Author: Passwd Word

use std::collections::HashMap;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use async_trait::async_trait;
use opsdeck_ssh::{SessionId, SessionManager};
use opsdeck_storage::models::{TransferDirection, TransferTask};
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileType, OpenFlags};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{Mutex, mpsc};

use crate::error::SftpError;
use crate::types::{FileEntry, FileKind, sort_entries};

/// Progress update for a running transfer.
#[derive(Debug, Clone)]
pub struct TransferProgress {
    pub task_id: String,
    pub bytes_done: u64,
    pub bytes_total: Option<u64>,
}

/// Sink for [`TransferProgress`] updates.
pub type ProgressSender = mpsc::UnboundedSender<TransferProgress>;

/// SFTP engine interface (docs §6.7).
#[async_trait]
pub trait SftpEngine: Send + Sync {
    async fn list_dir(&self, session: SessionId, path: &str) -> Result<Vec<FileEntry>, SftpError>;
    async fn mkdir(&self, session: SessionId, path: &str) -> Result<(), SftpError>;
    async fn remove(&self, session: SessionId, path: &str) -> Result<(), SftpError>;
    async fn rename(&self, session: SessionId, from: &str, to: &str) -> Result<(), SftpError>;
    async fn upload(&self, task: TransferTask, progress: ProgressSender) -> Result<(), SftpError>;
    async fn download(&self, task: TransferTask, progress: ProgressSender)
    -> Result<(), SftpError>;
}

/// russh-sftp based engine.
pub struct RusshSftpEngine {
    ssh: Arc<SessionManager>,
    /// host_id → active SSH session, maintained by the app.
    host_sessions: Arc<Mutex<HashMap<String, SessionId>>>,
}

impl RusshSftpEngine {
    /// Create an engine multiplexing over `ssh`.
    pub fn new(ssh: Arc<SessionManager>) -> Self {
        Self {
            ssh,
            host_sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Bind a host to its active SSH session (call on connect).
    pub async fn link_host(&self, host_id: &str, session: SessionId) {
        self.host_sessions
            .lock()
            .await
            .insert(host_id.to_string(), session);
    }

    /// Unbind a host (call on disconnect).
    pub async fn unlink_host(&self, host_id: &str) {
        self.host_sessions.lock().await.remove(host_id);
    }

    async fn session_for_host(&self, host_id: &str) -> Result<SessionId, SftpError> {
        self.host_sessions
            .lock()
            .await
            .get(host_id)
            .copied()
            .ok_or_else(|| SftpError::NoSession(host_id.to_string()))
    }

    /// Open a fresh SFTP subsystem channel on an SSH session.
    async fn open_sftp(&self, session: SessionId) -> Result<SftpSession, SftpError> {
        let handle = self.ssh.handle_for(session).await?;
        let channel = handle
            .lock()
            .await
            .channel_open_session()
            .await
            .map_err(|e| {
                SftpError::Ssh(opsdeck_ssh::SshError::Channel(format!("sftp channel: {e}")))
            })?;
        channel.request_subsystem(true, "sftp").await.map_err(|e| {
            SftpError::Ssh(opsdeck_ssh::SshError::Channel(format!(
                "sftp subsystem: {e}"
            )))
        })?;
        let stream = channel.into_stream();
        SftpSession::new(stream).await.map_err(SftpError::from)
    }

    /// Emit progress no more often than ~4 Hz to avoid UI spam.
    fn progress_state() -> ProgressThrottle {
        ProgressThrottle {
            last_emit: std::time::Instant::now(),
            last_done: 0,
        }
    }
}

struct ProgressThrottle {
    last_emit: std::time::Instant,
    last_done: u64,
}

impl ProgressThrottle {
    fn maybe_emit(
        &mut self,
        tx: &ProgressSender,
        task_id: &str,
        done: u64,
        total: Option<u64>,
        force: bool,
    ) {
        let elapsed = self.last_emit.elapsed();
        if force || elapsed.as_millis() >= 250 || done - self.last_done >= 256 * 1024 {
            self.last_emit = std::time::Instant::now();
            self.last_done = done;
            let _ = tx.send(TransferProgress {
                task_id: task_id.to_string(),
                bytes_done: done,
                bytes_total: total,
            });
        }
    }
}

/// Render one permission bit as `rwx`/`-`.
fn flag(set: bool, ch: char) -> char {
    if set { ch } else { '-' }
}

fn to_file_entry(
    name: String,
    path: String,
    file_type: FileType,
    size: u64,
    modified: Option<std::time::SystemTime>,
    permissions: String,
) -> FileEntry {
    FileEntry {
        name,
        path,
        kind: match file_type {
            FileType::Dir => FileKind::Directory,
            FileType::Symlink => FileKind::Symlink,
            FileType::File => FileKind::File,
            FileType::Other => FileKind::Other,
        },
        size,
        modified_at: modified
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64),
        permissions: Some(permissions),
    }
}

#[async_trait]
impl SftpEngine for RusshSftpEngine {
    async fn list_dir(&self, session: SessionId, path: &str) -> Result<Vec<FileEntry>, SftpError> {
        let sftp = self.open_sftp(session).await?;
        let mut entries = Vec::new();
        for dir_entry in sftp.read_dir(path).await? {
            let meta = dir_entry.metadata();
            let perms = meta.permissions();
            entries.push(to_file_entry(
                dir_entry.file_name(),
                dir_entry.path(),
                meta.file_type(),
                meta.len(),
                meta.modified().ok(),
                format!(
                    "{}{}{}{}{}{}{}{}{}",
                    flag(perms.owner_read, 'r'),
                    flag(perms.owner_write, 'w'),
                    flag(perms.owner_exec, 'x'),
                    flag(perms.group_read, 'r'),
                    flag(perms.group_write, 'w'),
                    flag(perms.group_exec, 'x'),
                    flag(perms.other_read, 'r'),
                    flag(perms.other_write, 'w'),
                    flag(perms.other_exec, 'x'),
                ),
            ));
        }
        sort_entries(&mut entries);
        Ok(entries)
    }

    async fn mkdir(&self, session: SessionId, path: &str) -> Result<(), SftpError> {
        let sftp = self.open_sftp(session).await?;
        sftp.create_dir(path).await?;
        Ok(())
    }

    async fn remove(&self, session: SessionId, path: &str) -> Result<(), SftpError> {
        let sftp = self.open_sftp(session).await?;
        // Stat first so we remove files and (empty) directories correctly.
        let meta = sftp.symlink_metadata(path).await?;
        if meta.file_type().is_dir() {
            sftp.remove_dir(path).await?;
        } else {
            sftp.remove_file(path).await?;
        }
        Ok(())
    }

    async fn rename(&self, session: SessionId, from: &str, to: &str) -> Result<(), SftpError> {
        let sftp = self.open_sftp(session).await?;
        sftp.rename(from, to).await?;
        Ok(())
    }

    async fn upload(&self, task: TransferTask, progress: ProgressSender) -> Result<(), SftpError> {
        let session = self.session_for_host(&task.host_id).await?;
        let sftp = self.open_sftp(session).await?;

        let mut local = tokio::fs::File::open(&task.local_path).await?;
        let total = local.metadata().await?.len();

        // Resume: if the remote file exists and is smaller, continue there.
        let remote_size = sftp
            .metadata(&task.remote_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let start = if remote_size < total { remote_size } else { 0 };
        if start > 0 {
            local.seek(std::io::SeekFrom::Start(start)).await?;
        }
        let mut remote = if start > 0 {
            let mut f = sftp
                .open_with_flags(&task.remote_path, OpenFlags::WRITE)
                .await?;
            f.seek(std::io::SeekFrom::Start(start)).await?;
            f
        } else {
            sftp.create(&task.remote_path).await?
        };

        let mut throttle = RusshSftpEngine::progress_state();
        let mut buf = vec![0u8; 256 * 1024];
        let mut done = start;
        loop {
            let n = local.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            remote.write_all(&buf[..n]).await?;
            done += n as u64;
            throttle.maybe_emit(&progress, &task.id, done, Some(total), false);
        }
        remote.flush().await?;
        throttle.maybe_emit(&progress, &task.id, done, Some(total), true);
        tracing::info!("upload complete: {} ({} bytes)", task.remote_path, done);
        Ok(())
    }

    async fn download(
        &self,
        task: TransferTask,
        progress: ProgressSender,
    ) -> Result<(), SftpError> {
        let session = self.session_for_host(&task.host_id).await?;
        let sftp = self.open_sftp(session).await?;

        let total = sftp.metadata(&task.remote_path).await.map(|m| m.len()).ok();

        // Resume: if the local file exists and is smaller, continue there.
        let local_size = tokio::fs::metadata(&task.local_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let start = match total {
            Some(t) if local_size < t => local_size,
            _ if local_size > 0 && total.is_none() => 0, // unknown total: restart to be safe
            _ => 0,
        };
        let mut local = if start > 0 {
            let mut f = tokio::fs::OpenOptions::new()
                .write(true)
                .open(&task.local_path)
                .await?;
            f.seek(std::io::SeekFrom::Start(start)).await?;
            f
        } else {
            tokio::fs::File::create(&task.local_path).await?
        };
        let mut remote = sftp
            .open_with_flags(&task.remote_path, OpenFlags::READ)
            .await?;
        if start > 0 {
            remote.seek(std::io::SeekFrom::Start(start)).await?;
        }

        let mut throttle = RusshSftpEngine::progress_state();
        let mut buf = vec![0u8; 256 * 1024];
        let mut done = start;
        loop {
            let n = remote.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            local.write_all(&buf[..n]).await?;
            done += n as u64;
            throttle.maybe_emit(&progress, &task.id, done, total, false);
        }
        local.flush().await?;
        throttle.maybe_emit(&progress, &task.id, done, total, true);
        tracing::info!("download complete: {} ({} bytes)", task.local_path, done);
        Ok(())
    }
}

/// Convenience: direction of a [`TransferTask`] or `None` when malformed.
pub fn task_direction(task: &TransferTask) -> Option<TransferDirection> {
    TransferDirection::parse(&task.direction)
}
