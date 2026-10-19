//! Sync remotes and the sync engine.
//!
//! [`Remote`] is the transport abstraction. Two implementations ship:
//! - [`FileRemote`]: sync through a shared directory (works today, e.g. via
//!   Syncthing / Nextcloud / a USB stick).
//! - [`HttpRemote`]: sync against an OpsDeck-compatible sync server over
//!   HTTP(S). Protocol documented in `docs/sync-protocol.md`.
//!
//! The [`SyncEngine`] orchestrates export → push → pull → apply with
//! last-write-wins conflict resolution (see [`crate::conflict`]).
//!
//! Author: Passwd Word

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use opsdeck_storage::Storage;
use opsdeck_storage::models::{Host, HostGroup, Snippet, now_unix};
use opsdeck_vault::Vault;
use serde::{Deserialize, Serialize};

use crate::conflict::resolve;
use crate::error::SyncError;
use crate::http::{HttpResponse, http_request};
use crate::model::{SyncCollection, SyncEntity, SyncRecord, SyncSummary, open_entity};

// ---------------------------------------------------------------------------
// Remote trait
// ---------------------------------------------------------------------------

/// Sync transport.
#[async_trait]
pub trait Remote: Send + Sync {
    /// Upload records (idempotent: server keeps the newest version per id).
    async fn push(&self, records: &[SyncRecord]) -> Result<(), SyncError>;
    /// Download records changed after `since` (unix seconds).
    async fn pull(&self, since: i64) -> Result<Vec<SyncRecord>, SyncError>;
    /// Human-readable remote name.
    fn name(&self) -> &str;
    /// Whether the remote is configured (unconfigured → [`SyncError::NotConfigured`]).
    fn is_configured(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// File remote
// ---------------------------------------------------------------------------

/// Sync through a shared directory: each record is one JSON file.
///
/// Any file-sync tool (Syncthing, Nextcloud, rsync…) can replicate the
/// directory between devices. Fully offline-capable.
pub struct FileRemote {
    dir: PathBuf,
}

impl FileRemote {
    /// Create a file remote rooted at `dir` (created on first use).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path_for(&self, id: &str) -> PathBuf {
        let safe: String = id.chars().map(|c| if c == ':' { '_' } else { c }).collect();
        self.dir.join(format!("{safe}.json"))
    }
}

#[async_trait]
impl Remote for FileRemote {
    async fn push(&self, records: &[SyncRecord]) -> Result<(), SyncError> {
        tokio::fs::create_dir_all(&self.dir).await?;
        for record in records {
            let json = serde_json::to_vec_pretty(record)?;
            tokio::fs::write(self.path_for(&record.id), json).await?;
        }
        Ok(())
    }

    async fn pull(&self, since: i64) -> Result<Vec<SyncRecord>, SyncError> {
        let mut out = Vec::new();
        let mut entries = match tokio::fs::read_dir(&self.dir).await {
            Ok(e) => e,
            Err(_) => return Ok(out), // no dir yet → nothing to pull
        };
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| SyncError::Io(e.to_string()))?
        {
            if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let data = tokio::fs::read(entry.path()).await?;
            let record: SyncRecord = serde_json::from_slice(&data).map_err(|e| {
                SyncError::Malformed(entry.path().display().to_string(), e.to_string())
            })?;
            if record.updated_at > since {
                out.push(record);
            }
        }
        Ok(out)
    }

    fn name(&self) -> &str {
        "file"
    }
}

// ---------------------------------------------------------------------------
// HTTP remote
// ---------------------------------------------------------------------------

/// Sync against an OpsDeck-compatible sync server.
///
/// Protocol (`docs/sync-protocol.md`):
/// - `POST {endpoint}/v1/push` with `{device_id, records:[...]}` → `200`
/// - `GET  {endpoint}/v1/pull?since=<unix>` → `200 {records:[...]}`
/// - `GET  {endpoint}/v1/health` → `200 {ok:true}`
#[derive(Debug, Clone)]
pub struct HttpRemote {
    endpoint: Option<String>,
    device_id: String,
}

