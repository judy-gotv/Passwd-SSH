//! OpsDeck UI: egui views.
//!
//! Architecture (docs §12.1): the UI never touches the database or the
//! network. The app builds a [`UiData`] snapshot every frame, [`render`]
//! draws it, and user actions come back as [`UiEvent`]s for the app to
//! execute.
//!
//! Author: Passwd Word

use std::collections::HashMap;

use opsdeck_ops::{ProcessInfo, SysInfo};
use opsdeck_sftp::FileEntry;
use opsdeck_ssh::{HostKeyDecision, SessionId, SessionInfo};
use opsdeck_storage::models::{
    CredentialMeta, Host, HostGroup, MetricSnapshot, Snippet, TransferTask,
};

pub mod i18n;
pub mod shortcuts;
pub mod state;
pub mod theme;
pub mod views;

pub use i18n::Language;

pub use state::{
    ConfirmDialog, GroupFilter, HostForm, MainTab, Pane, Screen, Selection, SnippetForm, TermTab,
    Toast, UiState,
};
pub use theme::Theme;

/// Actions the app must execute.
#[derive(Debug, Clone)]
pub enum UiEvent {
    // vault
    CreateVault(String),
    UnlockVault(String),
    LockVault,
    ChangeMasterPassword {
        old: String,
        new: String,
    },
    // hosts
    ConnectHost(String),
    DisconnectSession(SessionId),
    ReconnectSession(SessionId),
    SaveHost(Box<HostForm>),
    DeleteHost(String),
    ToggleFavorite(String),
    ImportHosts(String),
    ExportHosts,
    PingHosts,
    CreateGroup(String),
    DeleteGroup(String),
    TestConnection(Box<HostForm>),
    // terminal
    TerminalInput(SessionId, Vec<u8>),
    TerminalResize(SessionId, u16, u16),
    NewTabForHost(String),
    SplitPane {
        vertical: bool,
    },
    ClosePane(SessionId),
    HostKeyDecision {
        prompt_id: u64,
        decision: HostKeyDecision,
    },
    // sftp
    SftpNavigate {
        host_id: String,
        path: String,
    },
    SftpMkdir {
        host_id: String,
        path: String,
    },
    SftpDelete {
        host_id: String,
        path: String,
    },
    SftpRename {
        host_id: String,
        from: String,
        to: String,
    },
    SftpUpload {
        host_id: String,
        local: String,
        remote: String,
    },
    SftpDownload {
        host_id: String,
        remote: String,
        local: String,
    },
    TransferPause(String),
    TransferRetry(String),
    TransferCancel(String),
    // monitor
    MonitorSelect(String),
    // snippets
    RunSnippet {
        snippet_id: String,
        session: SessionId,
    },
    SaveSnippet(SnippetForm),
    DeleteSnippet(String),
    // batch
    BatchRun {
        sessions: Vec<SessionId>,
        command: String,
        allow_dangerous: bool,
    },
    // forwarding
    StartForward {
        session: SessionId,
        bind: String,
        remote_host: String,
        remote_port: u16,
    },
    StopForward(usize),
    // sync
    SyncNow,
    SetSyncKind(String),
    // settings
    SetTheme(String),
    SetLanguage(String),
    SetFontSize(f32),
    SetTerminalFontSize(f32),
    CheckUpdate,
    OpenUrl(String),
}

/// A host-key approval waiting for the user.
#[derive(Debug, Clone)]
pub struct KeyPromptView {
    pub id: u64,
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub key_openssh: String,
    pub changed: bool,
}

/// A cached SFTP directory listing.
#[derive(Debug, Clone, Default)]
pub struct SftpListing {
    pub path: String,
    pub entries: Vec<FileEntry>,
    pub loading: bool,
    pub error: Option<String>,
}

/// Update-check state.
#[derive(Debug, Clone, Default)]
pub struct UpdateStatusView {
    pub checking: bool,
    pub current: String,
    pub latest: Option<String>,
    pub url: Option<String>,
    pub error: Option<String>,
    /// `true` when a newer release exists.
    pub available: bool,
}

/// Sync state.
#[derive(Debug, Clone, Default)]
pub struct SyncStatusView {
    pub running: bool,
    pub last_sync: Option<i64>,
    pub last_summary: Option<String>,
    pub error: Option<String>,
    pub configured: bool,
}

/// A running port forward for display.
#[derive(Debug, Clone)]
pub struct ForwardView {
    pub id: usize,
    pub session_name: String,
    pub local: String,
    pub remote: String,
}

/// Read-only backend snapshot for one frame.
pub struct UiData<'a> {
    pub hosts: &'a [Host],
    pub groups: &'a [HostGroup],
    pub sessions: &'a [SessionInfo],
    pub snippets: &'a [Snippet],
    pub transfers: &'a [TransferTask],
    pub credentials: &'a [CredentialMeta],
    pub sftp_listings: &'a HashMap<String, SftpListing>,
    pub metrics: &'a HashMap<String, Vec<MetricSnapshot>>,
    pub processes: &'a HashMap<String, Vec<ProcessInfo>>,
    pub key_prompts: &'a [KeyPromptView],
    pub connecting: &'a HashMap<String, String>,
    pub forwards: &'a [ForwardView],
    pub update: &'a UpdateStatusView,
    pub sync: &'a SyncStatusView,
    pub theme: &'a Theme,
    /// TCP connect latency per host id: Some(ms) or None (timeout/refused).
    /// Missing key = not measured yet.
    pub latencies: &'a HashMap<String, Option<u32>>,
    /// Static host facts (OS / kernel / uptime), fetched once per session.
    pub sysinfo: &'a HashMap<String, SysInfo>,
}

impl<'a> UiData<'a> {
    /// Find a host by id.
    pub fn host(&self, id: &str) -> Option<&'a Host> {
        self.hosts.iter().find(|h| h.id == id)
    }

    /// Find a session by id.
    pub fn session(&self, id: &SessionId) -> Option<&'a SessionInfo> {
        self.sessions.iter().find(|s| &s.id == id)
    }

    /// Host name for a session (falls back to the session's own name).
    pub fn session_host_name(&self, session: &SessionInfo) -> String {
        session.name.clone()
    }
}

/// Draw one frame.
pub fn render(ui: &mut egui::Ui, state: &mut UiState, data: &UiData, events: &mut Vec<UiEvent>) {
    let ctx = ui.ctx().clone();
    // Apply theme on change.
    if state.applied_theme != state.theme_name {
        data.theme.apply(&ctx);
        state.applied_theme = state.theme_name.clone();
    }
    state.prune_toasts();

    match state.screen {
        Screen::Locked { .. } => {
            views::vault::show(ui, state, data, events);
        }
        Screen::Main => {
            // All panels share the root Ui so they tile correctly.
            views::main::top_bar(ui, state, events);
            views::hosts::left_panel(ui, state, data, events);
            views::main::central(ui, state, data, events);
            views::main::status_bar(ui, state, data);
            views::dialogs::show_all(&ctx, state, data, events);
            views::toasts::show(&ctx, state);
        }
    }

    // Global shortcuts (main screen only).
    if matches!(state.screen, Screen::Main) {
        views::main::global_shortcuts(&ctx, state, events);
    }
}
