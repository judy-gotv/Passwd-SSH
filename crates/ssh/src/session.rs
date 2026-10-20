//! SSH session management: dial, authenticate, shells, exec, forwarding.
//!
//! Implements the session state machine from docs §6.2:
//!
//! ```text
//! Disconnected → Connecting → Connected → Ready
//!                    ↓ Error / Timeout
//!               Reconnecting → (fail) → Disconnected
//! ```
//!
//! Author: Passwd Word

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use opsdeck_storage::models::{AuthMethod, HostConfig};
use opsdeck_vault::{SecretBytes, SecretString};
use russh::client::{self, Handler};
use russh::{ChannelMsg, Disconnect};
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, mpsc, oneshot};
use uuid::Uuid;

use crate::agent::authenticate_agent;
use crate::error::SshError;

/// Opaque session identifier.
pub type SessionId = Uuid;

/// Concrete russh client handle used by this engine.
pub type ClientHandle = russh::client::Handle<SessionHandler>;

/// Session lifecycle state (docs §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Disconnected,
    Connecting,
    Connected,
    Ready,
    Reconnecting,
    Failed,
}

impl SessionState {
    /// Whether the session can run shells / exec / forwarding.
    pub fn is_usable(self) -> bool {
        matches!(self, SessionState::Ready)
    }
}

/// Resolves credentials for a host config. Implemented by the app layer
/// (vault + storage); the SSH engine never touches the database directly.
#[async_trait]
pub trait CredentialProvider: Send + Sync {
    /// Password for password auth.
    async fn password_for(&self, config: &HostConfig) -> Result<SecretString, SshError>;
    /// (PEM bytes, optional passphrase) for private-key auth.
    async fn private_key_for(
        &self,
        config: &HostConfig,
    ) -> Result<(SecretBytes, Option<SecretString>), SshError>;
}

/// The user's decision for an unknown/changed server host key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyDecision {
    TrustOnce,
    TrustAlways,
    Reject,
}

/// A host-key approval request sent to the UI thread.
pub struct HostKeyPrompt {
    pub host: String,
    pub port: u16,
    pub key_type: String,
    /// Full OpenSSH public-key line (`ssh-ed25519 AAAA...`).
    pub key_openssh: String,
    /// `true` when we previously trusted a *different* key (possible MITM).
    pub changed: bool,
    pub responder: oneshot::Sender<HostKeyDecision>,
}

// ---------------------------------------------------------------------------
// Known-hosts TOFU database
// ---------------------------------------------------------------------------

/// Trust-on-first-use database: `host:port` → full OpenSSH public key line.
///
/// Persisted as JSON next to the app config. A *changed* key is never
/// silently accepted — the user is always prompted.
#[derive(Debug, Default)]
struct KnownHostsDb {
    path: PathBuf,
    keys: HashMap<String, String>,
}

impl KnownHostsDb {
    fn load(path: &Path) -> Self {
        let keys = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            path: path.to_path_buf(),
            keys,
        }
    }

    fn lookup(&self, id: &str) -> Option<&str> {
        self.keys.get(id).map(String::as_str)
    }

    fn insert(&mut self, id: String, key_openssh: String) {
        self.keys.insert(id, key_openssh);
        let _ = std::fs::create_dir_all(self.path.parent().unwrap_or(Path::new(".")));
        if let Ok(json) = serde_json::to_string_pretty(&self.keys) {
            let _ = std::fs::write(&self.path, json);
        }
    }
}

// ---------------------------------------------------------------------------
// russh Handler
// ---------------------------------------------------------------------------

/// russh client handler: verifies host keys (TOFU + UI prompt).
#[derive(Clone)]
pub struct SessionHandler {
    host: String,
    port: u16,
    known_hosts: Arc<Mutex<KnownHostsDb>>,
    prompt_tx: mpsc::UnboundedSender<HostKeyPrompt>,
}

impl SessionHandler {
    async fn prompt_user(
        &self,
        changed: bool,
        key_type: String,
        key_openssh: String,
    ) -> Result<bool, SshError> {
        let id = format!("{}:{}", self.host, self.port);
        let (tx, rx) = oneshot::channel();
        let prompt = HostKeyPrompt {
            host: self.host.clone(),
            port: self.port,
            key_type,
            key_openssh: key_openssh.clone(),
            changed,
            responder: tx,
        };
        self.prompt_tx
            .send(prompt)
            .map_err(|_| SshError::HostKeyRejected)?;

        // The user gets two minutes; silence means rejection.
        match tokio::time::timeout(Duration::from_secs(120), rx).await {
            Ok(Ok(HostKeyDecision::TrustAlways)) => {
                self.known_hosts.lock().await.insert(id, key_openssh);
                Ok(true)
            }
            Ok(Ok(HostKeyDecision::TrustOnce)) => Ok(true),
            _ => Ok(false),
        }
    }
}