impl HttpRemote {
    /// Create an unconfigured remote (sync disabled until configured).
    pub fn unconfigured(device_id: &str) -> Self {
        Self {
            endpoint: None,
            device_id: device_id.to_string(),
        }
    }

    /// Create a remote pointed at a sync server, e.g. `https://sync.example.com`.
    pub fn with_endpoint(endpoint: &str, device_id: &str) -> Self {
        Self {
            endpoint: Some(endpoint.trim_end_matches('/').to_string()),
            device_id: device_id.to_string(),
        }
    }

    /// Check server reachability.
    pub async fn health(&self) -> Result<bool, SyncError> {
        let endpoint = self.endpoint.as_deref().ok_or(SyncError::NotConfigured)?;
        let resp = http_request("GET", &format!("{endpoint}/v1/health"), None, &[]).await?;
        Ok(resp.status == 200)
    }
}

#[async_trait]
impl Remote for HttpRemote {
    async fn push(&self, records: &[SyncRecord]) -> Result<(), SyncError> {
        let endpoint = self.endpoint.as_deref().ok_or(SyncError::NotConfigured)?;
        #[derive(Serialize)]
        struct PushBody<'a> {
            device_id: &'a str,
            records: &'a [SyncRecord],
        }
        let body = serde_json::to_vec(&PushBody {
            device_id: &self.device_id,
            records,
        })?;
        let resp: HttpResponse =
            http_request("POST", &format!("{endpoint}/v1/push"), Some(&body), &[]).await?;
        if resp.status != 200 {
            return Err(SyncError::Remote(format!("push -> HTTP {}", resp.status)));
        }
        Ok(())
    }

    async fn pull(&self, since: i64) -> Result<Vec<SyncRecord>, SyncError> {
        let endpoint = self.endpoint.as_deref().ok_or(SyncError::NotConfigured)?;
        let resp = http_request(
            "GET",
            &format!("{endpoint}/v1/pull?since={since}"),
            None,
            &[],
        )
        .await?;
        if resp.status != 200 {
            return Err(SyncError::Remote(format!("pull -> HTTP {}", resp.status)));
        }
        #[derive(Deserialize)]
        struct PullBody {
            records: Vec<SyncRecord>,
        }
        let body: PullBody = serde_json::from_slice(&resp.body)
            .map_err(|e| SyncError::Malformed("pull".into(), e.to_string()))?;
        Ok(body.records)
    }

    fn name(&self) -> &str {
        "http"
    }

    fn is_configured(&self) -> bool {
        self.endpoint.is_some()
    }
}

// ---------------------------------------------------------------------------
// Local manifest (change tracking)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ManifestEntry {
    version: u64,
    updated_at: i64,
}

type Manifest = HashMap<String, ManifestEntry>;

const MANIFEST_KEY: &str = "sync.manifest";
const WATERMARK_KEY: &str = "sync.last_pull";

async fn load_manifest(storage: &Storage) -> Result<Manifest, SyncError> {
    let json = storage.get_setting(MANIFEST_KEY).await?;
    match json {
        Some(s) => Ok(serde_json::from_str(&s).unwrap_or_default()),
        None => Ok(HashMap::new()),
    }
}

