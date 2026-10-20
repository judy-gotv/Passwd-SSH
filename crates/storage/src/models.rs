//! Domain models: hosts, credentials, groups, snippets, transfers, metrics.
//!
//! These mirror the SQL schema in `migrations/001_init.sql` (docs §5) and the
//! API types in docs §9.
//!
//! Author: Passwd Word

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Current unix timestamp in seconds.
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Generate a new v4 UUID string for primary keys.
pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

// ---------------------------------------------------------------------------
// Hosts
// ---------------------------------------------------------------------------

/// Authentication method for an SSH host (docs §9.1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthMethod {
    /// Password authentication; the password lives in the vault credential
    /// referenced by `Host::credential_id`.
    Password,
    /// Private-key authentication.
    PrivateKey {
        key_id: Uuid,
        passphrase_id: Option<Uuid>,
    },
    /// SSH agent authentication.
    Agent,
}

impl AuthMethod {
    /// Short stable name stored in `hosts.auth_type`.
    pub fn type_name(&self) -> &'static str {
        match self {
            AuthMethod::Password => "password",
            AuthMethod::PrivateKey { .. } => "private_key",
            AuthMethod::Agent => "agent",
        }
    }

    /// Human-readable label for the UI.
    pub fn label(&self) -> &'static str {
        match self {
            AuthMethod::Password => "Password",
            AuthMethod::PrivateKey { .. } => "Private key",
            AuthMethod::Agent => "SSH agent",
        }
    }
}