impl Handler for SessionHandler {
    type Error = SshError;

    async fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let pubkey = match key {
            russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } => key,
            russh::keys::PublicKeyOrCertificate::Certificate(_) => {
                return Err(SshError::Key(
                    "server presented a certificate host key, which is not supported".into(),
                ));
            }
        };
        let openssh = pubkey
            .to_openssh()
            .map(|s| s.to_string())
            .unwrap_or_else(|_| format!("{pubkey:?}"));
        let key_type = openssh
            .split_whitespace()
            .next()
            .unwrap_or("unknown")
            .to_string();
        let id = format!("{}:{}", self.host, self.port);

        let known = {
            self.known_hosts
                .lock()
                .await
                .lookup(&id)
                .map(str::to_string)
        };
        match known {
            Some(k) if k == openssh => Ok(true),
            Some(_) => {
                tracing::warn!("host key for {id} CHANGED — possible MITM");
                self.prompt_user(true, key_type, openssh).await
            }
            None => self.prompt_user(false, key_type, openssh).await,
        }
    }
}

// ---------------------------------------------------------------------------
// Shell channels
// ---------------------------------------------------------------------------

/// Events flowing from a remote shell to the UI.
#[derive(Debug)]
pub enum ShellEvent {
    /// Raw stdout/stderr bytes.
    Data(Vec<u8>),
    /// The channel closed (EOF or error).
    Closed,
    /// Transport-level error text.
    Error(String),
}

/// Commands flowing from the UI to a remote shell.
#[derive(Debug)]
pub enum ShellCommand {
    Write(Vec<u8>),
    Resize(u16, u16),
    Close,
}

/// An open interactive shell: send [`ShellCommand`]s, receive [`ShellEvent`]s.
pub struct ShellChannel {
    /// Stream of shell output / close notifications.
    pub events: mpsc::UnboundedReceiver<ShellEvent>,
    /// Sink for user input, resizes and close.
    pub commands: mpsc::UnboundedSender<ShellCommand>,
    _task: tokio::task::JoinHandle<()>,
}

impl ShellChannel {
    /// Build a channel pair from another crate (e.g. Telnet): the task drives
    /// the transport until it exits.
    pub fn wrap(
        events: mpsc::UnboundedReceiver<ShellEvent>,
        commands: mpsc::UnboundedSender<ShellCommand>,
        task: tokio::task::JoinHandle<()>,
    ) -> Self {
        Self {
            events,
            commands,
            _task: task,
        }
    }
}

/// Output of a one-shot `exec` command.
#[derive(Debug, Clone)]
pub struct ExecOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<u32>,
}

impl ExecOutput {
    /// Whether the command exited with status 0.
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }
}

// ---------------------------------------------------------------------------
// Port forwarding
// ---------------------------------------------------------------------------

/// A running local port forward. Dropping / closing aborts the listener.
pub struct LocalForward {
    /// Where clients can connect locally.
    pub local_addr: SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

impl LocalForward {
    /// Stop the forward.
    pub fn close(self) {
        self.task.abort();
    }
}

// ---------------------------------------------------------------------------
// Session manager
// ---------------------------------------------------------------------------

/// A managed SSH session.
pub struct ManagedSession {
    pub id: SessionId,
    pub config: HostConfig,
    pub state: SessionState,
    pub last_error: Option<String>,
    /// Jump host this session tunnels through, if any.
    pub via: Option<SessionId>,
    handle: Option<Arc<Mutex<ClientHandle>>>,
}

/// Read-only session summary for the UI.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: SessionId,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub state: SessionState,
    pub last_error: Option<String>,
    pub via: Option<SessionId>,
}

struct ManagerInner {
    sessions: HashMap<SessionId, ManagedSession>,
}

/// SSH engine: dials, authenticates and manages sessions (docs §6.3).
pub struct SessionManager {
    inner: Arc<Mutex<ManagerInner>>,
    prompt_tx: mpsc::UnboundedSender<HostKeyPrompt>,
    known_hosts: Arc<Mutex<KnownHostsDb>>,
}