async fn save_manifest(storage: &Storage, manifest: &Manifest) -> Result<(), SyncError> {
    storage
        .set_setting(MANIFEST_KEY, &serde_json::to_string(manifest)?)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Seal every local entity that changed since the manifest into records.
async fn export_all(
    vault: &Vault,
    storage: &Storage,
    device_id: &str,
    manifest: &mut Manifest,
) -> Result<Vec<SyncRecord>, SyncError> {
    let mut records = Vec::new();

    let hosts = storage.hosts().list_all().await?;
    let groups = storage.groups().list_all().await?;
    let snippets = storage.snippets().list_all().await?;

    // (collection, entity_id, updated_at, entity json value)
    let mut entities: Vec<(SyncCollection, String, i64, serde_json::Value)> = Vec::new();
    for h in &hosts {
        entities.push((
            SyncCollection::Hosts,
            h.id.clone(),
            h.updated_at,
            serde_json::to_value(h)?,
        ));
    }
    for g in &groups {
        entities.push((
            SyncCollection::HostGroups,
            g.id.clone(),
            g.created_at,
            serde_json::to_value(g)?,
        ));
    }
    for s in &snippets {
        entities.push((
            SyncCollection::Snippets,
            s.id.clone(),
            s.updated_at,
            serde_json::to_value(s)?,
        ));
    }

    let mut live_ids = std::collections::HashSet::new();
    for (collection, entity_id, updated_at, value) in entities {
        let record_id = format!("{}:{entity_id}", collection.as_str());
        live_ids.insert(record_id.clone());
        let changed = manifest
            .get(&record_id)
            .map(|e| updated_at > e.updated_at)
            .unwrap_or(true);
        if changed {
            let record = SyncRecord::seal(
                vault,
                collection,
                &entity_id,
                updated_at.max(0) as u64,
                updated_at,
                false,
                device_id,
                &value,
            )?;
            manifest.insert(
                record_id,
                ManifestEntry {
                    version: record.version,
                    updated_at,
                },
            );
            records.push(record);
        }
    }

    // Tombstones for entities deleted locally since the last sync.
    let tombstoned: Vec<String> = manifest
        .keys()
        .filter(|id| !live_ids.contains(*id))
        .cloned()
        .collect();
    for record_id in tombstoned {
        let Some((collection, entity_id)) = record_id.split_once(':') else {
            continue;
        };
        let Some(collection) = SyncCollection::from_str(collection) else {
            continue;
        };
        let now = now_unix();
        let record = SyncRecord::seal(
            vault,
            collection,
            entity_id,
            now.max(0) as u64,
            now,
            true,
            device_id,
            &serde_json::Value::Null,
        )?;
        manifest.insert(
            record_id,
            ManifestEntry {
                version: record.version,
                updated_at: now,
            },
        );
        records.push(record);
    }

    Ok(records)
}

// ---------------------------------------------------------------------------
// Apply (pull side)
// ---------------------------------------------------------------------------

struct ApplySummary {
    applied: usize,
    conflicts: usize,
    errors: Vec<String>,
}

async fn apply_records(
    vault: &Vault,
    storage: &Storage,
    records: &[SyncRecord],
    manifest: &mut Manifest,
) -> Result<ApplySummary, SyncError> {
    let mut summary = ApplySummary {
        applied: 0,
        conflicts: 0,
        errors: Vec::new(),
    };

    for record in records {
        let entry = manifest.get(&record.id).cloned().unwrap_or_default();
        if record.version <= entry.version && record.updated_at <= entry.updated_at {
            continue; // already have this (or a newer) version
        }

        let entity = match open_entity(vault, record) {
            Ok(e) => e,
            Err(e) => {
                summary.errors.push(format!("{}: {e}", record.id));
                continue;
            }
        };

        if record.deleted {
            apply_delete(storage, &record.collection, &record.entity_id).await;
            manifest.insert(
                record.id.clone(),
                ManifestEntry {
                    version: record.version,
                    updated_at: record.updated_at,
                },
            );
            summary.applied += 1;
            continue;
        }

        // Conflict detection: local changed since the manifest *and* remote is newer.
        let local_changed = local_updated_at(storage, &record.collection, &record.entity_id)
            .await
            .map(|t| t > entry.updated_at)
            .unwrap_or(false);
        let local_stub = SyncRecord {
            version: entry.version,
            updated_at: entry.updated_at,
            device_id: "local".into(),
            ..record.clone()
        };
        let (remote_wins, conflict) = resolve(&local_stub, record, local_changed);
        if let Some(c) = conflict {
            summary.conflicts += 1;
            tracing::warn!(
                "sync conflict on {} (local {} vs remote {}), last-write-wins applied",
                c.record_id,
                c.local_updated_at,
                c.remote_updated_at,
            );
        }
        if remote_wins {
            match apply_upsert(storage, entity).await {
                Ok(()) => {
                    manifest.insert(
                        record.id.clone(),
                        ManifestEntry {
                            version: record.version,
                            updated_at: record.updated_at,
                        },
                    );
                    summary.applied += 1;
                }
                Err(e) => summary.errors.push(format!("{}: {e}", record.id)),
            }
        } else {
            // Local wins: keep the manifest version so we re-export it.
            manifest.insert(
                record.id.clone(),
                ManifestEntry {
                    version: entry.version,
                    updated_at: entry.updated_at,
                },
            );
        }
    }

    Ok(summary)
}

async fn local_updated_at(
    storage: &Storage,
    collection: &str,
    entity_id: &str,
) -> Result<Option<i64>, SyncError> {
    let ts = match collection {
        "hosts" => storage
            .hosts()
            .get(entity_id)
            .await
            .ok()
            .map(|h| h.updated_at),
        "host_groups" => storage.groups().list_all().await.ok().and_then(|gs| {
            gs.into_iter()
                .find(|g| g.id == entity_id)
                .map(|g| g.created_at)
        }),
        "snippets" => storage
            .snippets()
            .get(entity_id)
            .await
            .ok()
            .map(|s| s.updated_at),
        _ => None,
    };
    Ok(ts)
}

async fn apply_upsert(storage: &Storage, entity: SyncEntity) -> Result<(), SyncError> {
    match entity {
        SyncEntity::Host(h) => {
            if storage.hosts().get(&h.id).await.is_ok() {
                storage.hosts().update(&h).await?;
            } else {
                storage.hosts().create(&h).await?;
            }
        }
        SyncEntity::HostGroup(g) => {
            if storage
                .groups()
                .list_all()
                .await?
                .iter()
                .any(|x| x.id == g.id)
            {
                if storage
                    .groups()
                    .list_all()
                    .await?
                    .iter()
                    .find(|x| x.id == g.id)
                    .map(|x| &x.name)
                    != Some(&g.name)
                {
                    storage.groups().rename(&g.id, &g.name).await?;
                }
            } else {
                storage.groups().create(&g).await?;
            }
        }
        SyncEntity::Snippet(s) => {
            if storage.snippets().get(&s.id).await.is_ok() {
                storage.snippets().update(&s).await?;
            } else {
                storage.snippets().create(&s).await?;
            }
        }
    }
    Ok(())
}

async fn apply_delete(storage: &Storage, collection: &str, entity_id: &str) {
    match collection {
        "hosts" => {
            let _ = storage.hosts().delete(entity_id).await;
        }
        "host_groups" => {
            let _ = storage.groups().delete(entity_id).await;
        }
        "snippets" => {
            let _ = storage.snippets().delete(entity_id).await;
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Sync engine
// ---------------------------------------------------------------------------

/// Orchestrates a full sync run.
pub struct SyncEngine {
    remote: Arc<dyn Remote>,
    storage: Storage,
    device_id: String,
}

impl SyncEngine {
    /// Create an engine. `device_id` should be stable per installation.
    pub fn new(remote: Arc<dyn Remote>, storage: Storage, device_id: String) -> Self {
        Self {
            remote,
            storage,
            device_id,
        }
    }

    /// The configured remote's name.
    pub fn remote_name(&self) -> &str {
        self.remote.name()
    }

    /// Run one full sync: export → push → pull → apply.
    pub async fn sync(&self, vault: &Vault) -> Result<SyncSummary, SyncError> {
        if !self.remote.is_configured() {
            return Err(SyncError::NotConfigured);
        }
        let mut manifest = load_manifest(&self.storage).await?;

        // 1-2. Export local changes and push them.
        let local_records =
            export_all(vault, &self.storage, &self.device_id, &mut manifest).await?;
        let pushed = local_records.len();
        self.remote.push(&local_records).await?;
        save_manifest(&self.storage, &manifest).await?;

        // 3-4. Pull remote changes and apply.
        let since: i64 = self
            .storage
            .get_setting(WATERMARK_KEY)
            .await?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let remote_records = self.remote.pull(since).await?;
        let pulled = remote_records.len();
        let applied = apply_records(vault, &self.storage, &remote_records, &mut manifest).await?;
        save_manifest(&self.storage, &manifest).await?;
        self.storage
            .set_setting(WATERMARK_KEY, &now_unix().to_string())
            .await?;

        Ok(SyncSummary {
            pushed,
            pulled,
            applied: applied.applied,
            conflicts: applied.conflicts,
            errors: applied.errors,
        })
    }

    /// Last successful sync time (unix seconds), if any.
    pub async fn last_sync_at(&self) -> Result<Option<i64>, SyncError> {
        Ok(self
            .storage
            .get_setting(WATERMARK_KEY)
            .await?
            .and_then(|s| s.parse().ok()))
    }
}

/// Generate a stable device id, persisting it in settings.
pub async fn device_id(storage: &Storage) -> Result<String, SyncError> {
    const KEY: &str = "sync.device_id";
    if let Some(id) = storage.get_setting(KEY).await? {
        return Ok(id);
    }
    let id = uuid::Uuid::new_v4().to_string();
    storage.set_setting(KEY, &id).await?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opsdeck_storage::models::Host;
    use opsdeck_vault::SecretString;

    async fn test_storage() -> Storage {
        Storage::open_in_memory().await.unwrap()
    }

    fn test_vault() -> (Vault, opsdeck_vault::VaultVerifier) {
        Vault::create(&SecretString::new("sync-test-password")).unwrap()
    }

    #[tokio::test]
    async fn file_remote_roundtrip() {
        let dir = std::env::temp_dir().join(format!("opsdeck-sync-test-{}", uuid::Uuid::new_v4()));
        let remote = FileRemote::new(&dir);
        let (vault, _) = test_vault();
        let record = SyncRecord::seal(
            &vault,
            SyncCollection::Hosts,
            "h1",
            1,
            100,
            false,
            "dev-a",
            &serde_json::json!({"name": "web-1"}),
        )
        .unwrap();
        remote.push(std::slice::from_ref(&record)).await.unwrap();
        let pulled = remote.pull(0).await.unwrap();
        assert_eq!(pulled.len(), 1);
        assert_eq!(pulled[0].id, "hosts:h1");
        assert!(remote.pull(200).await.unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn full_sync_between_two_devices() {
        let dir = std::env::temp_dir().join(format!("opsdeck-sync-test-{}", uuid::Uuid::new_v4()));

        // Device A: create a host, sync (push).
        let storage_a = test_storage().await;
        let (vault_a, _) = test_vault();
        let mut host = Host::new("web-1", "10.0.0.1", "root");
        storage_a.hosts().create(&host).await.unwrap();
        let engine_a = SyncEngine::new(
            Arc::new(FileRemote::new(&dir)),
            storage_a.clone(),
            "device-a".into(),
        );
        let summary = engine_a.sync(&vault_a).await.unwrap();
        assert_eq!(summary.pushed, 1);

        // Device B (fresh): sync (pull) → host appears.
        // (Same vault as A: simulates the same master password on both devices.)
        let storage_b = test_storage().await;
        let engine_b = SyncEngine::new(
            Arc::new(FileRemote::new(&dir)),
            storage_b.clone(),
            "device-b".into(),
        );
        let summary_b = engine_b.sync(&vault_a).await.unwrap();
        assert_eq!(summary_b.pulled, 1);
        assert_eq!(summary_b.applied, 1);
        let hosts_b = storage_b.hosts().list_all().await.unwrap();
        assert_eq!(hosts_b.len(), 1);
        assert_eq!(hosts_b[0].name, "web-1");

        // B renames; A syncs again → last-write-wins applies on A.
        host.name = "web-1-renamed".to_string();
        host.updated_at = now_unix() + 100;
        storage_b.hosts().update(&host).await.unwrap();
        let summary_b2 = engine_b.sync(&vault_a).await.unwrap();
        assert_eq!(summary_b2.pushed, 1);
        let summary_a2 = engine_a.sync(&vault_a).await.unwrap();
        assert!(summary_a2.applied >= 1 || summary_a2.pulled >= 1);
        let hosts_a = storage_a.hosts().list_all().await.unwrap();
        assert_eq!(hosts_a[0].name, "web-1-renamed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn unconfigured_remote_errors() {
        let storage = test_storage().await;
        let (vault, _) = test_vault();
        let engine = SyncEngine::new(
            Arc::new(HttpRemote::unconfigured("dev")),
            storage,
            "dev".into(),
        );
        assert!(matches!(
            engine.sync(&vault).await,
            Err(SyncError::NotConfigured)
        ));
        assert!(!engine.remote.is_configured());
    }
}
