//! [`OpsDeckApp`]: eframe application wiring every crate together.
//!
//! The app owns all backend state (sessions, shells, transfers, metrics) and
//! translates [`UiEvent`][opsdeck_ui::UiEvent]s from the UI into backend work.
//! Background tasks talk back through an [`AppMsg`] channel drained each frame.
//!
//! Author: Passwd Word

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use opsdeck_ops::{MetricsCollector, SnippetService, batch};
use opsdeck_sftp::{FileEntry, QueueEvent, RusshSftpEngine, SftpEngine, TransferQueue};
use opsdeck_ssh::{
    CredentialProvider, HostKeyDecision, HostKeyPrompt, LocalForward, SessionId, SessionInfo,
    SessionManager, ShellChannel, ShellCommand, ShellEvent, SshError,
};
use opsdeck_storage::Storage;
use opsdeck_storage::models::{
    AuthMethod, Credential, CredentialKind, ExportData, Host, HostConfig, MetricSnapshot,
    TransferDirection, TransferTask, now_unix,
};
use opsdeck_sync::{FileRemote, HttpRemote, Remote, SyncEngine, SyncSummary, device_id};
use opsdeck_ui::{
    ForwardView, KeyPromptView, Language, MainTab, Screen, SftpListing, SyncStatusView, TermTab,
    Theme, UiData, UiEvent, UiState, UpdateStatusView,
};
use opsdeck_vault::{EncryptedBlob, SecretBytes, SecretString, Vault, VaultError, VaultVerifier};
use tokio::sync::{RwLock, mpsc, oneshot};

use crate::update::{UpdateInfo, check_update};

// ---------------------------------------------------------------------------
// Background messages
// ---------------------------------------------------------------------------

/// Messages from background tasks to the UI thread.
enum AppMsg {
    Connected {
        host_id: String,
        session: SessionId,
        shell: ShellChannel,
        open_tab: bool,
        split_into: Option<(usize, bool)>,
    },
    ConnectFailed {
        host_id: String,
        error: String,
    },
    DesktopConnected {
        id: u64,
        host_id: String,
        name: String,
        kind: String,
        handle: opsdeck_desktop::DesktopHandle,
    },
    Reconnected {
        old: SessionId,
        new: SessionId,
        host_id: String,
        shell: ShellChannel,
    },
    SessionList(Vec<SessionInfo>),
    Latencies(HashMap<String, Option<u32>>),
    TestResult(Result<u32, String>),
    /// A host save finished; connect to it now (Save and connect).
    HostSavedConnect(String),
    SysInfo {
        host_id: String,
        info: opsdeck_ops::SysInfo,
    },
    SftpListed {
        host_id: String,
        path: String,
        result: Result<Vec<FileEntry>, String>,
    },
    SftpOpDone {
        host_id: String,
        result: Result<(), String>,
        what: String,
    },
    BatchDone {
        results: Vec<(String, String)>,
    },
    MetricUpdate {
        host_id: String,
        snapshot: MetricSnapshot,
        processes: Vec<opsdeck_ops::ProcessInfo>,
    },
    ForwardStarted {
        id: usize,
        session: SessionId,
        local: String,
        remote: String,
        forward: LocalForward,
    },
    ForwardFailed {
        error: String,
    },
    SyncDone {
        result: Result<SyncSummary, String>,
    },
    RekeyDone {
        result: Result<(), String>,
    },
    UpdateDone {
        result: Result<UpdateInfo, String>,
    },
}

struct PendingPrompt {
    id: u64,
    view: KeyPromptView,
    responder: oneshot::Sender<HostKeyDecision>,
}

struct RunningForward {
    id: usize,
    session_name: String,
    local: String,
    remote: String,
    forward: LocalForward,
}

#[derive(Default)]
struct CachedData {
    hosts: Vec<Host>,
    groups: Vec<opsdeck_storage::models::HostGroup>,
    snippets: Vec<opsdeck_storage::models::Snippet>,
    credentials: Vec<opsdeck_storage::models::CredentialMeta>,
    transfers: Vec<TransferTask>,
    refreshed: Option<Instant>,
}

// ---------------------------------------------------------------------------
// Credential provider (vault-backed)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AppCredentials {
    storage: Storage,
    vault: Arc<RwLock<Option<Arc<Vault>>>>,
}

impl AppCredentials {
    async fn secret_for(&self, credential_id: &str) -> Result<Vec<u8>, SshError> {
        let cred = self
            .storage
            .credentials()
            .get(credential_id)
            .await
            .map_err(|e| SshError::Auth(format!("credential not found: {e}")))?;
        let vault = self.vault.read().await.clone();
        let vault = vault
            .as_ref()
            .ok_or_else(|| SshError::Auth("vault is locked".into()))?;
        let blob = EncryptedBlob {
            nonce: cred
                .nonce
                .try_into()
                .map_err(|_| SshError::Auth("bad nonce".into()))?,
            ciphertext: cred.ciphertext,
        };
        vault
            .decrypt(&blob)
            .map_err(|e| SshError::Auth(format!("decrypt credential: {e}")))
    }
}

#[async_trait]
impl CredentialProvider for AppCredentials {
    async fn password_for(&self, config: &HostConfig) -> Result<SecretString, SshError> {
        // The credential id lives on the Host row (AuthMethod::Password has no payload).
        let host = self
            .storage
            .hosts()
            .get(&config.id.to_string())
            .await
            .map_err(|e| SshError::Auth(format!("host not found: {e}")))?;
        let cred_id = host
            .credential_id
            .ok_or_else(|| SshError::Auth("no password credential configured".into()))?;
        let bytes = self.secret_for(&cred_id).await?;
        let s = String::from_utf8(bytes)
            .map_err(|_| SshError::Auth("credential is not text".into()))?;
        Ok(SecretString::new(s))
    }

    async fn private_key_for(
        &self,
        config: &HostConfig,
    ) -> Result<(SecretBytes, Option<SecretString>), SshError> {
        let (key_id, passphrase_id) = match &config.auth {
            AuthMethod::PrivateKey {
                key_id,
                passphrase_id,
            } => (*key_id, *passphrase_id),
            _ => return Err(SshError::Auth("host is not configured for key auth".into())),
        };
        let bytes = self.secret_for(&key_id.to_string()).await?;
        let passphrase = match passphrase_id {
            Some(pid) => {
                let bytes = self.secret_for(&pid.to_string()).await?;
                Some(SecretString::new(String::from_utf8(bytes).map_err(
                    |_| SshError::Auth("passphrase is not text".into()),
                )?))
            }
            None => None,
        };
        Ok((SecretBytes::new(bytes), passphrase))
    }
}

// ---------------------------------------------------------------------------
// The app
// ---------------------------------------------------------------------------

pub struct OpsDeckApp {
    state: UiState,
    storage: Storage,
    ssh: Arc<SessionManager>,
    creds: Arc<AppCredentials>,
    vault: Arc<RwLock<Option<Arc<Vault>>>>,
    verifier_path: PathBuf,

    msg_tx: mpsc::UnboundedSender<AppMsg>,
    msg_rx: mpsc::UnboundedReceiver<AppMsg>,
    prompt_rx: mpsc::UnboundedReceiver<HostKeyPrompt>,
    pending_prompts: Vec<PendingPrompt>,
    next_prompt_id: u64,

    shells: HashMap<SessionId, ShellChannel>,
    host_sessions: HashMap<String, SessionId>,
    session_names: HashMap<SessionId, String>,
    connecting: HashMap<String, String>,
    sessions: Vec<SessionInfo>,

    sftp: Arc<RusshSftpEngine>,
    transfers: Arc<TransferQueue>,
    transfer_rx: mpsc::UnboundedReceiver<QueueEvent>,
    sftp_listings: HashMap<String, SftpListing>,

    metric_tx: mpsc::UnboundedSender<AppMsg>,
    monitor_tasks: HashMap<String, tokio::task::AbortHandle>,
    metrics: HashMap<String, Vec<MetricSnapshot>>,
    processes: HashMap<String, Vec<opsdeck_ops::ProcessInfo>>,
    metrics_collector: MetricsCollector,

    forwards: Vec<RunningForward>,
    next_forward_id: usize,

    theme: Theme,
    update_view: UpdateStatusView,
    sync_view: SyncStatusView,
    cached: CachedData,
    cache_dirty: bool,
    activity: bool,
    /// A vault file exists on disk (verifier present).
    vault_exists: bool,
    /// The vault is currently unlocked (mirrored into UiState per frame).
    vault_unlocked: bool,
    /// Host to connect once its save has landed in the cache.
    pending_connect: Option<String>,
    /// Recently closed sessions shown with a "disconnected" notice until the
    /// user closes the tab (session → (info, closed_at)).
    recently_closed: HashMap<SessionId, (SessionInfo, Instant)>,
    /// sessions + recently_closed, rebuilt each frame for UiData.
    session_views: Vec<SessionInfo>,
    /// TCP connect latency cache (host id -> ms, None = unreachable).
    latencies: HashMap<String, Option<u32>>,
    ping_in_flight: bool,
    /// Periodic keepalive-command tasks, keyed by host id.
    keepalive_tasks: HashMap<String, tokio::task::AbortHandle>,
    /// Sessions opened through Telnet (not in the SSH manager).
    telnet_sessions: std::collections::HashSet<SessionId>,
    /// SessionInfo entries for non-SSH sessions (merged into session_views).
    extra_sessions: Vec<SessionInfo>,
    /// Running remote-desktop sessions + their UiData views.
    desktops: HashMap<u64, opsdeck_desktop::DesktopHandle>,
    desktop_views: Vec<opsdeck_ui::DesktopView>,
    next_desktop_id: u64,
    /// Static host facts per host id (OS / kernel / uptime).
    sysinfo: HashMap<String, opsdeck_ops::SysInfo>,
}