impl SessionManager {
    /// Create a manager.
    ///
    /// - `prompt_tx`: receives [`HostKeyPrompt`]s for unknown/changed host keys.
    /// - `known_hosts_path`: JSON file persisting trusted host keys (TOFU).
    pub fn new(prompt_tx: mpsc::UnboundedSender<HostKeyPrompt>, known_hosts_path: &Path) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ManagerInner {
                sessions: HashMap::new(),
            })),
            prompt_tx,
            known_hosts: Arc::new(Mutex::new(KnownHostsDb::load(known_hosts_path))),
        }
    }

    /// Receiver side is owned by the app; this is the sender for prompts.
    pub fn prompt_sender(&self) -> mpsc::UnboundedSender<HostKeyPrompt> {
        self.prompt_tx.clone()
    }

    // -- state helpers ------------------------------------------------------

    async fn insert_session(&self, id: SessionId, config: HostConfig, via: Option<SessionId>) {
        let mut inner = self.inner.lock().await;
        inner.sessions.insert(
            id,
            ManagedSession {
                id,
                config,
                state: SessionState::Connecting,
                last_error: None,
                via,
                handle: None,
            },
        );
    }

    async fn set_state(&self, id: SessionId, state: SessionState, err: Option<String>) {
        let mut inner = self.inner.lock().await;
        if let Some(s) = inner.sessions.get_mut(&id) {
            s.state = state;
            if err.is_some() {
                s.last_error = err;
            }
        }
    }

    async fn set_ready(&self, id: SessionId, handle: Arc<Mutex<ClientHandle>>) {
        let mut inner = self.inner.lock().await;
        if let Some(s) = inner.sessions.get_mut(&id) {
            s.state = SessionState::Ready;
            s.last_error = None;
            s.handle = Some(handle);
        }
    }

    /// Borrow the transport handle for a ready session.
    ///
    /// Used by the SFTP engine and other subsystems that multiplex channels
    /// over an established SSH connection.
    /// The stored config of a session.
    pub async fn config_for(&self, id: SessionId) -> Option<HostConfig> {
        let inner = self.inner.lock().await;
        inner.sessions.get(&id).map(|s| s.config.clone())
    }

    pub async fn handle_for(&self, id: SessionId) -> Result<Arc<Mutex<ClientHandle>>, SshError> {
        let inner = self.inner.lock().await;
        let s = inner.sessions.get(&id).ok_or(SshError::SessionNotFound)?;
        if !s.state.is_usable() {
            return Err(SshError::NotReady(format!("session is {:?}", s.state)));
        }
        s.handle
            .clone()
            .ok_or(SshError::NotReady("no transport handle".into()))
    }

    /// Current state of a session.
    pub async fn state(&self, id: SessionId) -> Option<SessionState> {
        self.inner.lock().await.sessions.get(&id).map(|s| s.state)
    }

    /// Snapshot of all sessions for the UI.
    pub async fn list(&self) -> Vec<SessionInfo> {
        self.inner
            .lock()
            .await
            .sessions
            .values()
            .map(|s| SessionInfo {
                id: s.id,
                name: s.config.name.clone(),
                host: s.config.host.clone(),
                port: s.config.port,
                state: s.state,
                last_error: s.last_error.clone(),
                via: s.via,
            })
            .collect()
    }

    // -- dialing ------------------------------------------------------------

    fn russh_config_for(config: &HostConfig) -> Arc<client::Config> {
        let keepalive = match config.keepalive_secs {
            None => Some(Duration::from_secs(30)),
            Some(0) => None,
            Some(secs) => Some(Duration::from_secs(secs as u64)),
        };
        Arc::new(client::Config {
            keepalive_interval: keepalive,
            keepalive_max: 3,
            nodelay: true,
            ..Default::default()
        })
    }

    fn handler_for(&self, config: &HostConfig) -> SessionHandler {
        SessionHandler {
            host: config.host.clone(),
            port: config.port,
            known_hosts: self.known_hosts.clone(),
            prompt_tx: self.prompt_tx.clone(),
        }
    }

    async fn dial_direct(&self, config: &HostConfig) -> Result<ClientHandle, SshError> {
        let timeout_secs = config.connect_timeout_secs.unwrap_or(20).max(1);
        if let Some(proxy) = config.proxy_url.as_deref().filter(|p| !p.trim().is_empty()) {
            return self.dial_via_proxy(config, proxy, timeout_secs).await;
        }
        let handler = self.handler_for(config);
        let addrs = (config.host.as_str(), config.port);
        let connect = russh::client::connect(Self::russh_config_for(config), addrs, handler);
        tokio::time::timeout(Duration::from_secs(timeout_secs as u64), connect)
            .await
            .map_err(|_| SshError::Timeout(format!("TCP connect timed out after {timeout_secs}s")))?
            .map_err(|e: SshError| SshError::Connection(format!("connect failed: {e}")))
    }

    /// Dial the target through an HTTP CONNECT or SOCKS5 proxy, then run the
    /// SSH handshake over that stream.
    async fn dial_via_proxy(
        &self,
        config: &HostConfig,
        proxy_url: &str,
        timeout_secs: u32,
    ) -> Result<ClientHandle, SshError> {
        let parsed = parse_proxy_url(proxy_url)
            .ok_or_else(|| SshError::Connection(format!("bad proxy URL: {proxy_url}")))?;
        let deadline = Duration::from_secs(timeout_secs as u64);
        let stream = tokio::time::timeout(deadline, async {
            let mut tcp = tokio::net::TcpStream::connect((parsed.host.as_str(), parsed.port))
                .await
                .map_err(|e| SshError::Connection(format!("proxy connect failed: {e}")))?;
            match parsed.kind {
                ProxyKind::Socks5 => {
                    socks5_handshake(&mut tcp, &parsed, &config.host, config.port).await?;
                }
                ProxyKind::Http => {
                    http_connect(&mut tcp, &parsed, &config.host, config.port).await?;
                }
            }
            Ok::<_, SshError>(tcp)
        })
        .await
        .map_err(|_| SshError::Timeout("proxy connect timed out".into()))??;

        let handler = self.handler_for(config);
        russh::client::connect_stream(Self::russh_config_for(config), stream, handler)
            .await
            .map_err(|e: SshError| SshError::Connection(format!("connect via proxy failed: {e}")))
    }

    async fn dial_via(
        &self,
        jump: &Arc<Mutex<ClientHandle>>,
        config: &HostConfig,
    ) -> Result<ClientHandle, SshError> {
        let channel = jump
            .lock()
            .await
            .channel_open_direct_tcpip(config.host.as_str(), config.port as u32, "127.0.0.1", 0)
            .await
            .map_err(|e| SshError::Connection(format!("proxyjump channel failed: {e}")))?;
        let stream = channel.into_stream();
        let handler = self.handler_for(config);
        russh::client::connect_stream(Self::russh_config_for(config), stream, handler)
            .await
            .map_err(|e: SshError| {
                SshError::Connection(format!("connect via jump host failed: {e}"))
            })
    }

    async fn authenticate(
        &self,
        handle: &Arc<Mutex<ClientHandle>>,
        config: &HostConfig,
        creds: &dyn CredentialProvider,
    ) -> Result<(), SshError> {
        let mut h = handle.lock().await;
        let result = match &config.auth {
            AuthMethod::Password => {
                let pw = creds.password_for(config).await?;
                h.authenticate_password(config.username.clone(), pw.expose().to_string())
                    .await?
            }
            AuthMethod::PrivateKey { .. } => {
                let (pem, passphrase) = creds.private_key_for(config).await?;
                let pem_str = std::str::from_utf8(pem.expose())
                    .map_err(|_| SshError::Key("private key is not valid UTF-8".into()))?;
                let key = russh::keys::decode_secret_key(
                    pem_str,
                    passphrase.as_ref().map(|p| p.expose()),
                )
                .map_err(|e| SshError::Key(format!("cannot parse private key: {e}")))?;
                let key = russh::keys::PrivateKeyWithHashAlg::new(std::sync::Arc::new(key), None);
                h.authenticate_publickey(config.username.clone(), key)
                    .await?
            }
            AuthMethod::Agent => {
                drop(h);
                let mut h2 = handle.lock().await;
                return authenticate_agent(&mut h2, &config.username).await;
            }
        };
        drop(h);
        if result.success() {
            Ok(())
        } else {
            Err(SshError::Auth("server rejected the credentials".into()))
        }
    }

    // -- public API ---------------------------------------------------------

    /// Connect a chain of hosts: `chain[..n-1]` are jump hosts, `chain[n-1]`
    /// is the target. Returns the target's session id.
    pub async fn connect_chain(
        &self,
        chain: &[HostConfig],
        creds: &dyn CredentialProvider,
    ) -> Result<SessionId, SshError> {
        let Some((target, jumps)) = chain.split_last() else {
            return Err(SshError::Connection("empty connection chain".into()));
        };

        let mut prev: Option<Arc<Mutex<ClientHandle>>> = None;
        let mut prev_id: Option<SessionId> = None;

        for jump_cfg in jumps {
            let id = Uuid::new_v4();
            self.insert_session(id, jump_cfg.clone(), prev_id).await;
            let handle = match &prev {
                None => self.dial_direct(jump_cfg).await,
                Some(j) => self.dial_via(j, jump_cfg).await,
            };
            match handle {
                Ok(h) => {
                    let shared = Arc::new(Mutex::new(h));
                    self.set_state(id, SessionState::Connected, None).await;
                    if let Err(e) = self.authenticate(&shared, jump_cfg, creds).await {
                        self.set_state(id, SessionState::Failed, Some(e.to_string()))
                            .await;
                        return Err(e);
                    }
                    self.set_ready(id, shared.clone()).await;
                    prev = Some(shared);
                    prev_id = Some(id);
                }
                Err(e) => {
                    self.set_state(id, SessionState::Failed, Some(e.to_string()))
                        .await;
                    return Err(e);
                }
            }
        }

        let id = Uuid::new_v4();
        self.insert_session(id, target.clone(), prev_id).await;
        let handle = match &prev {
            None => self.dial_direct(target).await,
            Some(j) => self.dial_via(j, target).await,
        };
        let handle = match handle {
            Ok(h) => h,
            Err(e) => {
                self.set_state(id, SessionState::Failed, Some(e.to_string()))
                    .await;
                return Err(e);
            }
        };
        let shared = Arc::new(Mutex::new(handle));
        self.set_state(id, SessionState::Connected, None).await;
        if let Err(e) = self.authenticate(&shared, target, creds).await {
            self.set_state(id, SessionState::Failed, Some(e.to_string()))
                .await;
            return Err(e);
        }
        self.set_ready(id, shared).await;
        Ok(id)
    }

    /// Connect a single host (no jump).
    pub async fn connect(
        &self,
        config: &HostConfig,
        creds: &dyn CredentialProvider,
    ) -> Result<SessionId, SshError> {
        self.connect_chain(std::slice::from_ref(config), creds)
            .await
    }

    /// Open an interactive shell on a ready session.
    pub async fn open_shell(
        &self,
        id: SessionId,
        cols: u16,
        rows: u16,
    ) -> Result<ShellChannel, SshError> {
        let handle = self.handle_for(id).await?;
        let channel = handle
            .lock()
            .await
            .channel_open_session()
            .await
            .map_err(|e| SshError::Channel(format!("open session channel: {e}")))?;
        let (mut read_half, write_half) = channel.split();
        let term = self
            .config_for(id)
            .await
            .and_then(|c| c.term_type)
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| "xterm-256color".to_string());
        let env = self.config_for(id).await.map(|c| c.env).unwrap_or_default();
        for (name, value) in &env {
            // Best effort: many servers refuse env requests (AcceptEnv).
            let _ = write_half.set_env(true, name.clone(), value.clone()).await;
        }
        write_half
            .request_pty(true, term.as_str(), cols as u32, rows as u32, 0, 0, &[])
            .await
            .map_err(|e| SshError::Channel(format!("pty request: {e}")))?;
        write_half
            .request_shell(true)
            .await
            .map_err(|e| SshError::Channel(format!("shell request: {e}")))?;

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<ShellCommand>();
        let mut writer = write_half.make_writer();

        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    msg = read_half.wait() => {
                        match msg {
                            Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                                if event_tx.send(ShellEvent::Data(data.to_vec())).is_err() {
                                    break;
                                }
                            }
                            // Terminal conditions: the remote shell is gone.
                            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                                let _ = event_tx.send(ShellEvent::Closed);
                                break;
                            }
                            // russh forwards channel request replies (Success/Failure),
                            // flow-control (WindowAdjusted, XonXoff) and exit
                            // notifications through wait(): none of them mean the
                            // shell died. Ignoring them keeps the session alive.
                            _ => {}
                        }
                    }
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(ShellCommand::Write(data)) => {
                                if writer.write_all(&data).await.is_err()
                                    || writer.flush().await.is_err()
                                {
                                    let _ = event_tx.send(ShellEvent::Error("write to channel failed".into()));
                                    break;
                                }
                            }
                            Some(ShellCommand::Resize(c, r)) => {
                                let _ = write_half.window_change(c as u32, r as u32, 0, 0).await;
                            }
                            Some(ShellCommand::Close) | None => break,
                        }
                    }
                }
            }
            let _ = event_tx.send(ShellEvent::Closed);
        });

        Ok(ShellChannel {
            events: event_rx,
            commands: cmd_tx,
            _task: task,
        })
    }

    /// Run a one-shot command and collect its output.
    pub async fn exec(&self, id: SessionId, command: &str) -> Result<ExecOutput, SshError> {
        self.exec_timeout(id, command, Duration::from_secs(120))
            .await
    }

    /// Run a one-shot command with an explicit timeout.
    pub async fn exec_timeout(
        &self,
        id: SessionId,
        command: &str,
        timeout: Duration,
    ) -> Result<ExecOutput, SshError> {
        let work = async {
            let handle = self.handle_for(id).await?;
            let channel = handle
                .lock()
                .await
                .channel_open_session()
                .await
                .map_err(|e| SshError::Channel(format!("open session channel: {e}")))?;
            let (mut read_half, write_half) = channel.split();
            write_half
                .exec(true, command.as_bytes().to_vec())
                .await
                .map_err(|e| SshError::Channel(format!("exec request: {e}")))?;

            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let mut exit_code = None;
            while let Some(msg) = read_half.wait().await {
                match msg {
                    ChannelMsg::Data { data } => stdout.extend_from_slice(&data),
                    ChannelMsg::ExtendedData { data, .. } => stderr.extend_from_slice(&data),
                    ChannelMsg::ExitStatus { exit_status } => exit_code = Some(exit_status),
                    ChannelMsg::Eof | ChannelMsg::Close => break,
                    _ => {}
                }
            }
            Ok(ExecOutput {
                stdout: String::from_utf8_lossy(&stdout).into_owned(),
                stderr: String::from_utf8_lossy(&stderr).into_owned(),
                exit_code,
            })
        };
        tokio::time::timeout(timeout, work)
            .await
            .map_err(|_| SshError::Timeout(format!("exec timed out: {command}")))?
    }

    /// Start a local port forward: `bind` → `remote_host:remote_port` via the session.
    pub async fn start_local_forward(
        &self,
        id: SessionId,
        bind: SocketAddr,
        remote_host: String,
        remote_port: u16,
    ) -> Result<LocalForward, SshError> {
        let handle = self.handle_for(id).await?;
        let listener = tokio::net::TcpListener::bind(bind).await?;
        let local_addr = listener.local_addr()?;
        tracing::info!("local forward {local_addr} -> {remote_host}:{remote_port}");

        let task = tokio::spawn(async move {
            loop {
                let Ok((inbound, peer)) = listener.accept().await else {
                    break;
                };
                let handle = handle.clone();
                let remote_host = remote_host.clone();
                tokio::spawn(async move {
                    let channel = match handle
                        .lock()
                        .await
                        .channel_open_direct_tcpip(
                            remote_host.as_str(),
                            remote_port as u32,
                            "127.0.0.1",
                            0,
                        )
                        .await
                    {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::warn!("forward for {peer} failed: {e}");
                            return;
                        }
                    };
                    let stream = channel.into_stream();
                    let (mut ri, mut wi) = tokio::io::split(inbound);
                    let (mut rs, mut ws) = tokio::io::split(stream);
                    let a = tokio::io::copy(&mut ri, &mut ws);
                    let b = tokio::io::copy(&mut rs, &mut wi);
                    tokio::select! {
                        _ = a => {},
                        _ = b => {},
                    }
                });
            }
        });

        Ok(LocalForward { local_addr, task })
    }

    /// Gracefully disconnect a session and forget it.
    pub async fn disconnect(&self, id: SessionId) -> Result<(), SshError> {
        let handle = {
            let mut inner = self.inner.lock().await;
            let s = inner
                .sessions
                .remove(&id)
                .ok_or(SshError::SessionNotFound)?;
            s.handle
        };
        if let Some(h) = handle {
            let _ = h
                .lock()
                .await
                .disconnect(Disconnect::ByApplication, "client disconnect", "")
                .await;
        }
        Ok(())
    }

    /// Re-run the full connect flow for a session's config.
    pub async fn reconnect(
        &self,
        id: SessionId,
        creds: &dyn CredentialProvider,
    ) -> Result<SessionId, SshError> {
        let config = {
            let inner = self.inner.lock().await;
            inner
                .sessions
                .get(&id)
                .ok_or(SshError::SessionNotFound)?
                .config
                .clone()
        };
        self.set_state(id, SessionState::Reconnecting, None).await;
        let _ = self.disconnect(id).await;
        self.connect(&config, creds).await
    }
}

