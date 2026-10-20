//! [`UiState`]: all view state owned by the UI layer.
//!
//! The app owns backend state (sessions, listings, metrics) and feeds it in
//! as [`UiData`][crate::UiData]; everything the user *sees and edits* lives
//! here.
//!
//! Author: Passwd Word

use std::collections::HashMap;
use std::time::{Duration, Instant};

use opsdeck_ssh::SessionId;
use opsdeck_terminal::Terminal;

use crate::i18n::Language;

/// Which top-level screen is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    /// Vault is locked (or not yet created).
    Locked {
        first_run: bool,
    },
    Main,
}

/// Main content tabs (docs §8.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainTab {
    /// Connection manager (Xterminal-style): group tree + host table.
    Manager,
    Terminal,
    Files,
    Monitor,
    Snippets,
    Settings,
}

/// Group-tree filter for the manager host table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupFilter {
    /// Every host.
    All,
    /// Starred hosts.
    Favorites,
    /// Recently connected hosts, most recent first.
    Recent,
    /// Most frequently connected hosts.
    Frequent,
    /// Hosts of one group id.
    Group(String),
}

/// One terminal pane: a live shell session.
#[derive(Debug, Clone)]
pub struct Pane {
    pub session: SessionId,
    pub host_id: String,
    pub title: String,
}

/// A terminal tab holding 1–2 panes (split view).
#[derive(Debug, Clone)]
pub struct TermTab {
    pub panes: Vec<Pane>,
    /// `true` = panes stacked vertically, `false` = side by side.
    pub vertical: bool,
    pub active_pane: usize,
}

impl TermTab {
    pub fn single(session: SessionId, host_id: String, title: String) -> Self {
        Self {
            panes: vec![Pane {
                session,
                host_id,
                title,
            }],
            vertical: false,
            active_pane: 0,
        }
    }
}

/// Text selection in a terminal (screen coordinates: (row, col)).
#[derive(Debug, Clone, Copy)]
pub struct Selection {
    pub anchor: (usize, usize),
    pub head: (usize, usize),
}

impl Selection {
    /// Normalize to ((start_row, start_col), (end_row, end_col)).
    pub fn normalized(self) -> ((usize, usize), (usize, usize)) {
        if (self.anchor.0, self.anchor.1) <= (self.head.0, self.head.1) {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
}

/// Host add/edit form.
#[derive(Debug, Clone, Default)]
pub struct HostForm {
    pub id: Option<String>,
    /// "ssh" | "telnet" | "vnc" | "rdp"
    pub protocol: String,
    pub name: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub auth_type: String, // "password" | "private_key" | "agent"
    pub group_id: String,
    pub proxy_jump_id: String,
    pub startup_command: String,
    pub note: String,
    pub connect_timeout: String,
    pub keepalive_secs: String,
    pub term_type: String,
    pub default_path: String,
    pub proxy_url: String,
    pub env_vars: String,
    pub keepalive_cmd: String,
    pub keepalive_cmd_secs: String,
    pub tags: String,
    pub favorite: bool,
    /// New password (creates/updates a credential on save; empty = keep).
    pub password: String,
    /// New private-key PEM (creates/updates a credential on save).
    pub private_key_pem: String,
    /// Passphrase for the key above.
    pub passphrase: String,
    /// Existing credential to reuse (empty = create from the fields above).
    pub credential_id: String,
    pub error: Option<String>,
}

impl HostForm {
    pub fn blank() -> Self {
        Self {
            protocol: "ssh".to_string(),
            port: "22".to_string(),
            username: "root".to_string(),
            auth_type: "password".to_string(),
            term_type: "xterm-256color".to_string(),
            keepalive_cmd_secs: "300".to_string(),
            ..Default::default()
        }
    }
}

/// Snippet add/edit form.
#[derive(Debug, Clone, Default)]
pub struct SnippetForm {
    pub id: Option<String>,
    pub title: String,
    pub command: String,
    pub description: String,
    pub tags: String,
}

/// A transient toast notification.
#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
    pub until: Instant,
    pub error: bool,
}

/// A generic confirmation dialog.
#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
    /// Event payload the app should act on when confirmed (opaque string).
    pub action: String,
}