impl OpsDeckApp {
    /// Async constructor (called via `runtime.block_on` before eframe starts).
    pub async fn new_async() -> anyhow::Result<Self> {
        let proj = directories::ProjectDirs::from("com", "PasswdWord", "OpsDeck")
            .ok_or_else(|| anyhow::anyhow!("cannot determine app directories"))?;
        let data_dir = proj.data_dir().to_path_buf();
        let config_dir = proj.config_dir().to_path_buf();
        std::fs::create_dir_all(&data_dir)?;
        std::fs::create_dir_all(&config_dir)?;

        let storage = Storage::open(&data_dir.join("opsdeck.db")).await?;

        // Seed built-in snippets on first run.
        {
            let svc = SnippetService::new(storage.clone());
            let _ = svc.seed_defaults().await;
        }

        let (prompt_tx, prompt_rx) = mpsc::unbounded_channel();
        let ssh = Arc::new(SessionManager::new(
            prompt_tx,
            &config_dir.join("known_hosts.json"),
        ));

        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        let (transfer_tx, transfer_rx) = mpsc::unbounded_channel();

        let sftp_engine = Arc::new(RusshSftpEngine::new(ssh.clone()));
        let transfers = Arc::new(TransferQueue::new(
            sftp_engine.clone() as Arc<dyn opsdeck_sftp::SftpEngine>,
            storage.clone(),
            transfer_tx,
        ));
        // Resume interrupted transfers as paused.
        let _ = transfers.restore_after_restart().await;

        let vault: Arc<RwLock<Option<Arc<Vault>>>> = Arc::new(RwLock::new(None));
        let creds = Arc::new(AppCredentials {
            storage: storage.clone(),
            vault: vault.clone(),
        });

        // Background session list poller.
        {
            let ssh = ssh.clone();
            let tx = msg_tx.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(2));
                loop {
                    interval.tick().await;
                    let list = ssh.list().await;
                    if tx.send(AppMsg::SessionList(list)).is_err() {
                        break;
                    }
                }
            });
        }

        // Theme.
        let theme = match storage.get_setting("ui.theme").await? {
            Some(json) => serde_json::from_str(&json).unwrap_or_else(|_| Theme::tokyo_night()),
            None => Theme::tokyo_night(),
        };

        // Language (default: Simplified Chinese).
        let language = storage
            .get_setting("ui.language")
            .await?
            .as_deref()
            .map(Language::from_code)
            .unwrap_or_default();

        // The vault never gates startup: the app opens on the main screen
        // and asks for the master password only when a credential is used.
        let verifier_path = config_dir.join("vault.json");
        let has_vault = verifier_path.exists();
        let mut state = UiState::new();
        state.screen = Screen::Main;
        state.theme_name = theme.name.clone();
        state.language = language;

        // Sync settings → view.
        let sync_kind = storage
            .get_setting("sync.kind")
            .await?
            .unwrap_or_else(|| "off".to_string());
        state.sync_kind = sync_kind.clone();
        state.sync_file_dir = storage
            .get_setting("sync.file_dir")
            .await?
            .unwrap_or_default();
        state.sync_http_endpoint = storage
            .get_setting("sync.http_endpoint")
            .await?
            .unwrap_or_default();
        let sync_view = SyncStatusView {
            running: false,
            last_sync: None,
            last_summary: None,
            error: None,
            configured: sync_kind != "off",
        };

        let mut app = Self {
            state,
            storage,
            ssh: ssh.clone(),
            creds,
            vault,
            verifier_path,
            msg_tx: msg_tx.clone(),
            msg_rx,
            prompt_rx,
            pending_prompts: Vec::new(),
            next_prompt_id: 1,
            shells: HashMap::new(),
            host_sessions: HashMap::new(),
            session_names: HashMap::new(),
            connecting: HashMap::new(),
            sessions: Vec::new(),
            sftp: sftp_engine,
            transfers,
            transfer_rx,
            sftp_listings: HashMap::new(),
            metric_tx: msg_tx,
            monitor_tasks: HashMap::new(),
            metrics: HashMap::new(),
            processes: HashMap::new(),
            metrics_collector: MetricsCollector::new(ssh.clone()),
            forwards: Vec::new(),
            next_forward_id: 1,
            theme,
            update_view: UpdateStatusView {
                current: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            },
            sync_view,
            cached: CachedData::default(),
            cache_dirty: true,
            activity: false,
            vault_exists: has_vault,
            vault_unlocked: false,
            pending_connect: None,
            recently_closed: HashMap::new(),
            session_views: Vec::new(),
            latencies: HashMap::new(),
            ping_in_flight: false,
            keepalive_tasks: HashMap::new(),
            telnet_sessions: std::collections::HashSet::new(),
            extra_sessions: Vec::new(),
            desktops: HashMap::new(),
            desktop_views: Vec::new(),
            next_desktop_id: 1,
            sysinfo: HashMap::new(),
        };
        app.refresh_cache().await;
        let _ = device_id(&app.storage).await; // ensure a stable device id exists
        Ok(app)
    }

    // -- cache ---------------------------------------------------------------

    async fn refresh_cache(&mut self) {
        let hosts = self.storage.hosts().list_all().await.unwrap_or_default();
        let groups = self.storage.groups().list_all().await.unwrap_or_default();
        let snippets = self.storage.snippets().list_all().await.unwrap_or_default();
        let credentials = self
            .storage
            .credentials()
            .list_meta()
            .await
            .unwrap_or_default();
        let transfers = self
            .storage
            .transfers()
            .list_recent(200)
            .await
            .unwrap_or_default();
        self.cached = CachedData {
            hosts,
            groups,
            snippets,
            credentials,
            transfers,
            refreshed: Some(Instant::now()),
        };
        self.cache_dirty = false;
    }

    // -- per-frame -----------------------------------------------------------

    fn drain_messages(&mut self, ctx: &egui::Context) {
        // App messages.
        while let Ok(msg) = self.msg_rx.try_recv() {
            self.activity = true;
            self.on_app_msg(ctx, msg);
        }
        // Host-key prompts.
        while let Ok(prompt) = self.prompt_rx.try_recv() {
            self.activity = true;
            let id = self.next_prompt_id;
            self.next_prompt_id += 1;
            self.pending_prompts.push(PendingPrompt {
                id,
                view: KeyPromptView {
                    id,
                    host: prompt.host,
                    port: prompt.port,
                    key_type: prompt.key_type,
                    key_openssh: prompt.key_openssh,
                    changed: prompt.changed,
                },
                responder: prompt.responder,
            });
        }
        // Shell output.
        let mut closed = Vec::new();
        for (session, shell) in self.shells.iter_mut() {
            while let Ok(ev) = shell.events.try_recv() {
                self.activity = true;
                match ev {
                    ShellEvent::Data(bytes) => {
                        self.state.feed_terminal(*session, &bytes);
                    }
                    ShellEvent::Closed => {
                        closed.push(*session);
                    }
                    ShellEvent::Error(e) => {
                        tracing::warn!("shell {session} error: {e}");
                        closed.push(*session);
                    }
                }
            }
        }
        for session in closed {
            self.on_shell_closed(session);
        }
        // Transfer queue events.
        while let Ok(ev) = self.transfer_rx.try_recv() {
            self.activity = true;
            match ev {
                QueueEvent::Progress {
                    task_id,
                    bytes_done,
                    ..
                } => {
                    if let Some(t) = self.cached.transfers.iter_mut().find(|t| t.id == task_id) {
                        t.bytes_done = bytes_done as i64;
                    }
                }
                QueueEvent::Finished { task_id, result } => {
                    match result {
                        Ok(()) => self.state.toast(
                            self.state
                                .tf("Transfer {task_id} done", &[("task_id", &task_id)]),
                            false,
                        ),
                        Err(e) => self.state.toast(
                            self.state
                                .tf("Transfer failed: {e}", &[("e", &e.to_string())]),
                            true,
                        ),
                    }
                    self.cache_dirty = true;
                }
            }
        }
    }

    fn on_shell_closed(&mut self, session: SessionId) {
        self.shells.remove(&session);
        let name = self
            .session_names
            .remove(&session)
            .unwrap_or_else(|| "session".to_string());
        let host_id = self
            .host_sessions
            .iter()
            .find(|(_, s)| **s == session)
            .map(|(h, _)| h.clone());
        // Mark the ssh session disconnected so the UI shows the notice.
        let ssh = self.ssh.clone();
        let sftp = self.sftp.clone();
        tokio::spawn(async move {
            let _ = ssh.disconnect(session).await;
            if let Some(h) = host_id {
                sftp.unlink_host(&h).await;
            }
        });
        self.host_sessions.retain(|_, s| *s != session);
        self.recently_closed.insert(
            session,
            (
                SessionInfo {
                    id: session,
                    name: name.clone(),
                    host: String::new(),
                    port: 0,
                    state: opsdeck_ssh::SessionState::Disconnected,
                    last_error: Some(self.state.t("Connection closed.").to_string()),
                    via: None,
                },
                Instant::now(),
            ),
        );
        self.state.toast(
            self.state
                .tf("{name}: connection closed", &[("name", &name)]),
            false,
        );
    }

    #[allow(clippy::too_many_lines)]
    fn on_app_msg(&mut self, _ctx: &egui::Context, msg: AppMsg) {
        match msg {
            AppMsg::Connected {
                host_id,
                session,
                shell,
                open_tab,
                split_into,
            } => {
                self.connecting.remove(&host_id);
                let name = self
                    .cached
                    .hosts
                    .iter()
                    .find(|h| h.id == host_id)
                    .map(|h| h.name.clone())
                    .unwrap_or_else(|| "session".to_string());
                let cmd_tx = shell.commands.clone();
                self.shells.insert(session, shell);
                let ssh_only = !self.telnet_sessions.contains(&session);
                // Init sequence: default path, then the startup command.
                if ssh_only && let Some(host) = self.cached.hosts.iter().find(|h| h.id == host_id) {
                    let mut init = String::new();
                    if let Some(path) = host
                        .default_path
                        .as_deref()
                        .filter(|p| !p.trim().is_empty())
                    {
                        init.push_str(&format!("cd {path}\n"));
                    }
                    if let Some(cmd) = host
                        .startup_command
                        .as_deref()
                        .filter(|c| !c.trim().is_empty())
                    {
                        init.push_str(cmd.trim_end());
                        init.push('\n');
                    }
                    if !init.is_empty() {
                        let _ = cmd_tx.send(ShellCommand::Write(init.into_bytes()));
                    }
                    // Periodic keepalive command (exec channel, invisible).
                    if let (Some(cmd), Some(secs)) = (
                        host.keepalive_cmd.clone().filter(|c| !c.trim().is_empty()),
                        host.keepalive_cmd_secs.filter(|s| *s > 0),
                    ) {
                        if let Some(old) = self.keepalive_tasks.remove(&host.id) {
                            old.abort();
                        }
                        let ssh = self.ssh.clone();
                        let handle = tokio::spawn(async move {
                            let mut tick =
                                tokio::time::interval(std::time::Duration::from_secs(secs as u64));
                            tick.tick().await; // first run after one period
                            loop {
                                tick.tick().await;
                                let _ = ssh
                                    .exec_timeout(session, &cmd, std::time::Duration::from_secs(15))
                                    .await;
                            }
                        })
                        .abort_handle();
                        self.keepalive_tasks.insert(host_id.clone(), handle);
                    }
                }
                self.host_sessions.insert(host_id.clone(), session);
                // SSH-only integrations: monitoring, sysinfo, SFTP, init cmds.
                if !self.telnet_sessions.contains(&session) {
                    // Monitoring: start polling + fetch static host facts once.
                    self.ensure_monitor_task(&host_id);
                    {
                        let collector = self.metrics_collector.clone();
                        let tx = self.msg_tx.clone();
                        let hid = host_id.clone();
                        tokio::spawn(async move {
                            if let Ok(info) = collector.sysinfo(session).await {
                                let _ = tx.send(AppMsg::SysInfo { host_id: hid, info });
                            }
                        });
                    }
                }
                self.session_names.insert(session, name.clone());
                // Link for SFTP (SSH only).
                if !self.telnet_sessions.contains(&session) {
                    let sftp = self.sftp.clone();
                    tokio::spawn(async move {
                        sftp.link_host(&host_id, session).await;
                    });
                }
                // Place the pane.
                if let Some((tab_idx, vertical)) = split_into {
                    if let Some(tab) = self.state.tabs.get_mut(tab_idx) {
                        tab.vertical = vertical;
                        tab.panes.push(opsdeck_ui::Pane {
                            session,
                            host_id: String::new(),
                            title: name.clone(),
                        });
                        tab.active_pane = tab.panes.len() - 1;
                    }
                } else if open_tab {
                    self.state
                        .tabs
                        .push(TermTab::single(session, String::new(), name.clone()));
                    self.state.active_tab = self.state.tabs.len() - 1;
                    self.state.main_tab = MainTab::Terminal;
                }
                // Record host_id on the pane. (placed below)
                for tab in &mut self.state.tabs {
                    for pane in &mut tab.panes {
                        if pane.session == session {
                            pane.host_id = self
                                .host_sessions
                                .iter()
                                .find(|(_, s)| **s == session)
                                .map(|(h, _)| h.clone())
                                .unwrap_or_default();
                        }
                    }
                }
                self.state.toast(
                    self.state.tf("Connected to {name}", &[("name", &name)]),
                    false,
                );
            }
            AppMsg::DesktopConnected {
                id,
                host_id,
                name,
                kind,
                handle,
            } => {
                self.connecting.remove(&host_id);
                self.desktop_views.push(opsdeck_ui::DesktopView {
                    id,
                    host_id: host_id.clone(),
                    name: name.clone(),
                    kind: kind.clone(),
                    frame: handle.frame.clone(),
                });
                self.desktops.insert(id, handle);
                self.state.desktops.push(opsdeck_ui::DesktopTab {
                    id,
                    host_id,
                    title: name,
                    kind,
                });
                self.state.active_desktop = Some(id);
                self.state.main_tab = MainTab::Terminal;
            }
            AppMsg::ConnectFailed { host_id, error } => {
                self.connecting.remove(&host_id);
                self.state.toast(
                    self.state
                        .tf("Connection failed: {error}", &[("error", &error)]),
                    true,
                );
            }
            AppMsg::Reconnected {
                old,
                new,
                host_id,
                shell,
            } => {
                // Move terminal state to the new session id.
                if let Some(term) = self.state.terminals.remove(&old) {
                    self.state.terminals.insert(new, term);
                }
                if let Some(size) = self.state.term_sizes.remove(&old) {
                    self.state.term_sizes.insert(new, size);
                }
                self.shells.remove(&old);
                self.shells.insert(new, shell);
                self.host_sessions.insert(host_id.clone(), new);
                if let Some(name) = self.session_names.remove(&old) {
                    self.session_names.insert(new, name);
                }
                for tab in &mut self.state.tabs {
                    for pane in &mut tab.panes {
                        if pane.session == old {
                            pane.session = new;
                        }
                    }
                }
                let sftp = self.sftp.clone();
                tokio::spawn(async move {
                    sftp.link_host(&host_id, new).await;
                });
                self.state.toast(self.state.t("Reconnected"), false);
            }
            AppMsg::SessionList(list) => {
                self.sessions = list;
            }
            AppMsg::Latencies(map) => {
                self.latencies = map;
                self.ping_in_flight = false;
                self.cache_dirty = true;
            }
            AppMsg::SysInfo { host_id, info } => {
                self.sysinfo.insert(host_id, info);
            }
            AppMsg::HostSavedConnect(host_id) => {
                self.pending_connect = Some(host_id);
                self.cache_dirty = true;
            }
            AppMsg::TestResult(result) => match result {
                Ok(ms) => {
                    self.state.toast(
                        self.state
                            .tf("Connection OK ({ms} ms)", &[("ms", &ms.to_string())]),
                        false,
                    );
                }
                Err(e) => {
                    self.state.toast(
                        self.state
                            .tf("Connection failed: {error}", &[("error", &e)]),
                        true,
                    );
                }
            },
            AppMsg::SftpListed {
                host_id,
                path,
                result,
            } => {
                let listing = self.sftp_listings.entry(host_id.clone()).or_default();
                listing.loading = false;
                match result {
                    Ok(entries) => {
                        listing.path = path.clone();
                        listing.entries = entries;
                        listing.error = None;
                        self.state.sftp_paths.insert(host_id, path);
                    }
                    Err(e) => {
                        listing.error = Some(e);
                    }
                }
            }
            AppMsg::SftpOpDone {
                host_id,
                result,
                what,
            } => {
                let op = self.state.t(match what.as_str() {
                    "mkdir" => "Create directory",
                    "delete" => "Delete",
                    "rename" => "Rename",
                    _ => "File operation",
                });
                match result {
                    Ok(()) => {
                        self.state
                            .toast(self.state.tf("SFTP {op} done", &[("op", op)]), false);
                        let path = self.state.sftp_path(&host_id);
                        self.navigate_sftp(&host_id, &path);
                    }
                    Err(e) => self.state.toast(
                        self.state.tf(
                            "SFTP {op} failed: {e}",
                            &[("op", op), ("e", &e.to_string())],
                        ),
                        true,
                    ),
                }
            }
            AppMsg::BatchDone { results } => {
                self.state.batch_output = results;
                self.state.toast(self.state.t("Batch run finished"), false);
            }
            AppMsg::MetricUpdate {
                host_id,
                snapshot,
                processes,
            } => {
                let entry = self.metrics.entry(host_id.clone()).or_default();
                entry.push(snapshot.clone());
                if entry.len() > 120 {
                    entry.drain(..entry.len() - 120);
                }
                self.processes.insert(host_id.clone(), processes);
                // Persist every snapshot (cheap, small table).
                let storage = self.storage.clone();
                tokio::spawn(async move {
                    let _ = storage.metrics().insert(&snapshot).await;
                });
            }
            AppMsg::ForwardStarted {
                id,
                session,
                local,
                remote,
                forward,
            } => {
                let session_name = self
                    .session_names
                    .get(&session)
                    .cloned()
                    .unwrap_or_else(|| "session".to_string());
                self.forwards.push(RunningForward {
                    id,
                    session_name,
                    local: local.clone(),
                    remote,
                    forward,
                });
                self.state.toast(
                    self.state.tf("Forwarding {local}", &[("local", &local)]),
                    false,
                );
            }
            AppMsg::ForwardFailed { error } => {
                self.state.toast(
                    self.state
                        .tf("Port forward failed: {error}", &[("error", &error)]),
                    true,
                );
            }
            AppMsg::SyncDone { result } => {
                self.sync_view.running = false;
                match result {
                    Ok(summary) => {
                        let text = format!(
                            "↑{} ↓{} ({} applied, {} conflicts)",
                            summary.pushed, summary.pulled, summary.applied, summary.conflicts
                        );
                        self.sync_view.last_summary = Some(text);
                        self.sync_view.last_sync = Some(now_unix());
                        self.sync_view.error = None;
                        if !summary.errors.is_empty() {
                            self.sync_view.error = Some(summary.errors.join("; "));
                        }
                        self.cache_dirty = true;
                        self.state.toast(self.state.t("Sync complete"), false);
                    }
                    Err(e) => {
                        self.sync_view.error = Some(e.clone());
                        self.state.toast(
                            self.state.tf("Sync failed: {e}", &[("e", &e.to_string())]),
                            true,
                        );
                    }
                }
            }
            AppMsg::RekeyDone { result } => match result {
                Ok(()) => self
                    .state
                    .toast(self.state.t("Master password changed"), false),
                Err(e) => self.state.toast(
                    self.state
                        .tf("Password change failed: {e}", &[("e", &e.to_string())]),
                    true,
                ),
            },
            AppMsg::UpdateDone { result } => {
                self.update_view.checking = false;
                match result {
                    Ok(info) => {
                        self.update_view.available = info.available;
                        self.update_view.latest = Some(info.latest.clone());
                        self.update_view.url = Some(info.url.clone());
                        self.update_view.error = None;
                        if info.available {
                            self.state.toast(
                                self.state
                                    .tf("Update available: {latest}", &[("latest", &info.latest)]),
                                false,
                            );
                        } else {
                            self.state.toast(self.state.t("You're up to date"), false);
                        }
                    }
                    Err(e) => {
                        self.update_view.error = Some(e.to_string());
                    }
                }
            }
        }
    }

    // -- actions ---------------------------------------------------------------

    /// Connect to a host (resolving proxy jumps), then open a tab / split pane.
    /// Open a remote-desktop session (VNC today, RDP lands next).
    fn connect_desktop(&mut self, host: &Host) {
        if self.connecting.contains_key(&host.id) {
            return;
        }
        let id = self.next_desktop_id;
        self.next_desktop_id += 1;
        self.connecting
            .insert(host.id.clone(), "Connecting…".to_string());
        let creds = self.creds.clone();
        let cred_id = host.credential_id.clone();
        let host2 = host.clone();
        let tx = self.msg_tx.clone();
        tokio::spawn(async move {
            let password = match &cred_id {
                Some(cid) => match creds.secret_for(cid).await {
                    Ok(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
                    Err(_) => None,
                },
                None => None,
            };
            let kind = host2.protocol.clone();
            let kind_for_worker = kind.clone();
            let name = host2.name.clone();
            let host_id = host2.id.clone();
            let timeout = std::time::Duration::from_secs(
                host2.connect_timeout_secs.filter(|v| *v > 0).unwrap_or(15) as u64,
            );
            let username = host2.username.clone();
            let res = tokio::task::spawn_blocking(move || {
                if kind_for_worker == "vnc" {
                    opsdeck_vnc::connect(
                        &host2.host,
                        host2.port as u16,
                        password.as_deref(),
                        timeout,
                    )
                    .map_err(|e| e.to_string())
                } else {
                    opsdeck_rdp::connect(
                        &host2.host,
                        host2.port as u16,
                        &username,
                        password.as_deref().unwrap_or(""),
                        timeout,
                    )
                    .map_err(|e| e.to_string())
                }
            })
            .await;
            match res {
                Ok(Ok(handle)) => {
                    let _ = tx.send(AppMsg::DesktopConnected {
                        id,
                        host_id,
                        name,
                        kind,
                        handle,
                    });
                }
                Ok(Err(e)) => {
                    let _ = tx.send(AppMsg::ConnectFailed { host_id, error: e });
                }
                Err(e) => {
                    let _ = tx.send(AppMsg::ConnectFailed {
                        host_id,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    /// Open a Telnet session: produces a real ShellChannel, so the terminal
    /// handles it exactly like an SSH shell.
    fn connect_telnet(&mut self, host: &Host, open_tab: bool, split_into: Option<(usize, bool)>) {
        let host_id = host.id.clone();
        if self.connecting.contains_key(&host_id) {
            return;
        }
        let session: SessionId = uuid::Uuid::new_v4();
        self.telnet_sessions.insert(session);
        self.extra_sessions.push(SessionInfo {
            id: session,
            name: format!("{} (telnet)", host.name),
            host: host.host.clone(),
            port: host.port as u16,
            state: opsdeck_ssh::SessionState::Ready,
            last_error: None,
            via: None,
        });
        self.connecting
            .insert(host_id.clone(), "Connecting…".to_string());
        let timeout = std::time::Duration::from_secs(
            host.connect_timeout_secs.filter(|v| *v > 0).unwrap_or(20) as u64,
        );
        let addr = (host.host.clone(), host.port as u16);
        let term = host.term_type.clone();
        let tx = self.msg_tx.clone();
        tokio::spawn(async move {
            let res = opsdeck_telnet::open(&addr.0, addr.1, 80, 24, term.as_deref(), timeout).await;
            match res {
                Ok(shell) => {
                    let _ = tx.send(AppMsg::Connected {
                        host_id,
                        session,
                        shell,
                        open_tab,
                        split_into,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppMsg::ConnectFailed {
                        host_id,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    fn connect_host(&mut self, host_id: &str, open_tab: bool, split_into: Option<(usize, bool)>) {
        let Some(host) = self.cached.hosts.iter().find(|h| h.id == host_id).cloned() else {
            self.state.toast(self.state.t("Host not found"), true);
            return;
        };
        // Stored credentials live in the vault: if it is locked, ask for the
        // master password instead of failing the dial with a cryptic error.
        let needs_vault = host.protocol != "telnet"
            && (host.credential_id.is_some() || host.passphrase_credential_id.is_some());
        if needs_vault && !self.vault_unlocked {
            self.state.toast(
                self.state.t("Vault is locked — unlock it to connect."),
                true,
            );
            self.state.show_vault_dialog = true;
            return;
        }
        if host.protocol == "telnet" {
            self.connect_telnet(&host, open_tab, split_into);
            return;
        }
        if host.protocol == "vnc" || host.protocol == "rdp" {
            self.connect_desktop(&host);
            return;
        }
        if self.connecting.contains_key(host_id) {
            return;
        }
        // Build the jump chain: jumps first, target last.
        let mut chain: Vec<HostConfig> = Vec::new();
        let mut current = Some(host.clone());
        let mut guard = 0;
        while let Some(h) = current {
            guard += 1;
            if guard > 8 {
                self.state.toast(self.state.t("Jump chain too deep"), true);
                return;
            }
            match h.to_config() {
                Ok(cfg) => chain.push(cfg),
                Err(e) => {
                    self.state.toast(
                        self.state
                            .tf("Bad host config: {e}", &[("e", &e.to_string())]),
                        true,
                    );
                    return;
                }
            }
            current = h
                .proxy_jump_id
                .as_deref()
                .and_then(|jid| self.cached.hosts.iter().find(|x| x.id == jid).cloned());
        }
        chain.reverse();

        self.connecting
            .insert(host_id.to_string(), "Connecting…".to_string());
        let ssh = self.ssh.clone();
        let creds = self.creds.clone();
        let tx = self.msg_tx.clone();
        let host_id = host_id.to_string();
        // Initial terminal size (updated on first render).
        let (cols, rows) = (80u16, 24u16);
        tokio::spawn(async move {
            match ssh.connect_chain(&chain, creds.as_ref()).await {
                Ok(session) => match ssh.open_shell(session, cols, rows).await {
                    Ok(shell) => {
                        let _ = tx.send(AppMsg::Connected {
                            host_id,
                            session,
                            shell,
                            open_tab,
                            split_into,
                        });
                    }
                    Err(e) => {
                        let _ = ssh.disconnect(session).await;
                        let _ = tx.send(AppMsg::ConnectFailed {
                            host_id,
                            error: format!("shell: {e}"),
                        });
                    }
                },
                Err(e) => {
                    let _ = tx.send(AppMsg::ConnectFailed {
                        host_id,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    /// Measure TCP connect latency for every known host (concurrent).
    fn spawn_ping(&mut self) {
        if self.ping_in_flight {
            return;
        }
        self.ping_in_flight = true;
        let hosts: Vec<(String, String, u16)> = self
            .cached
            .hosts
            .iter()
            .map(|h| (h.id.clone(), h.host.clone(), h.port as u16))
            .collect();
        let tx = self.msg_tx.clone();
        tokio::spawn(async move {
            let mut set = tokio::task::JoinSet::new();
            for (id, host, port) in hosts {
                set.spawn(async move {
                    let start = std::time::Instant::now();
                    let res = tokio::time::timeout(
                        std::time::Duration::from_millis(2500),
                        tokio::net::TcpStream::connect((host.as_str(), port)),
                    )
                    .await;
                    let ms = match res {
                        Ok(Ok(_stream)) => Some(start.elapsed().as_millis() as u32),
                        _ => None,
                    };
                    (id, ms)
                });
            }
            let mut out: HashMap<String, Option<u32>> = HashMap::new();
            while let Some(joined) = set.join_next().await {
                if let Ok((id, ms)) = joined {
                    out.insert(id, ms);
                }
            }
            let _ = tx.send(AppMsg::Latencies(out));
        });
    }

    fn disconnect_session(&mut self, session: SessionId) {
        if let Some(shell) = self.shells.remove(&session) {
            let _ = shell.commands.send(ShellCommand::Close);
        }
        if let Some((hid, _)) = self.host_sessions.iter().find(|(_, s)| **s == session) {
            let hid = hid.clone();
            if let Some(handle) = self.keepalive_tasks.remove(&hid) {
                handle.abort();
            }
            if let Some(handle) = self.monitor_tasks.remove(&hid) {
                handle.abort();
            }
        }
        let ssh = self.ssh.clone();
        let sftp = self.sftp.clone();
        let host_id = self
            .host_sessions
            .iter()
            .find(|(_, s)| **s == session)
            .map(|(h, _)| h.clone());
        tokio::spawn(async move {
            let _ = ssh.disconnect(session).await;
            if let Some(h) = host_id {
                sftp.unlink_host(&h).await;
            }
        });
        self.host_sessions.retain(|_, s| *s != session);
        self.session_names.remove(&session);
        self.state.drop_terminal(session);
    }

    fn navigate_sftp(&mut self, host_id: &str, path: &str) {
        let Some(session) = self.host_sessions.get(host_id).cloned() else {
            self.state
                .toast(self.state.t("Connect the host first (Terminal tab)"), true);
            return;
        };
        let listing = self.sftp_listings.entry(host_id.to_string()).or_default();
        listing.loading = true;
        listing.error = None;
        let sftp = self.sftp.clone();
        let tx = self.msg_tx.clone();
        let host_id = host_id.to_string();
        let path = path.to_string();
        tokio::spawn(async move {
            let result = sftp
                .list_dir(session, &path)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(AppMsg::SftpListed {
                host_id,
                path,
                result,
            });
        });
    }

    fn ensure_monitor_task(&mut self, host_id: &str) {
        if self.monitor_tasks.contains_key(host_id) {
            return;
        }
        let Some(session) = self.host_sessions.get(host_id).cloned() else {
            self.state
                .toast(self.state.t("Connect the host first (Terminal tab)"), true);
            return;
        };
        let collector = self.metrics_collector.clone();
        let tx = self.metric_tx.clone();
        let host_id = host_id.to_string();
        let monitor_key = host_id.clone();
        let storage = self.storage.clone();
        // Load recent history from the DB for the charts.
        {
            let storage = storage.clone();
            let tx = tx.clone();
            let host_id = host_id.clone();
            tokio::spawn(async move {
                if let Ok(history) = storage.metrics().recent_for_host(&host_id, 120).await {
                    for snap in history {
                        let _ = tx.send(AppMsg::MetricUpdate {
                            host_id: host_id.clone(),
                            snapshot: snap,
                            processes: Vec::new(),
                        });
                    }
                }
            });
        }
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                match collector.collect(session, &host_id).await {
                    Ok(snap) => {
                        let procs = collector.top_processes(session).await.unwrap_or_default();
                        if tx
                            .send(AppMsg::MetricUpdate {
                                host_id: host_id.clone(),
                                snapshot: snap,
                                processes: procs,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(e) => tracing::warn!("metrics poll failed for {host_id}: {e}"),
                }
            }
        });
        self.monitor_tasks
            .insert(monitor_key, handle.abort_handle());
    }

    // -- vault -----------------------------------------------------------------

    fn load_verifier(&self) -> Result<VaultVerifier, String> {
        let json = std::fs::read_to_string(&self.verifier_path)
            .map_err(|e| format!("read verifier: {e}"))?;
        serde_json::from_str(&json).map_err(|e| format!("parse verifier: {e}"))
    }

    fn save_verifier(&self, verifier: &VaultVerifier) -> Result<(), String> {
        let json = serde_json::to_string_pretty(verifier).map_err(|e| e.to_string())?;
        std::fs::write(&self.verifier_path, json).map_err(|e| format!("write verifier: {e}"))?;
        Ok(())
    }

    // -- event handling ----------------------------------------------------------

    #[allow(clippy::too_many_lines)]
    fn handle_event(&mut self, ctx: &egui::Context, event: UiEvent) {
        let _ = ctx;
        match event {
            // -- vault ------------------------------------------------------
            UiEvent::CreateVault(password) => match Vault::create(&SecretString::new(password)) {
                Ok((vault, verifier)) => match self.save_verifier(&verifier) {
                    Ok(()) => {
                        *self.vault.blocking_write() = Some(Arc::new(vault));
                        self.vault_exists = true;
                        self.vault_unlocked = true;
                        self.state.show_vault_dialog = false;
                        self.state.toast(self.state.t("Vault created"), false);
                    }
                    Err(e) => self.state.toast(
                        self.state
                            .tf("Cannot save vault: {e}", &[("e", &e.to_string())]),
                        true,
                    ),
                },
                Err(e) => self.state.toast(
                    self.state
                        .tf("Cannot create vault: {e}", &[("e", &e.to_string())]),
                    true,
                ),
            },
            UiEvent::UnlockVault(password) => {
                match self.load_verifier().and_then(|v| {
                    Vault::unlock(&SecretString::new(password), &v).map_err(|e| e.to_string())
                }) {
                    Ok(vault) => {
                        *self.vault.blocking_write() = Some(Arc::new(vault));
                        self.vault_unlocked = true;
                        self.state.show_vault_dialog = false;
                        self.state.unlock_error = None;
                    }
                    Err(_) => {
                        // Never leak why unlock failed.
                        self.state.unlock_error = Some(self.state.t("Wrong password.").to_string());
                    }
                }
            }
            UiEvent::LockVault => {
                // Disconnect everything first.
                let sessions: Vec<SessionId> = self.shells.keys().copied().collect();
                for s in sessions {
                    self.disconnect_session(s);
                }
                *self.vault.blocking_write() = None;
                self.vault_unlocked = false;
                self.state.show_vault_dialog = false;
                self.state.password_input.clear();
                self.state.toast(self.state.t("Vault locked"), false);
            }
            UiEvent::ChangeMasterPassword { old, new } => {
                // Rekey in the background: unlock with the old password, decrypt
                // every credential, re-encrypt with the new password, save the
                // new verifier. The UI stays responsive; the result arrives as
                // AppMsg::RekeyDone.
                let storage = self.storage.clone();
                let vault_arc = self.vault.clone();
                let verifier_path = self.verifier_path.clone();
                let tx = self.msg_tx.clone();
                let wrong_pw_msg = self.state.t("Current password is wrong").to_string();
                tokio::spawn(async move {
                    let result: Result<(), String> = async {
                        let json = std::fs::read_to_string(&verifier_path)
                            .map_err(|e| format!("read verifier: {e}"))?;
                        let verifier: VaultVerifier = serde_json::from_str(&json)
                            .map_err(|e| format!("parse verifier: {e}"))?;
                        let old_vault = Vault::unlock(&SecretString::new(old), &verifier)
                            .map_err(|_| wrong_pw_msg.clone())?;
                        // Decrypt everything with the OLD key first.
                        let metas = storage
                            .credentials()
                            .list_meta()
                            .await
                            .map_err(|e| e.to_string())?;
                        let mut plaintexts = Vec::with_capacity(metas.len());
                        for meta in &metas {
                            let cred = storage
                                .credentials()
                                .get(&meta.id)
                                .await
                                .map_err(|e| e.to_string())?;
                            let blob = EncryptedBlob {
                                nonce: cred
                                    .nonce
                                    .clone()
                                    .try_into()
                                    .map_err(|_| "bad nonce".to_string())?,
                                ciphertext: cred.ciphertext.clone(),
                            };
                            let pt = old_vault
                                .decrypt(&blob)
                                .map_err(|e| format!("decrypt: {e}"))?;
                            plaintexts.push((cred.id.clone(), pt));
                        }
                        // Derive the NEW key and re-encrypt.
                        let (new_vault, new_verifier) = old_vault
                            .rekey(&SecretString::new(new))
                            .map_err(|e| format!("rekey: {e}"))?;
                        for (id, pt) in &plaintexts {
                            let blob =
                                new_vault.encrypt(pt).map_err(|e| format!("encrypt: {e}"))?;
                            let mut cred = storage
                                .credentials()
                                .get(id)
                                .await
                                .map_err(|e| e.to_string())?;
                            cred.nonce = blob.nonce.to_vec();
                            cred.ciphertext = blob.ciphertext;
                            cred.updated_at = now_unix();
                            storage
                                .credentials()
                                .upsert(&cred)
                                .await
                                .map_err(|e| e.to_string())?;
                        }
                        let json = serde_json::to_string_pretty(&new_verifier)
                            .map_err(|e| e.to_string())?;
                        std::fs::write(&verifier_path, json)
                            .map_err(|e| format!("write verifier: {e}"))?;
                        *vault_arc.write().await = Some(Arc::new(new_vault));
                        Ok(())
                    }
                    .await;
                    let _ = tx.send(AppMsg::RekeyDone { result });
                });
                self.state.toast(self.state.t("Changing password…"), false);
            }

            // -- hosts ------------------------------------------------------
            UiEvent::ConnectHost(host_id) => self.connect_host(&host_id, true, None),
            UiEvent::NewTabForHost(host_id) => self.connect_host(&host_id, true, None),
            UiEvent::DisconnectSession(session) => self.disconnect_session(session),
            UiEvent::ReconnectSession(old) => {
                let creds = self.creds.clone();
                let ssh = self.ssh.clone();
                let tx = self.msg_tx.clone();
                let host_id = self
                    .host_sessions
                    .iter()
                    .find(|(_, s)| **s == old)
                    .map(|(h, _)| h.clone())
                    .unwrap_or_default();
                tokio::spawn(async move {
                    match ssh.reconnect(old, creds.as_ref()).await {
                        Ok(new) => match ssh.open_shell(new, 80, 24).await {
                            Ok(shell) => {
                                let _ = tx.send(AppMsg::Reconnected {
                                    old,
                                    new,
                                    host_id,
                                    shell,
                                });
                            }
                            Err(e) => {
                                let _ = tx.send(AppMsg::ConnectFailed {
                                    host_id,
                                    error: e.to_string(),
                                });
                            }
                        },
                        Err(e) => {
                            let _ = tx.send(AppMsg::ConnectFailed {
                                host_id,
                                error: e.to_string(),
                            });
                        }
                    }
                });
            }
            UiEvent::SaveHost(form) => self.begin_save_host(*form, false),
            UiEvent::SaveHostConnect(form) => self.begin_save_host(*form, true),
            UiEvent::DeleteHost(id) => {
                let storage = self.storage.clone();
                tokio::spawn(async move {
                    let _ = storage.hosts().delete(&id).await;
                });
                self.cache_dirty = true;
                self.state.toast(self.state.t("Host deleted"), false);
            }
            UiEvent::ToggleFavorite(id) => {
                if let Some(host) = self.cached.hosts.iter().find(|h| h.id == id).cloned() {
                    let mut host = host;
                    host.favorite = !host.favorite;
                    host.updated_at = now_unix();
                    let storage = self.storage.clone();
                    tokio::spawn(async move {
                        let _ = storage.hosts().update(&host).await;
                    });
                    self.cache_dirty = true;
                }
            }
            UiEvent::ImportHosts(json) => self.import_hosts(&json),
            UiEvent::ExportHosts => self.export_hosts(),
            UiEvent::PingHosts => self.spawn_ping(),
            UiEvent::CreateGroup(name) => {
                let storage = self.storage.clone();
                tokio::spawn(async move {
                    let group = opsdeck_storage::models::HostGroup {
                        id: uuid::Uuid::new_v4().to_string(),
                        name,
                        parent_id: None,
                        sort_order: 0,
                        created_at: now_unix(),
                    };
                    let _ = storage.groups().create(&group).await;
                });
                self.cache_dirty = true;
                self.state.toast(self.state.t("Group created"), false);
            }
            UiEvent::TestConnection(form) => self.test_connection(*form),
            UiEvent::DeleteGroup(id) => {
                let storage = self.storage.clone();
                let gid = id.clone();
                tokio::spawn(async move {
                    let _ = storage.groups().delete(&gid).await;
                });
                self.cache_dirty = true;
                if self.state.group_filter == opsdeck_ui::GroupFilter::Group(id) {
                    self.state.group_filter = opsdeck_ui::GroupFilter::All;
                }
                self.state.toast(self.state.t("Group deleted"), false);
            }

            // -- terminal ---------------------------------------------------
            UiEvent::CloseDesktop { id } => {
                if let Some(h) = self.desktops.remove(&id) {
                    h.send(opsdeck_desktop::DesktopCommand::Close);
                }
                self.desktop_views.retain(|v| v.id != id);
            }
            UiEvent::DesktopPointer { id, buttons, x, y } => {
                if let Some(h) = self.desktops.get(&id) {
                    h.send(opsdeck_desktop::DesktopCommand::Pointer { buttons, x, y });
                }
            }
            UiEvent::DesktopKey { id, keysym, down } => {
                if let Some(h) = self.desktops.get(&id) {
                    h.send(opsdeck_desktop::DesktopCommand::Key {
                        keycode: keysym,
                        down,
                    });
                }
            }
            UiEvent::TerminalInput(session, bytes) => {
                if let Some(shell) = self.shells.get(&session) {
                    let _ = shell.commands.send(ShellCommand::Write(bytes));
                }
            }
            UiEvent::TerminalResize(session, cols, rows) => {
                if let Some(shell) = self.shells.get(&session) {
                    let _ = shell.commands.send(ShellCommand::Resize(cols, rows));
                }
            }
            UiEvent::SplitPane { vertical } => {
                if let Some(pane) = self.state.active_pane().cloned() {
                    let tab_idx = self.state.active_tab;
                    self.connect_host(&pane.host_id, false, Some((tab_idx, vertical)));
                } else {
                    self.state
                        .toast(self.state.t("No active terminal to split"), true);
                }
            }
            UiEvent::ClosePane(session) => self.disconnect_session(session),
            UiEvent::HostKeyDecision {
                prompt_id,
                decision,
            } => {
                if let Some(pos) = self.pending_prompts.iter().position(|p| p.id == prompt_id) {
                    let pending = self.pending_prompts.remove(pos);
                    let _ = pending.responder.send(decision);
                }
            }

            // -- sftp -------------------------------------------------------
            UiEvent::SftpNavigate { host_id, path } => self.navigate_sftp(&host_id, &path),
            UiEvent::SftpMkdir { host_id, path } => {
                if let Some(session) = self.host_sessions.get(&host_id).cloned() {
                    let sftp = self.sftp.clone();
                    let tx = self.msg_tx.clone();
                    let host_id_c = host_id.clone();
                    tokio::spawn(async move {
                        let result = sftp.mkdir(session, &path).await.map_err(|e| e.to_string());
                        let _ = tx.send(AppMsg::SftpOpDone {
                            host_id: host_id_c,
                            result,
                            what: "mkdir".to_string(),
                        });
                    });
                }
            }
            UiEvent::SftpDelete { host_id, path } => {
                if let Some(session) = self.host_sessions.get(&host_id).cloned() {
                    let sftp = self.sftp.clone();
                    let tx = self.msg_tx.clone();
                    tokio::spawn(async move {
                        let result = sftp.remove(session, &path).await.map_err(|e| e.to_string());
                        let _ = tx.send(AppMsg::SftpOpDone {
                            host_id,
                            result,
                            what: "delete".to_string(),
                        });
                    });
                }
            }
            UiEvent::SftpRename { host_id, from, to } => {
                if let Some(session) = self.host_sessions.get(&host_id).cloned() {
                    let sftp = self.sftp.clone();
                    let tx = self.msg_tx.clone();
                    tokio::spawn(async move {
                        let result = sftp
                            .rename(session, &from, &to)
                            .await
                            .map_err(|e| e.to_string());
                        let _ = tx.send(AppMsg::SftpOpDone {
                            host_id,
                            result,
                            what: "rename".to_string(),
                        });
                    });
                }
            }
            UiEvent::SftpUpload {
                host_id,
                local,
                remote,
            } => {
                let task =
                    TransferQueue::new_task(&host_id, TransferDirection::Upload, &local, &remote);
                let transfers = self.transfers.clone();
                tokio::spawn(async move {
                    if let Err(e) = transfers.enqueue(task).await {
                        tracing::warn!("enqueue upload failed: {e}");
                    }
                });
                self.cache_dirty = true;
                self.state.toast(self.state.t("Upload queued"), false);
            }
            UiEvent::SftpDownload {
                host_id,
                remote,
                local,
            } => {
                let task =
                    TransferQueue::new_task(&host_id, TransferDirection::Download, &local, &remote);
                let transfers = self.transfers.clone();
                tokio::spawn(async move {
                    if let Err(e) = transfers.enqueue(task).await {
                        tracing::warn!("enqueue download failed: {e}");
                    }
                });
                self.cache_dirty = true;
                self.state.toast(self.state.t("Download queued"), false);
            }
            UiEvent::TransferPause(id) => {
                let transfers = self.transfers.clone();
                tokio::spawn(async move {
                    let _ = transfers.pause(&id).await;
                });
                self.cache_dirty = true;
            }
            UiEvent::TransferRetry(id) => {
                let transfers = self.transfers.clone();
                tokio::spawn(async move {
                    let _ = transfers.retry(&id).await;
                });
                self.cache_dirty = true;
            }
            UiEvent::TransferCancel(id) => {
                let transfers = self.transfers.clone();
                tokio::spawn(async move {
                    let _ = transfers.cancel(&id).await;
                });
                self.cache_dirty = true;
            }

            // -- monitor ----------------------------------------------------
            UiEvent::MonitorSelect(host_id) => {
                self.state.monitor_host = Some(host_id.clone());
                self.ensure_monitor_task(&host_id);
            }

            // -- snippets ---------------------------------------------------
            UiEvent::RunSnippet {
                snippet_id,
                session,
            } => {
                if let Some(snippet) = self.cached.snippets.iter().find(|s| s.id == snippet_id) {
                    let mut cmd = snippet.command.clone();
                    cmd.push('\n');
                    if let Some(shell) = self.shells.get(&session) {
                        let _ = shell.commands.send(ShellCommand::Write(cmd.into_bytes()));
                    } else {
                        self.state
                            .toast(self.state.t("Session has no open shell"), true);
                    }
                }
            }
            UiEvent::SaveSnippet(form) => {
                let storage = self.storage.clone();
                tokio::spawn(async move {
                    let now = now_unix();
                    if let Some(id) = form.id {
                        if let Ok(mut s) = storage.snippets().get(&id).await {
                            s.title = form.title;
                            s.command = form.command;
                            s.description = Some(form.description).filter(|d| !d.is_empty());
                            s.tags = serde_json::to_string(
                                &form
                                    .tags
                                    .split(',')
                                    .map(str::trim)
                                    .filter(|t| !t.is_empty())
                                    .collect::<Vec<_>>(),
                            )
                            .unwrap_or_default();
                            s.updated_at = now;
                            let _ = storage.snippets().update(&s).await;
                        }
                    } else {
                        let snippet = opsdeck_storage::models::Snippet {
                            id: uuid::Uuid::new_v4().to_string(),
                            title: form.title,
                            command: form.command,
                            description: Some(form.description).filter(|d| !d.is_empty()),
                            host_id: None,
                            tags: serde_json::to_string(
                                &form
                                    .tags
                                    .split(',')
                                    .map(str::trim)
                                    .filter(|t| !t.is_empty())
                                    .collect::<Vec<_>>(),
                            )
                            .unwrap_or_default(),
                            created_at: now,
                            updated_at: now,
                        };
                        let _ = storage.snippets().create(&snippet).await;
                    }
                });
                self.cache_dirty = true;
            }
            UiEvent::DeleteSnippet(id) => {
                let storage = self.storage.clone();
                tokio::spawn(async move {
                    let _ = storage.snippets().delete(&id).await;
                });
                self.cache_dirty = true;
            }

            // -- batch ------------------------------------------------------
            UiEvent::BatchRun {
                sessions,
                command,
                allow_dangerous,
            } => {
                let ssh = self.ssh.clone();
                let tx = self.msg_tx.clone();
                let names = self.session_names.clone();
                tokio::spawn(async move {
                    let targets: Vec<(SessionId, String)> = sessions
                        .into_iter()
                        .map(|s| {
                            let name = names.get(&s).cloned().unwrap_or_else(|| format!("{s}"));
                            (s, name)
                        })
                        .collect();
                    match batch::run_on_hosts(&ssh, &targets, &command, allow_dangerous).await {
                        Ok(results) => {
                            let out = results
                                .into_iter()
                                .map(|r| {
                                    let text = match r.result {
                                        Ok(o) => o.stdout,
                                        Err(e) => format!("error: {e}"),
                                    };
                                    (r.label, text)
                                })
                                .collect();
                            let _ = tx.send(AppMsg::BatchDone { results: out });
                        }
                        Err(e) => {
                            let _ = tx.send(AppMsg::BatchDone {
                                results: vec![("error".to_string(), e.to_string())],
                            });
                        }
                    }
                });
                self.state.toast(self.state.t("Batch run started"), false);
            }

            // -- port forwarding --------------------------------------------
            UiEvent::StartForward {
                session,
                bind,
                remote_host,
                remote_port,
            } => match bind.parse::<std::net::SocketAddr>() {
                Ok(addr) => {
                    let ssh = self.ssh.clone();
                    let tx = self.msg_tx.clone();
                    let id = self.next_forward_id;
                    self.next_forward_id += 1;
                    let local = bind.clone();
                    let remote = format!("{remote_host}:{remote_port}");
                    tokio::spawn(async move {
                        match ssh
                            .start_local_forward(session, addr, remote_host, remote_port)
                            .await
                        {
                            Ok(forward) => {
                                let _ = tx.send(AppMsg::ForwardStarted {
                                    id,
                                    session,
                                    local,
                                    remote,
                                    forward,
                                });
                            }
                            Err(e) => {
                                let _ = tx.send(AppMsg::ForwardFailed {
                                    error: e.to_string(),
                                });
                            }
                        }
                    });
                }
                Err(_) => self
                    .state
                    .toast(self.state.t("Bad bind address (use 127.0.0.1:8080)"), true),
            },
            UiEvent::StopForward(id) => {
                if let Some(pos) = self.forwards.iter().position(|f| f.id == id) {
                    let fwd = self.forwards.remove(pos);
                    fwd.forward.close();
                    self.state.toast(self.state.t("Forward stopped"), false);
                }
            }

            // -- sync -------------------------------------------------------
            UiEvent::SetSyncKind(kind) => {
                self.sync_view.configured = kind != "off";
                let storage = self.storage.clone();
                tokio::spawn(async move {
                    let _ = storage.set_setting("sync.kind", &kind).await;
                });
            }
            UiEvent::SyncNow => self.start_sync(),

            // -- settings ---------------------------------------------------
            UiEvent::SetTheme(name) => {
                if let Some(t) = Theme::builtins().into_iter().find(|t| t.name == name) {
                    // Preserve the user's font sizes across theme switches.
                    let (fs, tfs) = (self.theme.font_size, self.theme.terminal_font_size);
                    self.theme = t;
                    self.theme.font_size = fs;
                    self.theme.terminal_font_size = tfs;
                    self.persist_theme();
                }
            }
            UiEvent::SetLanguage(code) => {
                let lang = Language::from_code(&code);
                self.state.language = lang;
                let storage = self.storage.clone();
                tokio::spawn(async move {
                    let _ = storage.set_setting("ui.language", lang.code()).await;
                });
            }
            UiEvent::SetFontSize(size) => {
                self.theme.font_size = size;
                self.persist_theme();
            }
            UiEvent::SetTerminalFontSize(size) => {
                self.theme.terminal_font_size = size;
                self.persist_theme();
            }
            UiEvent::CheckUpdate => {
                self.update_view.checking = true;
                let tx = self.msg_tx.clone();
                tokio::spawn(async move {
                    let result = check_update().await.map_err(|e| e.to_string());
                    let _ = tx.send(AppMsg::UpdateDone { result });
                });
            }
            UiEvent::OpenUrl(url) => open_url(&url),
        }
    }

    fn persist_theme(&self) {
        let storage = self.storage.clone();
        let theme = self.theme.clone();
        tokio::spawn(async move {
            if let Ok(json) = serde_json::to_string(&theme) {
                let _ = storage.set_setting("ui.theme", &json).await;
            }
        });
    }

    // -- hosts ---------------------------------------------------------------

    /// Guarded save: creating a credential needs an unlocked vault, and a
    /// save without any credential material used to die silently inside
    /// the async task. Refuse early, explain, hand the form back.
    fn begin_save_host(&mut self, form: opsdeck_ui::HostForm, connect: bool) {
        if form.credential_id.is_empty() {
            if !self.vault_unlocked {
                self.state.toast(
                    self.state
                        .t("Vault is locked — unlock it to save this host."),
                    true,
                );
                self.state.show_vault_dialog = true;
                self.state.host_form = form;
                self.state.show_host_editor = true;
                return;
            }
            let has_material = !form.password.is_empty()
                || !form.private_key_pem.trim().is_empty()
                || form.auth_type == "agent";
            if !has_material {
                self.state.toast(
                    self.state
                        .t("Enter a password or private key, or pick an existing credential."),
                    true,
                );
                self.state.host_form = form;
                self.state.show_host_editor = true;
                return;
            }
        }
        self.save_host(form, connect);
    }

    fn save_host(&mut self, form: opsdeck_ui::HostForm, connect_after: bool) {
        let storage = self.storage.clone();
        let vault = self.vault.clone();
        let tx = self.msg_tx.clone();
        tokio::spawn(async move {
            let mut saved_id: Option<String> = form.id.clone();
            // Resolve or create the credential.
            let mut credential_id = if form.credential_id.is_empty() {
                None
            } else {
                Some(form.credential_id.clone())
            };
            let mut passphrase_credential_id: Option<String> = None;

            let encrypt_secret =
                |vault: &Vault, plaintext: &[u8]| -> Result<(Vec<u8>, Vec<u8>), VaultError> {
                    let blob = vault.encrypt(plaintext)?;
                    Ok((blob.nonce.to_vec(), blob.ciphertext))
                };

            if credential_id.is_none() {
                let vault = vault.read().await.clone();
                let Some(v) = vault.as_ref() else { return };
                let (kind, secret): (CredentialKind, Vec<u8>) = match form.auth_type.as_str() {
                    "private_key" if !form.private_key_pem.trim().is_empty() => (
                        CredentialKind::PrivateKey,
                        form.private_key_pem.as_bytes().to_vec(),
                    ),
                    "agent" => {
                        // Agent auth needs no stored secret.
                        (CredentialKind::Agent, Vec::new())
                    }
                    _ if !form.password.is_empty() => {
                        (CredentialKind::Password, form.password.as_bytes().to_vec())
                    }
                    _ => return,
                };
                if kind != CredentialKind::Agent {
                    match encrypt_secret(v, &secret) {
                        Ok((nonce, ciphertext)) => {
                            let cred = Credential {
                                id: uuid::Uuid::new_v4().to_string(),
                                name: format!("{} credential", form.name),
                                kind: kind.as_str().to_string(),
                                ciphertext,
                                nonce,
                                created_at: now_unix(),
                                updated_at: now_unix(),
                            };
                            if storage.credentials().upsert(&cred).await.is_ok() {
                                credential_id = Some(cred.id.clone());
                            }
                        }
                        Err(e) => {
                            tracing::error!("encrypt credential: {e}");
                            return;
                        }
                    }
                    // Key passphrase as a separate credential.
                    if kind == CredentialKind::PrivateKey
                        && !form.passphrase.is_empty()
                        && let Ok((nonce, ciphertext)) =
                            encrypt_secret(v, form.passphrase.as_bytes())
                    {
                        let cred = Credential {
                            id: uuid::Uuid::new_v4().to_string(),
                            name: format!("{} key passphrase", form.name),
                            kind: CredentialKind::Password.as_str().to_string(),
                            ciphertext,
                            nonce,
                            created_at: now_unix(),
                            updated_at: now_unix(),
                        };
                        if storage.credentials().upsert(&cred).await.is_ok() {
                            passphrase_credential_id = Some(cred.id.clone());
                        }
                    }
                }
            }

            let tags_json = serde_json::to_string(
                &form
                    .tags
                    .split(',')
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>(),
            )
            .unwrap_or_else(|_| "[]".to_string());

            let opt_note = opt_str(&form.note);
            let opt_timeout = opt_num(&form.connect_timeout);
            let opt_keepalive = opt_num(&form.keepalive_secs);
            let opt_term = opt_str(&form.term_type);
            let opt_path = opt_str(&form.default_path);
            let opt_proxy = opt_str(&form.proxy_url);
            let opt_env = env_json_from(&form.env_vars);
            let opt_kcmd = opt_str(&form.keepalive_cmd);
            let opt_ksecs = opt_num(&form.keepalive_cmd_secs);
            let protocol = form.protocol.clone();
            let apply_opts = |host: &mut Host| {
                host.protocol = protocol.clone();
                host.note = opt_note.clone();
                host.connect_timeout_secs = opt_timeout;
                host.keepalive_secs = opt_keepalive;
                host.term_type = opt_term.clone();
                host.default_path = opt_path.clone();
                host.proxy_url = opt_proxy.clone();
                host.env_json = opt_env.clone();
                host.keepalive_cmd = opt_kcmd.clone();
                host.keepalive_cmd_secs = opt_ksecs;
            };

            if let Some(id) = form.id {
                if let Ok(mut host) = storage.hosts().get(&id).await {
                    apply_opts(&mut host);
                    host.name = form.name;
                    host.host = form.host;
                    host.port = form.port.parse().unwrap_or(22);
                    host.username = form.username;
                    host.auth_type = form.auth_type;
                    host.group_id = if form.group_id.is_empty() {
                        None
                    } else {
                        Some(form.group_id)
                    };
                    host.proxy_jump_id = if form.proxy_jump_id.is_empty() {
                        None
                    } else {
                        Some(form.proxy_jump_id)
                    };
                    host.startup_command = if form.startup_command.is_empty() {
                        None
                    } else {
                        Some(form.startup_command)
                    };
                    host.tags = tags_json;
                    host.favorite = form.favorite;
                    if credential_id.is_some() {
                        host.credential_id = credential_id;
                    }
                    if passphrase_credential_id.is_some() {
                        host.passphrase_credential_id = passphrase_credential_id;
                    }
                    host.updated_at = now_unix();
                    let _ = storage.hosts().update(&host).await;
                }
            } else {
                let mut host = Host::new(form.name, form.host, form.username);
                saved_id = Some(host.id.clone());
                apply_opts(&mut host);
                host.port = form.port.parse().unwrap_or(22);
                host.auth_type = form.auth_type;
                host.group_id = if form.group_id.is_empty() {
                    None
                } else {
                    Some(form.group_id)
                };
                host.proxy_jump_id = if form.proxy_jump_id.is_empty() {
                    None
                } else {
                    Some(form.proxy_jump_id)
                };
                host.startup_command = if form.startup_command.is_empty() {
                    None
                } else {
                    Some(form.startup_command)
                };
                host.tags = tags_json;
                host.favorite = form.favorite;
                host.credential_id = credential_id;
                host.passphrase_credential_id = passphrase_credential_id;
                let _ = storage.hosts().create(&host).await;
            }
            if connect_after && let Some(id) = saved_id {
                let _ = tx.send(AppMsg::HostSavedConnect(id));
            }
        });
        self.cache_dirty = true;
        self.state.toast(self.state.t("Host saved"), false);
    }

    /// Test a connection from the host editor: dials, then disconnects.
    fn test_connection(&mut self, form: opsdeck_ui::HostForm) {
        if !form.credential_id.is_empty()
            && form.password.is_empty()
            && form.private_key_pem.trim().is_empty()
            && !self.vault_unlocked
        {
            self.state.toast(
                self.state.t("Vault is locked — unlock it to connect."),
                true,
            );
            self.state.show_vault_dialog = true;
            return;
        }
        let mut target = Host::new(form.name.clone(), form.host.clone(), form.username.clone());
        target.port = form.port.parse().unwrap_or(22);
        target.auth_type = form.auth_type.clone();
        target.protocol = form.protocol.clone();
        if !form.proxy_jump_id.is_empty() {
            target.proxy_jump_id = Some(form.proxy_jump_id.clone());
        }
        apply_option_fields(&mut target, &form);
        target.id = form
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let mut chain: Vec<HostConfig> = Vec::new();
        let mut current = Some(target.clone());
        let mut guard = 0;
        while let Some(h) = current {
            guard += 1;
            if guard > 8 {
                self.state.toast(self.state.t("Jump chain too deep"), true);
                return;
            }
            match h.to_config() {
                Ok(cfg) => chain.push(cfg),
                Err(e) => {
                    self.state.toast(
                        self.state
                            .tf("Bad host config: {e}", &[("e", &e.to_string())]),
                        true,
                    );
                    return;
                }
            }
            current = h
                .proxy_jump_id
                .as_deref()
                .and_then(|jid| self.cached.hosts.iter().find(|x| x.id == jid).cloned());
        }
        chain.reverse();

        let password = if !form.password.is_empty() {
            Some(form.password.clone())
        } else {
            None
        };
        let (key_pem, passphrase) = if !form.private_key_pem.trim().is_empty() {
            (
                Some(form.private_key_pem.as_bytes().to_vec()),
                if form.passphrase.is_empty() {
                    None
                } else {
                    Some(form.passphrase.clone())
                },
            )
        } else {
            (None, None)
        };
        let creds = std::sync::Arc::new(FormCredentials {
            password,
            key_pem,
            passphrase,
            fallback: self.creds.clone(),
        });
        let ssh = self.ssh.clone();
        let tx = self.msg_tx.clone();
        self.state.toast(self.state.t("Testing connection…"), false);
        tokio::spawn(async move {
            let start = std::time::Instant::now();
            let result = async {
                let session = ssh.connect_chain(&chain, creds.as_ref()).await?;
                let _ = ssh.disconnect(session).await;
                Ok::<u32, SshError>(start.elapsed().as_millis() as u32)
            }
            .await;
            let _ = tx.send(AppMsg::TestResult(result.map_err(|e| e.to_string())));
        });
    }

    fn import_hosts(&mut self, json: &str) {
        match serde_json::from_str::<ExportData>(json) {
            Ok(data) => {
                let storage = self.storage.clone();
                let count = data.hosts.len();
                tokio::spawn(async move {
                    for host in data.hosts {
                        // Skip duplicates by id.
                        if storage.hosts().get(&host.id).await.is_err() {
                            let _ = storage.hosts().create(&host).await;
                        }
                    }
                    for group in data.groups {
                        let _ = storage.groups().create(&group).await;
                    }
                });
                self.cache_dirty = true;
                self.state.toast(
                    self.state
                        .tf("Imported {count} host(s)", &[("count", &count.to_string())]),
                    false,
                );
            }
            Err(e) => self.state.toast(
                self.state
                    .tf("Import failed: {e}", &[("e", &e.to_string())]),
                true,
            ),
        }
    }

    fn export_hosts(&mut self) {
        let hosts = self.cached.hosts.clone();
        let groups = self.cached.groups.clone();
        let data = ExportData::new(hosts, groups, Vec::new());
        match serde_json::to_string_pretty(&data) {
            Ok(json) => {
                let path = std::env::var("HOME")
                    .map(|h| format!("{h}/Downloads/opsdeck-hosts.json"))
                    .unwrap_or_else(|_| "/tmp/opsdeck-hosts.json".to_string());
                match std::fs::write(&path, json) {
                    Ok(()) => self.state.toast(
                        self.state.tf("Exported to {path}", &[("path", &path)]),
                        false,
                    ),
                    Err(e) => self.state.toast(
                        self.state
                            .tf("Export failed: {e}", &[("e", &e.to_string())]),
                        true,
                    ),
                }
            }
            Err(e) => self.state.toast(
                self.state
                    .tf("Export failed: {e}", &[("e", &e.to_string())]),
                true,
            ),
        }
    }

    /// Run one sync against the configured remote.
    fn start_sync(&mut self) {
        if self.sync_view.running {
            return;
        }
        let kind = self.state.sync_kind.clone();
        if kind == "off" {
            self.state
                .toast(self.state.t("Sync is off — pick a mode in Settings"), true);
            return;
        }
        let vault = self.vault.blocking_read().clone();
        let Some(vault) = vault else {
            self.state.toast(self.state.t("Vault is locked"), true);
            return;
        };
        self.sync_view.running = true;
        self.sync_view.error = None;

        let storage = self.storage.clone();
        let tx = self.msg_tx.clone();
        let file_dir = self.state.sync_file_dir.clone();
        let endpoint = self.state.sync_http_endpoint.clone();
        tokio::spawn(async move {
            let device = device_id(&storage)
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            let remote: Arc<dyn Remote> = match kind.as_str() {
                "file" => Arc::new(FileRemote::new(file_dir)),
                "http" => Arc::new(HttpRemote::with_endpoint(&endpoint, &device)),
                _ => {
                    let _ = tx.send(AppMsg::SyncDone {
                        result: Err("unknown sync mode".to_string()),
                    });
                    return;
                }
            };
            let engine = SyncEngine::new(remote, storage, device);
            let result = engine.sync(&vault).await.map_err(|e| e.to_string());
            let _ = tx.send(AppMsg::SyncDone { result });
        });
    }
}

impl eframe::App for OpsDeckApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.activity = false;
        self.drain_messages(&ctx);

        // Periodic cache refresh (1s) or on demand.
        let stale = self
            .cached
            .refreshed
            .map(|t| t.elapsed() > Duration::from_secs(1))
            .unwrap_or(true);
        if self.cache_dirty || stale {
            // refresh_cache is async; run it in the background and apply on completion.
            // To keep this simple and correct, block briefly via a one-shot spawn:
            // instead we refresh synchronously every N frames using block_in_place-safe
            // approach — eframe's update runs outside the runtime, so we can use
            // a dedicated short-lived block_on here.
            let storage = self.storage.clone();
            let (hosts, groups, snippets, credentials, transfers) =
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        (
                            storage.hosts().list_all().await.unwrap_or_default(),
                            storage.groups().list_all().await.unwrap_or_default(),
                            storage.snippets().list_all().await.unwrap_or_default(),
                            storage.credentials().list_meta().await.unwrap_or_default(),
                            storage
                                .transfers()
                                .list_recent(200)
                                .await
                                .unwrap_or_default(),
                        )
                    })
                });
            // Preserve live progress updates already applied to the cached vec.
            let progress: HashMap<String, i64> = self
                .cached
                .transfers
                .iter()
                .map(|t| (t.id.clone(), t.bytes_done))
                .collect();
            let mut transfers = transfers;
            for t in &mut transfers {
                if let Some(done) = progress.get(&t.id) {
                    t.bytes_done = (*done).max(t.bytes_done);
                }
            }
            self.cached = CachedData {
                hosts,
                groups,
                snippets,
                credentials,
                transfers,
                refreshed: Some(Instant::now()),
            };
            self.cache_dirty = false;
        }

        // Save-and-connect: the save has landed in the cache by now.
        if let Some(id) = self.pending_connect.take() {
            self.connect_host(&id, true, None);
        }

        // Mirror vault status for the UI (settings, menus, vault dialog).
        self.state.vault_exists = self.vault_exists;
        self.state.vault_unlocked = self.vault_unlocked;

        // Rebuild the session list for the UI: live sessions plus recently
        // closed ones (shown with a "disconnected" notice). Drop entries whose
        // tab was closed or that are older than 5 minutes.
        {
            let open: HashSet<SessionId> = self
                .state
                .tabs
                .iter()
                .flat_map(|t| t.panes.iter().map(|p| p.session))
                .collect();
            let now = Instant::now();
            self.recently_closed.retain(|s, (_, at)| {
                open.contains(s) && now.duration_since(*at) < Duration::from_secs(300)
            });
            self.session_views.clear();
            self.session_views.extend(self.sessions.iter().cloned());
            let live: HashSet<SessionId> = self.sessions.iter().map(|s| s.id).collect();
            for (info, _) in self.recently_closed.values() {
                if !live.contains(&info.id) {
                    self.session_views.push(info.clone());
                }
            }
            for info in &self.extra_sessions {
                if !live.contains(&info.id) {
                    self.session_views.push(info.clone());
                }
            }
        }

        // Measure latencies for any host we have not probed yet.
        if !self.ping_in_flight
            && self
                .cached
                .hosts
                .iter()
                .any(|h| !self.latencies.contains_key(&h.id))
        {
            self.spawn_ping();
        }

        // Build UiData. Prompts and forwards need owned scratch vecs that live
        // for the whole render call.
        let prompt_views: Vec<KeyPromptView> = self
            .pending_prompts
            .iter()
            .map(|p| p.view.clone())
            .collect();
        let forward_views: Vec<ForwardView> = self
            .forwards
            .iter()
            .map(|f| ForwardView {
                id: f.id,
                session_name: f.session_name.clone(),
                local: f.local.clone(),
                remote: f.remote.clone(),
            })
            .collect();
        let data = UiData {
            hosts: &self.cached.hosts,
            groups: &self.cached.groups,
            sessions: &self.session_views,
            snippets: &self.cached.snippets,
            transfers: &self.cached.transfers,
            credentials: &self.cached.credentials,
            sftp_listings: &self.sftp_listings,
            metrics: &self.metrics,
            processes: &self.processes,
            key_prompts: &prompt_views,
            connecting: &self.connecting,
            forwards: &forward_views,
            update: &self.update_view,
            sync: &self.sync_view,
            theme: &self.theme,
            latencies: &self.latencies,
            sysinfo: &self.sysinfo,
            desktops: &self.desktop_views,
        };

        let mut events = Vec::new();
        opsdeck_ui::render(ui, &mut self.state, &data, &mut events);
        for event in events {
            self.handle_event(&ctx, event);
        }

        // Keep repainting while there's live activity.
        if self.activity
            || !self.shells.is_empty()
            || !self.connecting.is_empty()
            || self.sync_view.running
            || self.update_view.checking
        {
            ctx.request_repaint();
        }
    }
}

/// Trim-then-keep: empty string becomes None.
fn opt_str(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// Parse a non-negative integer option.
fn opt_num(s: &str) -> Option<i64> {
    s.trim().parse::<i64>().ok().filter(|v| *v >= 0)
}

/// Parse `KEY=value` lines into a JSON object; empty = None.
fn env_json_from(text: &str) -> Option<String> {
    let mut map = std::collections::BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            if !k.is_empty() {
                map.insert(k.to_string(), v.trim().to_string());
            }
        }
    }
    if map.is_empty() {
        None
    } else {
        serde_json::to_string(&map).ok()
    }
}

/// Fill the Xterminal-style option fields of a Host from an editor form.
fn apply_option_fields(host: &mut Host, form: &opsdeck_ui::HostForm) {
    host.note = opt_str(&form.note);
    host.connect_timeout_secs = opt_num(&form.connect_timeout);
    host.keepalive_secs = opt_num(&form.keepalive_secs);
    host.term_type = opt_str(&form.term_type);
    host.default_path = opt_str(&form.default_path);
    host.proxy_url = opt_str(&form.proxy_url);
    host.env_json = env_json_from(&form.env_vars);
    host.keepalive_cmd = opt_str(&form.keepalive_cmd);
    host.keepalive_cmd_secs = opt_num(&form.keepalive_cmd_secs);
}

/// CredentialProvider backed by the host editor's in-progress fields,
/// falling back to stored vault credentials.
struct FormCredentials {
    password: Option<String>,
    key_pem: Option<Vec<u8>>,
    passphrase: Option<String>,
    fallback: std::sync::Arc<AppCredentials>,
}

#[async_trait::async_trait]
impl CredentialProvider for FormCredentials {
    async fn password_for(&self, config: &HostConfig) -> Result<SecretString, SshError> {
        if let Some(pw) = &self.password {
            return Ok(SecretString::new(pw.clone()));
        }
        self.fallback.password_for(config).await
    }

    async fn private_key_for(
        &self,
        config: &HostConfig,
    ) -> Result<(SecretBytes, Option<SecretString>), SshError> {
        if let Some(pem) = &self.key_pem {
            return Ok((
                SecretBytes::new(pem.clone()),
                self.passphrase.clone().map(SecretString::new),
            ));
        }
        self.fallback.private_key_for(config).await
    }
}

/// Open a URL in the system browser.
fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}