/// SSH engine interface (docs §6.3), implemented by [`SessionManager`].
#[async_trait]
pub trait SshEngine: Send + Sync {
    async fn connect(
        &self,
        config: &HostConfig,
        creds: &dyn CredentialProvider,
    ) -> Result<SessionId, SshError>;

    async fn open_shell(
        &self,
        session: SessionId,
        cols: u16,
        rows: u16,
    ) -> Result<ShellChannel, SshError>;

    async fn write_stdin(&self, channel: &ShellChannel, data: Vec<u8>) -> Result<(), SshError>;

    async fn resize(&self, channel: &ShellChannel, cols: u16, rows: u16) -> Result<(), SshError>;

    async fn exec(&self, session: SessionId, command: &str) -> Result<ExecOutput, SshError>;

    async fn disconnect(&self, session: SessionId) -> Result<(), SshError>;
}

#[async_trait]
impl SshEngine for SessionManager {
    async fn connect(
        &self,
        config: &HostConfig,
        creds: &dyn CredentialProvider,
    ) -> Result<SessionId, SshError> {
        SessionManager::connect(self, config, creds).await
    }

    async fn open_shell(
        &self,
        session: SessionId,
        cols: u16,
        rows: u16,
    ) -> Result<ShellChannel, SshError> {
        SessionManager::open_shell(self, session, cols, rows).await
    }