/// All UI view state.
/// A remote-desktop tab (VNC/RDP): one running desktop session.
#[derive(Debug, Clone)]
pub struct DesktopTab {
    pub id: u64,
    pub host_id: String,
    pub title: String,
    pub kind: String,
}

pub struct UiState {
    pub screen: Screen,
    // vault (the app mirrors exists/unlocked into these every frame)
    pub password_input: String,
    pub password_confirm: String,
    pub unlock_error: Option<String>,
    pub show_vault_dialog: bool,
    pub vault_exists: bool,
    pub vault_unlocked: bool,
    // main
    pub main_tab: MainTab,
    /// Manager protocol tab: "ssh" | "rdp" | "telnet" | "tunnel" | "vnc".
    pub manager_protocol: String,
    pub group_filter: GroupFilter,
    pub new_group_name: String,
    pub host_search: String,
    pub focus_host_search: bool,
    pub selected_host: Option<String>,
    pub show_host_editor: bool,
    pub host_editor_section: usize,
    pub host_form: HostForm,
    pub import_buffer: String,
    pub confirm: Option<ConfirmDialog>,
    // terminal tabs
    pub tabs: Vec<TermTab>,
    pub active_tab: usize,
    /// Open remote-desktop tabs.
    pub desktops: Vec<DesktopTab>,
    /// The focused desktop tab, if any (shares the workspace with terminals).
    pub active_desktop: Option<u64>,
    /// Uploaded egui textures per desktop id.
    pub desktop_textures: HashMap<u64, egui::TextureHandle>,
    /// Last frame version uploaded per desktop id.
    pub desktop_frame_versions: HashMap<u64, u64>,
    pub terminals: HashMap<SessionId, Terminal>,
    /// Last size we told the backend about (cols, rows).
    pub term_sizes: HashMap<SessionId, (usize, usize)>,
    pub selections: HashMap<SessionId, Selection>,
    pub search_open: HashMap<SessionId, bool>,
    pub search_query: HashMap<SessionId, String>,
    pub search_hit_cursor: HashMap<u64, usize>,
    // sftp
    pub sftp_host: Option<String>,
    pub sftp_paths: HashMap<String, String>,
    // terminal session view
    pub show_files_panel: bool,
    pub show_monitor_panel: bool,
    /// Last auto-load request time for the files panel, per host id.
    pub files_panel_requested: HashMap<String, Instant>,
    pub sftp_mkdir_open: bool,
    pub sftp_new_dir_name: String,
    pub sftp_rename_from: Option<String>,
    pub sftp_rename_to: String,
    pub sftp_upload_local: String,
    pub sftp_download_local: String,
    // snippets
    pub snippet_search: String,
    pub show_snippet_editor: bool,
    pub snippet_form: SnippetForm,
    // monitor
    pub monitor_host: Option<String>,
    pub monitor_auto: bool,
    // batch
    pub batch_command: String,
    pub batch_selected: Vec<SessionId>,
    pub batch_allow_dangerous: bool,
    pub batch_output: Vec<(String, String)>,
    // port forwarding
    pub show_forward_dialog: bool,
    pub forward_session: Option<SessionId>,
    pub forward_bind: String,
    pub forward_remote_host: String,
    pub forward_remote_port: String,
    // settings
    pub theme_name: String,
    pub applied_theme: String,
    pub language: Language,
    pub settings_pw_old: String,
    pub settings_pw_new: String,
    pub settings_pw_confirm: String,
    pub sync_kind: String, // "off" | "file" | "http"
    pub sync_file_dir: String,
    pub sync_http_endpoint: String,
    // toasts
    pub toasts: Vec<Toast>,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            screen: Screen::Locked { first_run: true },
            password_input: String::new(),
            password_confirm: String::new(),
            unlock_error: None,
            show_vault_dialog: false,
            vault_exists: false,
            vault_unlocked: false,
            main_tab: MainTab::Manager,
            manager_protocol: "ssh".to_string(),
            group_filter: GroupFilter::All,
            new_group_name: String::new(),
            host_search: String::new(),
            focus_host_search: false,
            selected_host: None,
            show_host_editor: false,
            host_editor_section: 0,
            host_form: HostForm::blank(),
            import_buffer: String::new(),
            confirm: None,
            tabs: Vec::new(),
            active_tab: 0,
            desktops: Vec::new(),
            active_desktop: None,
            desktop_textures: HashMap::new(),
            desktop_frame_versions: HashMap::new(),
            terminals: HashMap::new(),
            term_sizes: HashMap::new(),
            selections: HashMap::new(),
            search_open: HashMap::new(),
            search_query: HashMap::new(),
            search_hit_cursor: HashMap::new(),
            sftp_host: None,
            sftp_paths: HashMap::new(),
            show_files_panel: true,
            show_monitor_panel: true,
            files_panel_requested: HashMap::new(),
            sftp_mkdir_open: false,
            sftp_new_dir_name: String::new(),
            sftp_rename_from: None,
            sftp_rename_to: String::new(),
            sftp_upload_local: String::new(),
            sftp_download_local: String::new(),
            snippet_search: String::new(),
            show_snippet_editor: false,
            snippet_form: SnippetForm::default(),
            monitor_host: None,
            monitor_auto: true,
            batch_command: String::new(),
            batch_selected: Vec::new(),
            batch_allow_dangerous: false,
            batch_output: Vec::new(),
            show_forward_dialog: false,
            forward_session: None,
            forward_bind: "127.0.0.1:8080".to_string(),
            forward_remote_host: String::new(),
            forward_remote_port: String::new(),
            theme_name: "Tokyo Night".to_string(),
            applied_theme: String::new(),
            language: Language::default(),
            settings_pw_old: String::new(),
            settings_pw_new: String::new(),
            settings_pw_confirm: String::new(),
            sync_kind: "off".to_string(),
            sync_file_dir: String::new(),
            sync_http_endpoint: String::new(),
            toasts: Vec::new(),
        }
    }

    /// Push a toast notification.
    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toasts.push(Toast {
            text: text.into(),
            until: Instant::now() + Duration::from_secs(4),
            error,
        });
    }

    /// Translate a UI string into the current language.
    ///
    /// Returns `&'static str`, so the result never borrows `self` — it can be
    /// used freely even while other fields are mutably borrowed.
    pub fn t(&self, key: &'static str) -> &'static str {
        self.language.tr(key)
    }

    /// Translate a template with `{name}` placeholders.
    pub fn tf(&self, key: &'static str, args: &[(&str, &str)]) -> String {
        self.language.tr_fmt(key, args)
    }

    /// Drop expired toasts.
    pub fn prune_toasts(&mut self) {
        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);
    }

    /// Feed backend bytes into a terminal, creating it lazily if needed.
    pub fn feed_terminal(&mut self, session: SessionId, bytes: &[u8]) {
        let term = self
            .terminals
            .entry(session)
            .or_insert_with(|| Terminal::new(80, 24));
        let was_scrolled = term.is_scrolled_up();
        term.feed(bytes);
        if !was_scrolled {
            term.scroll_to_bottom();
        }
    }

    /// Remove all state for a closed terminal session.
    pub fn drop_terminal(&mut self, session: SessionId) {
        self.terminals.remove(&session);
        self.term_sizes.remove(&session);
        self.selections.remove(&session);
        self.search_open.remove(&session);
        self.search_query.remove(&session);
        for tab in &mut self.tabs {
            tab.panes.retain(|p| p.session != session);
        }
        self.tabs.retain(|t| !t.panes.is_empty());
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len().saturating_sub(1);
        }
    }

    /// The currently visible pane, if any.
    pub fn active_pane(&self) -> Option<&Pane> {
        self.tabs
            .get(self.active_tab)?
            .panes
            .get(self.tabs[self.active_tab].active_pane)
    }

    /// SFTP path for a host (defaults to the remote home).
    pub fn sftp_path(&self, host_id: &str) -> String {
        self.sftp_paths
            .get(host_id)
            .cloned()
            .unwrap_or_else(|| ".".to_string())
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self::new()
    }
}