/// A managed SSH host (docs §5.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: String,
    pub name: String,
    pub group_id: Option<String>,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    pub credential_id: Option<String>,
    /// Optional second credential holding the key passphrase.
    /// (Extension beyond docs §5.1; nullable, backwards compatible.)
    pub passphrase_credential_id: Option<String>,
    pub proxy_jump_id: Option<String>,
    pub startup_command: Option<String>,
    /// Connection protocol: "ssh" | "telnet" | "vnc" | "rdp".
    pub protocol: String,
    /// Free-form note shown in the manager tooltip.
    pub note: Option<String>,
    /// TCP/SSH connect timeout (seconds). None = 20s default.
    pub connect_timeout_secs: Option<i64>,
    /// Protocol keepalive interval (seconds). None = 30s default, 0 = off.
    pub keepalive_secs: Option<i64>,
    /// PTY terminal type (e.g. "xterm-256color"). None = xterm-256color.
    pub term_type: Option<String>,
    /// Working directory to `cd` into right after the shell opens.
    pub default_path: Option<String>,
    /// Outbound proxy URL (socks5://host:port or http://host:port).
    /// None = direct connection.
    pub proxy_url: Option<String>,
    /// JSON object of session environment variables ({"K": "V"}).
    pub env_json: Option<String>,
    /// Command run via exec every `keepalive_cmd_secs` to keep NAT/firewall
    /// sessions from idling out. None = protocol keepalive only.
    pub keepalive_cmd: Option<String>,
    pub keepalive_cmd_secs: Option<i64>,
    /// JSON array of tag strings.
    pub tags: String,
    pub favorite: bool,
    pub last_connected_at: Option<i64>,
    /// Successful connection count (drives the Frequent view).
    #[serde(default)]
    pub connect_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Host {
    /// Build a new host with sensible defaults.
    pub fn new(
        name: impl Into<String>,
        host: impl Into<String>,
        username: impl Into<String>,
    ) -> Self {
        let now = now_unix();
        Self {
            id: new_id(),
            name: name.into(),
            group_id: None,
            host: host.into(),
            port: 22,
            username: username.into(),
            auth_type: "password".to_string(),
            credential_id: None,
            passphrase_credential_id: None,
            proxy_jump_id: None,
            startup_command: None,
            protocol: "ssh".to_string(),
            note: None,
            connect_timeout_secs: None,
            keepalive_secs: None,
            term_type: None,
            default_path: None,
            proxy_url: None,
            env_json: None,
            keepalive_cmd: None,
            keepalive_cmd_secs: None,
            tags: "[]".to_string(),
            favorite: false,
            last_connected_at: None,
            connect_count: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// Decode the auth method from the stored columns.
    pub fn auth_method(&self) -> Result<AuthMethod, crate::StorageError> {
        let parse_uuid = |s: &str| {
            Uuid::parse_str(s).map_err(|e| crate::StorageError::InvalidData(e.to_string()))
        };
        match self.auth_type.as_str() {
            "password" => Ok(AuthMethod::Password),
            "agent" => Ok(AuthMethod::Agent),
            "private_key" => {
                let key_id = self
                    .credential_id
                    .as_deref()
                    .ok_or_else(|| {
                        crate::StorageError::InvalidData(
                            "private_key without key credential".into(),
                        )
                    })
                    .and_then(parse_uuid)?;
                let passphrase_id = self
                    .passphrase_credential_id
                    .as_deref()
                    .map(parse_uuid)
                    .transpose()?;
                Ok(AuthMethod::PrivateKey {
                    key_id,
                    passphrase_id,
                })
            }
            other => Err(crate::StorageError::InvalidData(format!(
                "unknown auth_type: {other}"
            ))),
        }
    }

    /// Tags as a string vector.
    pub fn tag_list(&self) -> Vec<String> {
        serde_json::from_str(&self.tags).unwrap_or_default()
    }

    /// Runtime connection config (docs §9.1).
    pub fn to_config(&self) -> Result<HostConfig, crate::StorageError> {
        Ok(HostConfig {
            id: Uuid::parse_str(&self.id)
                .map_err(|e| crate::StorageError::InvalidData(e.to_string()))?,
            name: self.name.clone(),
            host: self.host.clone(),
            port: self.port as u16,
            username: self.username.clone(),
            auth: self.auth_method()?,
            proxy_jump: self
                .proxy_jump_id
                .as_deref()
                .map(Uuid::parse_str)
                .transpose()
                .map_err(|e| crate::StorageError::InvalidData(e.to_string()))?,
            startup_command: self.startup_command.clone(),
            tags: self.tag_list(),
            connect_timeout_secs: self
                .connect_timeout_secs
                .and_then(|v| u32::try_from(v).ok()),
            keepalive_secs: self.keepalive_secs.and_then(|v| u32::try_from(v).ok()),
            term_type: self.term_type.clone(),
            default_path: self.default_path.clone(),
            proxy_url: self.proxy_url.clone(),
            env: self.env_pairs(),
            keepalive_cmd: self.keepalive_cmd.clone(),
            keepalive_cmd_secs: self.keepalive_cmd_secs.and_then(|v| u32::try_from(v).ok()),
        })
    }

    /// Decode `env_json` into (name, value) pairs.
    pub fn env_pairs(&self) -> Vec<(String, String)> {
        let Some(json) = &self.env_json else {
            return Vec::new();
        };
        let Ok(map) = serde_json::from_str::<std::collections::BTreeMap<String, String>>(json)
        else {
            return Vec::new();
        };
        map.into_iter().collect()
    }
}

/// Runtime SSH connection configuration (docs §9.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostConfig {
    pub id: Uuid,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    pub proxy_jump: Option<Uuid>,
    pub startup_command: Option<String>,
    pub tags: Vec<String>,
    /// Connect timeout in seconds. None = 20s default.
    pub connect_timeout_secs: Option<u32>,
    /// Protocol keepalive interval in seconds. None = 30s, Some(0) = off.
    pub keepalive_secs: Option<u32>,
    /// PTY terminal type. None = "xterm-256color".
    pub term_type: Option<String>,
    /// `cd` target applied right after the shell opens.
    pub default_path: Option<String>,
    /// Outbound proxy URL (socks5:// / http://). None = direct.
    pub proxy_url: Option<String>,
    /// Session environment variables to request on shell open.
    pub env: Vec<(String, String)>,
    /// Command exec'd every `keepalive_cmd_secs` seconds. None = off.
    pub keepalive_cmd: Option<String>,
    pub keepalive_cmd_secs: Option<u32>,
}

impl HostConfig {
    /// A bare config with default options (tests / ad-hoc connections).
    pub fn bare(id: Uuid, name: String, host: String, port: u16, username: String) -> Self {
        Self {
            id,
            name,
            host,
            port,
            username,
            auth: AuthMethod::Password,
            proxy_jump: None,
            startup_command: None,
            tags: Vec::new(),
            connect_timeout_secs: None,
            keepalive_secs: None,
            term_type: None,
            default_path: None,
            proxy_url: None,
            env: Vec::new(),
            keepalive_cmd: None,
            keepalive_cmd_secs: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Credentials
// ---------------------------------------------------------------------------

/// Credential kinds (docs §5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    Password,
    PrivateKey,
    Agent,
}

impl CredentialKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CredentialKind::Password => "password",
            CredentialKind::PrivateKey => "private_key",
            CredentialKind::Agent => "agent",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "password" => Some(CredentialKind::Password),
            "private_key" => Some(CredentialKind::PrivateKey),
            "agent" => Some(CredentialKind::Agent),
            _ => None,
        }
    }
}

/// An encrypted credential (docs §5.2).
///
/// `ciphertext`/`nonce` are produced by [`opsdeck_vault`](crate) — storage
/// never sees plaintext.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Credential {
    pub fn kind_enum(&self) -> Option<CredentialKind> {
        CredentialKind::parse(&self.kind)
    }
}

/// Credential metadata without secret bytes (for listings).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialMeta {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub created_at: i64,
    pub updated_at: i64,
}

// ---------------------------------------------------------------------------
// Host groups
// ---------------------------------------------------------------------------

/// A host group, optionally nested (docs §5.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostGroup {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub sort_order: i64,
    pub created_at: i64,
    /// Optional display color (hex "#rrggbb") picked in the group dialog.
    #[serde(default)]
    pub color: Option<String>,
}