    async fn write_stdin(&self, channel: &ShellChannel, data: Vec<u8>) -> Result<(), SshError> {
        channel
            .commands
            .send(ShellCommand::Write(data))
            .map_err(|_| SshError::Channel("shell task is gone".into()))
    }

    async fn resize(&self, channel: &ShellChannel, cols: u16, rows: u16) -> Result<(), SshError> {
        channel
            .commands
            .send(ShellCommand::Resize(cols, rows))
            .map_err(|_| SshError::Channel("shell task is gone".into()))
    }

    async fn exec(&self, session: SessionId, command: &str) -> Result<ExecOutput, SshError> {
        SessionManager::exec(self, session, command).await
    }

    async fn disconnect(&self, session: SessionId) -> Result<(), SshError> {
        SessionManager::disconnect(self, session).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_state_usable() {
        assert!(SessionState::Ready.is_usable());
        assert!(!SessionState::Connecting.is_usable());
        assert!(!SessionState::Failed.is_usable());
        assert!(!SessionState::Disconnected.is_usable());
    }

    #[tokio::test]
    async fn manager_starts_empty() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let dir = std::env::temp_dir().join(format!("opsdeck-test-{}", Uuid::new_v4()));
        let mgr = SessionManager::new(tx, &dir.join("known_hosts.json"));
        assert!(mgr.list().await.is_empty());
        assert!(mgr.state(Uuid::new_v4()).await.is_none());
        assert!(matches!(
            mgr.disconnect(Uuid::new_v4()).await,
            Err(SshError::SessionNotFound)
        ));
    }

