//! Transfer queue: persistent tasks, pause / resume / cancel (docs §6.6).
//!
//! ```text
//! enqueue() → persist (SQLite) → worker picks task → engine upload/download
//!   → progress events → UI progress bar → completed / failed
//! ```
//!
//! Pause aborts the worker and marks the task paused; resume re-enqueues it
//! and the engine continues from the partial file (see [`crate::engine`]).
//!
//! Author: Passwd Word

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use opsdeck_storage::Storage;
use opsdeck_storage::models::{TransferDirection, TransferStatus, TransferTask, new_id, now_unix};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

use crate::engine::{ProgressSender, SftpEngine, TransferProgress, task_direction};
use crate::error::SftpError;

/// Queue-level events for the UI.
#[derive(Debug, Clone)]
pub enum QueueEvent {
    Progress {
        task_id: String,
        bytes_done: u64,
        bytes_total: Option<u64>,
    },
    Finished {
        task_id: String,
        result: Result<(), String>,
    },
}

/// Persistent, pausable transfer queue.
pub struct TransferQueue {
    engine: Arc<dyn SftpEngine>,
    storage: Storage,
    workers: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    event_tx: mpsc::UnboundedSender<QueueEvent>,
}

impl TransferQueue {
    /// Create a queue. `event_tx` receives [`QueueEvent`]s for the UI.
    pub fn new(
        engine: Arc<dyn SftpEngine>,
        storage: Storage,
        event_tx: mpsc::UnboundedSender<QueueEvent>,
    ) -> Self {
        Self {
            engine,
            storage,
            workers: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }

    /// Build a new task record (not yet persisted / started).
    pub fn new_task(
        host_id: &str,
        direction: TransferDirection,
        local_path: &str,
        remote_path: &str,
    ) -> TransferTask {
        let now = now_unix();
        TransferTask {
            id: new_id(),
            host_id: host_id.to_string(),
            direction: direction.as_str().to_string(),
            local_path: local_path.to_string(),
            remote_path: remote_path.to_string(),
            status: TransferStatus::Pending.as_str().to_string(),
            bytes_total: None,
            bytes_done: 0,
            error: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Persist a task and start its worker.
    pub async fn enqueue(&self, task: TransferTask) -> Result<String, SftpError> {
        let id = task.id.clone();
        self.storage.transfers().create(&task).await?;
        self.spawn_worker(task).await;
        Ok(id)
    }

    /// Re-enqueue an existing (paused/failed) task: resumes from partial data.
    pub async fn retry(&self, task_id: &str) -> Result<(), SftpError> {
        let task = self.storage.transfers().get(task_id).await?;
        self.storage
            .transfers()
            .update_progress(
                task_id,
                TransferStatus::Pending,
                task.bytes_done,
                task.bytes_total,
                None,
            )
            .await?;
        self.spawn_worker(task).await;
        Ok(())
    }

    /// Pause a running task (aborts the worker; partial data is kept).
    pub async fn pause(&self, task_id: &str) -> Result<(), SshPauseOutcome> {
        self.abort_worker(task_id).await;
        let task = self.storage.transfers().get(task_id).await?;
        self.storage
            .transfers()
            .update_progress(
                task_id,
                TransferStatus::Paused,
                task.bytes_done,
                task.bytes_total,
                None,
            )
            .await?;
        Ok(SshPauseOutcome::Paused)
    }

    /// Cancel a running task.
    pub async fn cancel(&self, task_id: &str) -> Result<(), SftpError> {
        self.abort_worker(task_id).await;
        let task = self.storage.transfers().get(task_id).await?;
        self.storage
            .transfers()
            .update_progress(
                task_id,
                TransferStatus::Canceled,
                task.bytes_done,
                task.bytes_total,
                None,
            )
            .await?;
        Ok(())
    }

    /// After an app restart: tasks stuck in `running` become `paused`
    /// (their workers are gone); `pending`/`paused` stay queued.
    pub async fn restore_after_restart(&self) -> Result<(), SftpError> {
        for task in self.storage.transfers().list_unfinished().await? {
            if task.status == TransferStatus::Running.as_str() {
                self.storage
                    .transfers()
                    .update_progress(
                        &task.id,
                        TransferStatus::Paused,
                        task.bytes_done,
                        task.bytes_total,
                        None,
                    )
                    .await?;
            }
        }
        Ok(())
    }

    /// Whether a worker is currently running for the task.
    pub async fn is_running(&self, task_id: &str) -> bool {
        self.workers.lock().await.contains_key(task_id)
    }

    async fn abort_worker(&self, task_id: &str) {
        if let Some(handle) = self.workers.lock().await.remove(task_id) {
            handle.abort();
        }
    }

    async fn spawn_worker(&self, task: TransferTask) {
        let id = task.id.clone();
        let engine = self.engine.clone();
        let storage = self.storage.clone();
        let event_tx = self.event_tx.clone();
        let workers = self.workers.clone();
        let done_counter = Arc::new(AtomicU64::new(task.bytes_done as u64));

        let handle = tokio::spawn(async move {
            storage
                .transfers()
                .update_progress(&id, TransferStatus::Running, 0, None, None)
                .await
                .ok();

            let (ptx, mut prx) = mpsc::unbounded_channel::<TransferProgress>();
            let fwd_storage = storage.clone();
            let fwd_events = event_tx.clone();
            let fwd_done = done_counter.clone();
            let fwd_id = id.clone();
            let forwarder = tokio::spawn(async move {
                while let Some(p) = prx.recv().await {
                    fwd_done.store(p.bytes_done, Ordering::Relaxed);
                    fwd_storage
                        .transfers()
                        .update_progress(
                            &p.task_id,
                            TransferStatus::Running,
                            p.bytes_done as i64,
                            p.bytes_total.map(|b| b as i64),
                            None,
                        )
                        .await
                        .ok();
                    let _ = fwd_events.send(QueueEvent::Progress {
                        task_id: p.task_id,
                        bytes_done: p.bytes_done,
                        bytes_total: p.bytes_total,
                    });
                }
            });

            let direction = task_direction(&task);
            let result: Result<(), SftpError> = match direction {
                Some(TransferDirection::Upload) => engine.upload(task.clone(), ptx.clone()).await,
                Some(TransferDirection::Download) => {
                    engine.download(task.clone(), ptx.clone()).await
                }
                None => Err(SftpError::Io(format!("bad direction: {}", task.direction))),
            };

            drop(ptx);
            let _ = forwarder.await;
            let done = done_counter.load(Ordering::Relaxed) as i64;

            // Don't overwrite a pause/cancel that raced us.
            let current = storage.transfers().get(&id).await.ok();
            let raced = current.as_ref().is_some_and(|t| {
                t.status == TransferStatus::Paused.as_str()
                    || t.status == TransferStatus::Canceled.as_str()
            });
            if !raced {
                let (status, err) = match &result {
                    Ok(()) => (TransferStatus::Completed, None),
                    Err(e) => (TransferStatus::Failed, Some(e.to_string())),
                };
                storage
                    .transfers()
                    .update_progress(
                        &id,
                        status,
                        done,
                        current.and_then(|t| t.bytes_total),
                        err.as_deref(),
                    )
                    .await
                    .ok();
                let _ = event_tx.send(QueueEvent::Finished {
                    task_id: id.clone(),
                    result: result.map_err(|e| e.to_string()),
                });
            }
            workers.lock().await.remove(&id);
        });

        self.workers.lock().await.insert(id, handle);
    }
}

/// Outcome of [`TransferQueue::pause`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshPauseOutcome {
    Paused,
}

/// Build a [`ProgressSender`] pair for tests / manual engine use.
pub fn progress_channel() -> (ProgressSender, mpsc::UnboundedReceiver<TransferProgress>) {
    mpsc::unbounded_channel()
}