// ---------------------------------------------------------------------------
// Snippets
// ---------------------------------------------------------------------------

/// A saved command snippet (docs §5.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    pub id: String,
    pub title: String,
    pub command: String,
    pub description: Option<String>,
    pub host_id: Option<String>,
    pub tags: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Snippet {
    pub fn tag_list(&self) -> Vec<String> {
        serde_json::from_str(&self.tags).unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Transfer tasks
// ---------------------------------------------------------------------------

/// Transfer direction (docs §5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferDirection {
    Upload,
    Download,
}

impl TransferDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            TransferDirection::Upload => "upload",
            TransferDirection::Download => "download",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "upload" => Some(TransferDirection::Upload),
            "download" => Some(TransferDirection::Download),
            _ => None,
        }
    }
}

/// Transfer task status (docs §5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Canceled,
}

impl TransferStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TransferStatus::Pending => "pending",
            TransferStatus::Running => "running",
            TransferStatus::Paused => "paused",
            TransferStatus::Completed => "completed",
            TransferStatus::Failed => "failed",
            TransferStatus::Canceled => "canceled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(TransferStatus::Pending),
            "running" => Some(TransferStatus::Running),
            "paused" => Some(TransferStatus::Paused),
            "completed" => Some(TransferStatus::Completed),
            "failed" => Some(TransferStatus::Failed),
            "canceled" => Some(TransferStatus::Canceled),
            _ => None,
        }
    }

    /// Whether the task is in a terminal state.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TransferStatus::Completed | TransferStatus::Failed | TransferStatus::Canceled
        )
    }
}

/// A file transfer task (docs §5.5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferTask {
    pub id: String,
    pub host_id: String,
    pub direction: String,
    pub local_path: String,
    pub remote_path: String,
    pub status: String,
    pub bytes_total: Option<i64>,
    pub bytes_done: i64,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl TransferTask {
    pub fn direction_enum(&self) -> Option<TransferDirection> {
        TransferDirection::parse(&self.direction)
    }

    pub fn status_enum(&self) -> Option<TransferStatus> {
        TransferStatus::parse(&self.status)
    }

    /// Progress ratio in `[0.0, 1.0]`, if the total size is known.
    pub fn progress(&self) -> Option<f64> {
        match self.bytes_total {
            Some(total) if total > 0 => {
                Some((self.bytes_done as f64 / total as f64).clamp(0.0, 1.0))
            }
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Metric snapshots
// ---------------------------------------------------------------------------

/// A collected metric snapshot (docs §5.6, §9.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSnapshot {
    pub id: Option<i64>,
    pub host_id: String,
    pub collected_at: i64,
    pub cpu_percent: Option<f64>,
    pub memory_used_bytes: Option<i64>,
    pub memory_total_bytes: Option<i64>,
    pub disk_used_bytes: Option<i64>,
    pub disk_total_bytes: Option<i64>,
    pub load_1: Option<f64>,
    pub load_5: Option<f64>,
    pub load_15: Option<f64>,
    pub network_rx_bytes: Option<i64>,
    pub network_tx_bytes: Option<i64>,
    /// Buff/cache bytes from `free -b` (dashboard memory legend).
    #[serde(default)]
    pub memory_cached_bytes: Option<i64>,
    /// Cumulative disk sectors read/written (bytes) for rate deltas.
    #[serde(default)]
    pub disk_read_bytes: Option<i64>,
    #[serde(default)]
    pub disk_write_bytes: Option<i64>,
    /// Per-core busy percentages (dashboard core strip).
    #[serde(default)]
    pub cpu_per_core: Vec<f64>,
}

impl MetricSnapshot {
    /// Memory usage ratio in `[0.0, 1.0]`.
    pub fn memory_ratio(&self) -> Option<f64> {
        match (self.memory_used_bytes, self.memory_total_bytes) {
            (Some(used), Some(total)) if total > 0 => Some(used as f64 / total as f64),
            _ => None,
        }
    }

    /// Disk usage ratio in `[0.0, 1.0]`.
    pub fn disk_ratio(&self) -> Option<f64> {
        match (self.disk_used_bytes, self.disk_total_bytes) {
            (Some(used), Some(total)) if total > 0 => Some(used as f64 / total as f64),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Import / export
// ---------------------------------------------------------------------------

/// Portable export format for hosts + groups + snippets (no secrets).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportData {
    pub format: String,
    pub version: u32,
    pub hosts: Vec<Host>,
    pub groups: Vec<HostGroup>,
    pub snippets: Vec<Snippet>,
}

impl ExportData {
    pub fn new(hosts: Vec<Host>, groups: Vec<HostGroup>, snippets: Vec<Snippet>) -> Self {
        Self {
            format: "opsdeck-export".to_string(),
            version: 1,
            hosts,
            groups,
            snippets,
        }
    }
}