    #[tokio::test]
    async fn known_hosts_tofu_roundtrip() {
        let dir = std::env::temp_dir().join(format!("opsdeck-test-{}", Uuid::new_v4()));
        let path = dir.join("kh.json");
        let mut db = KnownHostsDb::load(&path);
        assert!(db.lookup("example.com:22").is_none());
        db.insert("example.com:22".into(), "ssh-ed25519 AAAA...".into());
        assert_eq!(db.lookup("example.com:22"), Some("ssh-ed25519 AAAA..."));
        // Reload from disk.
        let db2 = KnownHostsDb::load(&path);
        assert_eq!(db2.lookup("example.com:22"), Some("ssh-ed25519 AAAA..."));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn host_key_decision_is_copy() {
        let d = HostKeyDecision::TrustOnce;
        let _ = d;
    }
}

// ---------------------------------------------------------------------------
// Outbound proxies (HTTP CONNECT / SOCKS5)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProxyKind {
    Http,
    Socks5,
}

#[derive(Debug, Clone)]
struct ProxyUrl {
    kind: ProxyKind,
    host: String,
    port: u16,
    user: Option<String>,
    pass: Option<String>,
}

/// Parse `socks5://[user:pass@]host[:port]` / `http://...` proxy URLs.
fn parse_proxy_url(url: &str) -> Option<ProxyUrl> {
    let (kind, rest, default_port) = if let Some(r) = url.strip_prefix("socks5://") {
        (ProxyKind::Socks5, r, 1080)
    } else {
        let r = url.strip_prefix("http://")?;
        (ProxyKind::Http, r, 8080)
    };
    let rest = rest.split('/').next().unwrap_or("");
    let (userinfo, hostport) = match rest.rsplit_once('@') {
        Some((ui, hp)) => (Some(ui), hp),
        None => (None, rest),
    };
    let (user, pass) = match userinfo {
        Some(ui) => match ui.split_once(':') {
            Some((u, p)) => (Some(u.to_string()), Some(p.to_string())),
            None => (Some(ui.to_string()), None),
        },
        None => (None, None),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().ok()?),
        None => (hostport.to_string(), default_port),
    };
    if host.is_empty() {
        return None;
    }
    Some(ProxyUrl {
        kind,
        host,
        port,
        user,
        pass,
    })
}

async fn socks5_handshake(
    tcp: &mut tokio::net::TcpStream,
    proxy: &ProxyUrl,
    target_host: &str,
    target_port: u16,
) -> Result<(), SshError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    // (imports used below)
    // Method negotiation: offer no-auth, plus username/password when given.
    let mut greet = vec![0x05u8, if proxy.user.is_some() { 2 } else { 1 }];
    greet.push(0x00);
    if proxy.user.is_some() {
        greet.push(0x02);
    }
    tcp.write_all(&greet)
        .await
        .map_err(|e| SshError::Connection(format!("socks5 greet failed: {e}")))?;
    let mut method = [0u8; 2];
    tcp.read_exact(&mut method)
        .await
        .map_err(|e| SshError::Connection(format!("socks5 method reply failed: {e}")))?;
    if method[0] != 0x05 {
        return Err(SshError::Connection("socks5: bad version".into()));
    }
    match method[1] {
        0x00 => {}
        0x02 => {
            let user = proxy.user.as_deref().unwrap_or("");
            let pass = proxy.pass.as_deref().unwrap_or("");
            let ub = user.as_bytes();
            let pb = pass.as_bytes();
            if ub.len() > 255 || pb.len() > 255 {
                return Err(SshError::Connection("socks5 auth too long".into()));
            }
            let mut auth = vec![0x01u8, ub.len() as u8];
            auth.extend_from_slice(ub);
            auth.push(pb.len() as u8);
            auth.extend_from_slice(pb);
            tcp.write_all(&auth)
                .await
                .map_err(|e| SshError::Connection(format!("socks5 auth failed: {e}")))?;
            let mut resp = [0u8; 2];
            tcp.read_exact(&mut resp)
                .await
                .map_err(|e| SshError::Connection(format!("socks5 auth reply failed: {e}")))?;
            if resp[1] != 0x00 {
                return Err(SshError::Connection("socks5 auth rejected".into()));
            }
        }
        _ => return Err(SshError::Connection("socks5: no acceptable method".into())),
    }
    // CONNECT request (domain-name address form).
    let hb = target_host.as_bytes();
    if hb.len() > 255 {
        return Err(SshError::Connection("socks5 target name too long".into()));
    }
    let mut req = vec![0x05u8, 0x01, 0x00, 0x03, hb.len() as u8];
    req.extend_from_slice(hb);
    req.extend_from_slice(&target_port.to_be_bytes());
    tcp.write_all(&req)
        .await
        .map_err(|e| SshError::Connection(format!("socks5 connect failed: {e}")))?;
    let mut reply = [0u8; 4];
    tcp.read_exact(&mut reply)
        .await
        .map_err(|e| SshError::Connection(format!("socks5 connect reply failed: {e}")))?;
    if reply[0] != 0x05 || reply[1] != 0x00 {
        return Err(SshError::Connection(format!(
            "socks5 connect rejected (rep {:#04x})",
            reply[1]
        )));
    }
    // Skip the bound address.
    let skip = match reply[3] {
        0x01 => 4 + 2,
        0x03 => {
            let mut len = [0u8; 1];
            tcp.read_exact(&mut len)
                .await
                .map_err(|e| SshError::Connection(format!("socks5 bnd.addr failed: {e}")))?;
            len[0] as usize + 2
        }
        0x04 => 16 + 2,
        _ => return Err(SshError::Connection("socks5: bad address type".into())),
    };
    let mut buf = vec![0u8; skip];
    tcp.read_exact(&mut buf)
        .await
        .map_err(|e| SshError::Connection(format!("socks5 bnd.addr failed: {e}")))?;
    Ok(())
}

async fn http_connect(
    tcp: &mut tokio::net::TcpStream,
    proxy: &ProxyUrl,
    target_host: &str,
    target_port: u16,
) -> Result<(), SshError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut request = format!(
        "CONNECT {target_host}:{target_port} HTTP/1.1\r\nHost: {target_host}:{target_port}\r\n"
    );
    if let Some(user) = &proxy.user {
        let pass = proxy.pass.as_deref().unwrap_or("");
        let cred = base64_line(format!("{user}:{pass}").as_bytes());
        request.push_str(&format!("Proxy-Authorization: Basic {cred}\r\n"));
    }
    request.push_str("\r\n");
    tcp.write_all(request.as_bytes())
        .await
        .map_err(|e| SshError::Connection(format!("proxy CONNECT failed: {e}")))?;
    // Read the response header (bounded).
    let mut head = Vec::new();
    let mut one = [0u8; 1];
    while head.len() < 8192 {
        tcp.read_exact(&mut one)
            .await
            .map_err(|e| SshError::Connection(format!("proxy response failed: {e}")))?;
        head.push(one[0]);
        if head.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let status = String::from_utf8_lossy(&head);
    let first_line = status.lines().next().unwrap_or("");
    if !first_line.contains(" 200") {
        return Err(SshError::Connection(format!(
            "proxy refused CONNECT: {first_line}"
        )));
    }
    Ok(())
}

/// Minimal base64 (padding included); std-only so we do not add a dependency.
fn base64_line(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut n: u32 = 0;
        for &b in chunk {
            n = (n << 8) | b as u32;
        }
        n <<= 8 * (3 - chunk.len());
        let pad = 3 - chunk.len();
        for i in 0..4 {
            if i < 4 - pad {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod proxy_tests {
    use super::{ProxyKind, base64_line, parse_proxy_url};

    #[test]
    fn proxy_url_parsing() {
        let p = parse_proxy_url("socks5://127.0.0.1:1080").unwrap();
        assert_eq!(p.kind, ProxyKind::Socks5);
        assert_eq!(p.port, 1080);
        assert!(p.user.is_none());

        let p = parse_proxy_url("socks5://u:p@proxy.example.com").unwrap();
        assert_eq!(p.port, 1080);
        assert_eq!(p.user.as_deref(), Some("u"));
        assert_eq!(p.pass.as_deref(), Some("p"));

        let p = parse_proxy_url("http://proxy:3128").unwrap();
        assert_eq!(p.kind, ProxyKind::Http);
        assert_eq!(p.port, 3128);

        let p = parse_proxy_url("socks5://host").unwrap();
        assert_eq!(p.port, 1080);

        assert!(parse_proxy_url("ftp://host").is_none());
        assert!(parse_proxy_url("socks5://").is_none());
    }

    #[test]
    fn base64_works() {
        assert_eq!(base64_line(b"user:pass"), "dXNlcjpwYXNz");
        assert_eq!(base64_line(b""), "");
    }
}
