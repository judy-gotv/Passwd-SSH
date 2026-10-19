//! Data-access objects for every table.
//! Author: Passwd Word

use sqlx::{Row, SqlitePool};

use crate::models::*;
use crate::{Storage, StorageError};

// ---------------------------------------------------------------------------
// Row mapping helpers
// ---------------------------------------------------------------------------

fn map_host(row: &sqlx::sqlite::SqliteRow) -> Result<Host, sqlx::Error> {
    Ok(Host {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        group_id: row.try_get("group_id")?,
        host: row.try_get("host")?,
        port: row.try_get("port")?,
        username: row.try_get("username")?,
        auth_type: row.try_get("auth_type")?,
        credential_id: row.try_get("credential_id")?,
        passphrase_credential_id: row.try_get("passphrase_credential_id")?,
        proxy_jump_id: row.try_get("proxy_jump_id")?,
        startup_command: row.try_get("startup_command")?,
        tags: row.try_get::<Option<String>, _>("tags")?.unwrap_or_else(|| "[]".to_string()),
        favorite: row.try_get::<i64, _>("favorite")? != 0,
        last_connected_at: row.try_get("last_connected_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn map_credential(row: &sqlx::sqlite::SqliteRow) -> Result<Credential, sqlx::Error> {
    Ok(Credential {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        kind: row.try_get("kind")?,
        ciphertext: row.try_get("ciphertext")?,
        nonce: row.try_get("nonce")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn map_group(row: &sqlx::sqlite::SqliteRow) -> Result<HostGroup, sqlx::Error> {
    Ok(HostGroup {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        parent_id: row.try_get("parent_id")?,
        sort_order: row.try_get("sort_order")?,
        created_at: row.try_get("created_at")?,
    })
}

fn map_snippet(row: &sqlx::sqlite::SqliteRow) -> Result<Snippet, sqlx::Error> {
    Ok(Snippet {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        command: row.try_get("command")?,
        description: row.try_get("description")?,
        host_id: row.try_get("host_id")?,
        tags: row.try_get::<Option<String>, _>("tags")?.unwrap_or_else(|| "[]".to_string()),
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn map_transfer(row: &sqlx::sqlite::SqliteRow) -> Result<TransferTask, sqlx::Error> {
    Ok(TransferTask {
        id: row.try_get("id")?,
        host_id: row.try_get("host_id")?,
        direction: row.try_get("direction")?,
        local_path: row.try_get("local_path")?,
        remote_path: row.try_get("remote_path")?,
        status: row.try_get("status")?,
        bytes_total: row.try_get("bytes_total")?,
        bytes_done: row.try_get("bytes_done")?,
        error: row.try_get("error")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn map_metric(row: &sqlx::sqlite::SqliteRow) -> Result<MetricSnapshot, sqlx::Error> {
    Ok(MetricSnapshot {
        id: row.try_get("id")?,
        host_id: row.try_get("host_id")?,
        collected_at: row.try_get("collected_at")?,
        cpu_percent: row.try_get("cpu_percent")?,
        memory_used_bytes: row.try_get("memory_used_bytes")?,
        memory_total_bytes: row.try_get("memory_total_bytes")?,
        disk_used_bytes: row.try_get("disk_used_bytes")?,
        disk_total_bytes: row.try_get("disk_total_bytes")?,
        load_1: row.try_get("load_1")?,
        load_5: row.try_get("load_5")?,
        load_15: row.try_get("load_15")?,
        network_rx_bytes: row.try_get("network_rx_bytes")?,
        network_tx_bytes: row.try_get("network_tx_bytes")?,
    })
}

// ---------------------------------------------------------------------------
// Hosts
// ---------------------------------------------------------------------------

/// Data access for the `hosts` table.
pub struct HostDao<'a> {
    pool: &'a SqlitePool,
}

impl Storage {
    /// Access hosts.
    pub fn hosts(&self) -> HostDao<'_> {
        HostDao { pool: self.pool() }
    }
}

impl HostDao<'_> {
    /// Insert a new host.
    pub async fn create(&self, host: &Host) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO hosts (id, name, group_id, host, port, username, auth_type,
             credential_id, passphrase_credential_id, proxy_jump_id, startup_command,
             tags, favorite, last_connected_at, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&host.id)
        .bind(&host.name)
        .bind(&host.group_id)
        .bind(&host.host)
        .bind(host.port)
        .bind(&host.username)
        .bind(&host.auth_type)
        .bind(&host.credential_id)
        .bind(&host.passphrase_credential_id)
        .bind(&host.proxy_jump_id)
        .bind(&host.startup_command)
        .bind(&host.tags)
        .bind(if host.favorite { 1i64 } else { 0i64 })
        .bind(host.last_connected_at)
        .bind(host.created_at)
        .bind(host.updated_at)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Update an existing host (matched by id).
    pub async fn update(&self, host: &Host) -> Result<(), StorageError> {
        let now = now_unix();
        let rows = sqlx::query(
            "UPDATE hosts SET name = ?, group_id = ?, host = ?, port = ?, username = ?,
             auth_type = ?, credential_id = ?, passphrase_credential_id = ?,
             proxy_jump_id = ?, startup_command = ?, tags = ?, favorite = ?,
             updated_at = ? WHERE id = ?",
        )
        .bind(&host.name)
        .bind(&host.group_id)
        .bind(&host.host)
        .bind(host.port)
        .bind(&host.username)
        .bind(&host.auth_type)
        .bind(&host.credential_id)
        .bind(&host.passphrase_credential_id)
        .bind(&host.proxy_jump_id)
        .bind(&host.startup_command)
        .bind(&host.tags)
        .bind(if host.favorite { 1i64 } else { 0i64 })
        .bind(now)
        .bind(&host.id)
        .execute(self.pool)
        .await?
        .rows_affected();
        if rows == 0 {
            return Err(StorageError::NotFound(format!("host {}", host.id)));
        }
        Ok(())
    }

    /// Delete a host.
    pub async fn delete(&self, id: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM hosts WHERE id = ?").bind(id).execute(self.pool).await?;
        Ok(())
    }

    /// Fetch one host by id.
    pub async fn get(&self, id: &str) -> Result<Host, StorageError> {
        let row = sqlx::query("SELECT * FROM hosts WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("host {id}")))?;
        Ok(map_host(&row)?)
    }

    /// List all hosts, favorites first, then by name.
    pub async fn list_all(&self) -> Result<Vec<Host>, StorageError> {
        let rows = sqlx::query("SELECT * FROM hosts ORDER BY favorite DESC, name ASC")
            .fetch_all(self.pool)
            .await?;
        rows.iter().map(map_host).collect::<Result<_, _>>().map_err(Into::into)
    }

    /// List hosts in a group.
    pub async fn list_by_group(&self, group_id: &str) -> Result<Vec<Host>, StorageError> {
        let rows = sqlx::query("SELECT * FROM hosts WHERE group_id = ? ORDER BY name ASC")
            .bind(group_id)
            .fetch_all(self.pool)
            .await?;
        rows.iter().map(map_host).collect::<Result<_, _>>().map_err(Into::into)
    }

    /// Full-text-ish search over name, host, username and tags.
    pub async fn search(&self, query: &str) -> Result<Vec<Host>, StorageError> {
        let like = format!("%{query}%");
        let rows = sqlx::query(
            "SELECT * FROM hosts
             WHERE name LIKE ? OR host LIKE ? OR username LIKE ? OR tags LIKE ?
             ORDER BY favorite DESC, name ASC",
        )
        .bind(&like)
        .bind(&like)
        .bind(&like)
        .bind(&like)
        .fetch_all(self.pool)
        .await?;
        rows.iter().map(map_host).collect::<Result<_, _>>().map_err(Into::into)
    }

    /// Recently connected hosts, most recent first.
    pub async fn recent(&self, limit: i64) -> Result<Vec<Host>, StorageError> {
        let rows = sqlx::query(
            "SELECT * FROM hosts WHERE last_connected_at IS NOT NULL
             ORDER BY last_connected_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        rows.iter().map(map_host).collect::<Result<_, _>>().map_err(Into::into)
    }

    /// Mark a host as just connected.
    pub async fn touch_connected(&self, id: &str) -> Result<(), StorageError> {
        let now = now_unix();
        sqlx::query("UPDATE hosts SET last_connected_at = ?, updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(now)
            .bind(id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// Toggle the favorite flag.
    pub async fn set_favorite(&self, id: &str, favorite: bool) -> Result<(), StorageError> {
        sqlx::query("UPDATE hosts SET favorite = ?, updated_at = ? WHERE id = ?")
            .bind(if favorite { 1i64 } else { 0i64 })
            .bind(now_unix())
            .bind(id)
            .execute(self.pool)
            .await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Credentials
// ---------------------------------------------------------------------------

/// Data access for the `credentials` table.
pub struct CredentialDao<'a> {
    pool: &'a SqlitePool,
}

impl Storage {
    /// Access credentials.
    pub fn credentials(&self) -> CredentialDao<'_> {
        CredentialDao { pool: self.pool() }
    }
}

impl CredentialDao<'_> {
    /// Insert or replace a credential.
    pub async fn upsert(&self, cred: &Credential) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO credentials (id, name, kind, ciphertext, nonce, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, kind = excluded.kind,
             ciphertext = excluded.ciphertext, nonce = excluded.nonce,
             updated_at = excluded.updated_at",
        )
        .bind(&cred.id)
        .bind(&cred.name)
        .bind(&cred.kind)
        .bind(&cred.ciphertext)
        .bind(&cred.nonce)
        .bind(cred.created_at)
        .bind(cred.updated_at)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Fetch one credential by id (includes ciphertext).
    pub async fn get(&self, id: &str) -> Result<Credential, StorageError> {
        let row = sqlx::query("SELECT * FROM credentials WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("credential {id}")))?;
        Ok(map_credential(&row)?)
    }

    /// List credential metadata (no secret bytes).
    pub async fn list_meta(&self) -> Result<Vec<CredentialMeta>, StorageError> {
        let rows = sqlx::query(
            "SELECT id, name, kind, created_at, updated_at FROM credentials ORDER BY name ASC",
        )
        .fetch_all(self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                Ok(CredentialMeta {
                    id: r.try_get("id")?,
                    name: r.try_get("name")?,
                    kind: r.try_get("kind")?,
                    created_at: r.try_get("created_at")?,
                    updated_at: r.try_get("updated_at")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(Into::into)
    }

    /// Delete a credential.
    pub async fn delete(&self, id: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM credentials WHERE id = ?").bind(id).execute(self.pool).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Host groups
// ---------------------------------------------------------------------------

/// Data access for the `host_groups` table.
pub struct HostGroupDao<'a> {
    pool: &'a SqlitePool,
}

impl Storage {
    /// Access host groups.
    pub fn groups(&self) -> HostGroupDao<'_> {
        HostGroupDao { pool: self.pool() }
    }
}

impl HostGroupDao<'_> {
    /// Create a group.
    pub async fn create(&self, group: &HostGroup) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO host_groups (id, name, parent_id, sort_order, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&group.id)
        .bind(&group.name)
        .bind(&group.parent_id)
        .bind(group.sort_order)
        .bind(group.created_at)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Rename a group.
    pub async fn rename(&self, id: &str, name: &str) -> Result<(), StorageError> {
        sqlx::query("UPDATE host_groups SET name = ? WHERE id = ?")
            .bind(name)
            .bind(id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// Delete a group (hosts in it become ungrouped).
    pub async fn delete(&self, id: &str) -> Result<(), StorageError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE hosts SET group_id = NULL WHERE group_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM host_groups WHERE id = ?").bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// List all groups in sort order.
    pub async fn list_all(&self) -> Result<Vec<HostGroup>, StorageError> {
        let rows = sqlx::query("SELECT * FROM host_groups ORDER BY sort_order ASC, name ASC")
            .fetch_all(self.pool)
            .await?;
        rows.iter().map(map_group).collect::<Result<_, _>>().map_err(Into::into)
    }
}

// ---------------------------------------------------------------------------
// Snippets
// ---------------------------------------------------------------------------

/// Data access for the `snippets` table.
pub struct SnippetDao<'a> {
    pool: &'a SqlitePool,
}

impl Storage {
    /// Access snippets.
    pub fn snippets(&self) -> SnippetDao<'_> {
        SnippetDao { pool: self.pool() }
    }
}

impl SnippetDao<'_> {
    /// Insert a snippet.
    pub async fn create(&self, snippet: &Snippet) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO snippets (id, title, command, description, host_id, tags, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&snippet.id)
        .bind(&snippet.title)
        .bind(&snippet.command)
        .bind(&snippet.description)
        .bind(&snippet.host_id)
        .bind(&snippet.tags)
        .bind(snippet.created_at)
        .bind(snippet.updated_at)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Update a snippet.
    pub async fn update(&self, snippet: &Snippet) -> Result<(), StorageError> {
        sqlx::query(
            "UPDATE snippets SET title = ?, command = ?, description = ?, host_id = ?,
             tags = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&snippet.title)
        .bind(&snippet.command)
        .bind(&snippet.description)
        .bind(&snippet.host_id)
        .bind(&snippet.tags)
        .bind(now_unix())
        .bind(&snippet.id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Delete a snippet.
    pub async fn delete(&self, id: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM snippets WHERE id = ?").bind(id).execute(self.pool).await?;
        Ok(())
    }

    /// Fetch one snippet.
    pub async fn get(&self, id: &str) -> Result<Snippet, StorageError> {
        let row = sqlx::query("SELECT * FROM snippets WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("snippet {id}")))?;
        Ok(map_snippet(&row)?)
    }

    /// List all snippets by title.
    pub async fn list_all(&self) -> Result<Vec<Snippet>, StorageError> {
        let rows = sqlx::query("SELECT * FROM snippets ORDER BY title ASC").fetch_all(self.pool).await?;
        rows.iter().map(map_snippet).collect::<Result<_, _>>().map_err(Into::into)
    }

    /// Search snippets by title, command or tags.
    pub async fn search(&self, query: &str) -> Result<Vec<Snippet>, StorageError> {
        let like = format!("%{query}%");
        let rows = sqlx::query(
            "SELECT * FROM snippets WHERE title LIKE ? OR command LIKE ? OR tags LIKE ?
             ORDER BY title ASC",
        )
        .bind(&like)
        .bind(&like)
        .bind(&like)
        .fetch_all(self.pool)
        .await?;
        rows.iter().map(map_snippet).collect::<Result<_, _>>().map_err(Into::into)
    }
}

// ---------------------------------------------------------------------------
// Transfer tasks
// ---------------------------------------------------------------------------

/// Data access for the `transfer_tasks` table.
pub struct TransferDao<'a> {
    pool: &'a SqlitePool,
}

impl Storage {
    /// Access transfer tasks.
    pub fn transfers(&self) -> TransferDao<'_> {
        TransferDao { pool: self.pool() }
    }
}

impl TransferDao<'_> {
    /// Insert a new task.
    pub async fn create(&self, task: &TransferTask) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO transfer_tasks (id, host_id, direction, local_path, remote_path,
             status, bytes_total, bytes_done, error, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&task.id)
        .bind(&task.host_id)
        .bind(&task.direction)
        .bind(&task.local_path)
        .bind(&task.remote_path)
        .bind(&task.status)
        .bind(task.bytes_total)
        .bind(task.bytes_done)
        .bind(&task.error)
        .bind(task.created_at)
        .bind(task.updated_at)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Update task status / progress.
    pub async fn update_progress(
        &self,
        id: &str,
        status: TransferStatus,
        bytes_done: i64,
        bytes_total: Option<i64>,
        error: Option<&str>,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "UPDATE transfer_tasks SET status = ?, bytes_done = ?, bytes_total = ?,
             error = ?, updated_at = ? WHERE id = ?",
        )
        .bind(status.as_str())
        .bind(bytes_done)
        .bind(bytes_total)
        .bind(error)
        .bind(now_unix())
        .bind(id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Fetch one task.
    pub async fn get(&self, id: &str) -> Result<TransferTask, StorageError> {
        let row = sqlx::query("SELECT * FROM transfer_tasks WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("transfer {id}")))?;
        Ok(map_transfer(&row)?)
    }

    /// List tasks for a host, newest first.
    pub async fn list_for_host(&self, host_id: &str, limit: i64) -> Result<Vec<TransferTask>, StorageError> {
        let rows = sqlx::query(
            "SELECT * FROM transfer_tasks WHERE host_id = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(host_id)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        rows.iter().map(map_transfer).collect::<Result<_, _>>().map_err(Into::into)
    }

    /// List all unfinished tasks (for resume after restart).
    pub async fn list_unfinished(&self) -> Result<Vec<TransferTask>, StorageError> {
        let rows = sqlx::query(
            "SELECT * FROM transfer_tasks
             WHERE status IN ('pending', 'running', 'paused')
             ORDER BY created_at ASC",
        )
        .fetch_all(self.pool)
        .await?;
        rows.iter().map(map_transfer).collect::<Result<_, _>>().map_err(Into::into)
    }

    /// Delete finished tasks older than `cutoff` (unix seconds).
    pub async fn prune_finished_before(&self, cutoff: i64) -> Result<u64, StorageError> {
        let r = sqlx::query(
            "DELETE FROM transfer_tasks
             WHERE status IN ('completed', 'failed', 'canceled') AND updated_at < ?",
        )
        .bind(cutoff)
        .execute(self.pool)
        .await?;
        Ok(r.rows_affected())
    }
}

// ---------------------------------------------------------------------------
// Metric snapshots
// ---------------------------------------------------------------------------

/// Data access for the `metric_snapshots` table.
pub struct MetricDao<'a> {
    pool: &'a SqlitePool,
}

impl Storage {
    /// Access metric snapshots.
    pub fn metrics(&self) -> MetricDao<'_> {
        MetricDao { pool: self.pool() }
    }
}

impl MetricDao<'_> {
    /// Insert a snapshot.
    pub async fn insert(&self, snap: &MetricSnapshot) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO metric_snapshots (host_id, collected_at, cpu_percent,
             memory_used_bytes, memory_total_bytes, disk_used_bytes, disk_total_bytes,
             load_1, load_5, load_15, network_rx_bytes, network_tx_bytes)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&snap.host_id)
        .bind(snap.collected_at)
        .bind(snap.cpu_percent)
        .bind(snap.memory_used_bytes)
        .bind(snap.memory_total_bytes)
        .bind(snap.disk_used_bytes)
        .bind(snap.disk_total_bytes)
        .bind(snap.load_1)
        .bind(snap.load_5)
        .bind(snap.load_15)
        .bind(snap.network_rx_bytes)
        .bind(snap.network_tx_bytes)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Recent snapshots for a host, oldest first (for charts).
    pub async fn recent_for_host(
        &self,
        host_id: &str,
        limit: i64,
    ) -> Result<Vec<MetricSnapshot>, StorageError> {
        let rows = sqlx::query(
            "SELECT * FROM (SELECT * FROM metric_snapshots WHERE host_id = ?
             ORDER BY collected_at DESC LIMIT ?) ORDER BY collected_at ASC",
        )
        .bind(host_id)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        rows.iter().map(map_metric).collect::<Result<_, _>>().map_err(Into::into)
    }

    /// Latest snapshot for a host.
    pub async fn latest_for_host(&self, host_id: &str) -> Result<Option<MetricSnapshot>, StorageError> {
        let row = sqlx::query(
            "SELECT * FROM metric_snapshots WHERE host_id = ? ORDER BY collected_at DESC LIMIT 1",
        )
        .bind(host_id)
        .fetch_optional(self.pool)
        .await?;
        row.map(|r| map_metric(&r)).transpose().map_err(Into::into)
    }

    /// Delete snapshots older than `cutoff` (unix seconds).
    pub async fn prune_before(&self, cutoff: i64) -> Result<u64, StorageError> {
        let r = sqlx::query("DELETE FROM metric_snapshots WHERE collected_at < ?")
            .bind(cutoff)
            .execute(self.pool)
            .await?;
        Ok(r.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem() -> Storage {
        Storage::open_in_memory().await.unwrap()
    }

    #[tokio::test]
    async fn host_crud() {
        let db = mem().await;
        let mut host = Host::new("web-1", "192.168.1.10", "root");
        host.tags = r#"["prod","web"]"#.to_string();
        db.hosts().create(&host).await.unwrap();

        let got = db.hosts().get(&host.id).await.unwrap();
        assert_eq!(got.name, "web-1");
        assert_eq!(got.tag_list(), vec!["prod", "web"]);

        let found = db.hosts().search("192.168").await.unwrap();
        assert_eq!(found.len(), 1);

        db.hosts().set_favorite(&host.id, true).await.unwrap();
        assert!(db.hosts().get(&host.id).await.unwrap().favorite);

        db.hosts().touch_connected(&host.id).await.unwrap();
        let recent = db.hosts().recent(5).await.unwrap();
        assert_eq!(recent.len(), 1);

        let mut host2 = host.clone();
        host2.name = "web-1-renamed".to_string();
        db.hosts().update(&host2).await.unwrap();
        assert_eq!(db.hosts().get(&host.id).await.unwrap().name, "web-1-renamed");

        db.hosts().delete(&host.id).await.unwrap();
        assert!(matches!(
            db.hosts().get(&host.id).await,
            Err(StorageError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn credential_and_group_flow() {
        let db = mem().await;
        let now = now_unix();
        db.credentials()
            .upsert(&Credential {
                id: "cred-1".into(),
                name: "root password".into(),
                kind: "password".into(),
                ciphertext: vec![1, 2, 3],
                nonce: vec![4, 5, 6],
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();
        let meta = db.credentials().list_meta().await.unwrap();
        assert_eq!(meta.len(), 1);
        assert_eq!(meta[0].name, "root password");

        let group = HostGroup {
            id: new_id(),
            name: "Production".into(),
            parent_id: None,
            sort_order: 0,
            created_at: now,
        };
        db.groups().create(&group).await.unwrap();
        assert_eq!(db.groups().list_all().await.unwrap().len(), 1);
        db.groups().rename(&group.id, "Prod").await.unwrap();
        db.groups().delete(&group.id).await.unwrap();
        assert!(db.groups().list_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn snippet_and_transfer_flow() {
        let db = mem().await;
        let now = now_unix();
        let snip = Snippet {
            id: new_id(),
            title: "disk usage".into(),
            command: "df -h".into(),
            description: Some("show disks".into()),
            host_id: None,
            tags: r#"["sys"]"#.into(),
            created_at: now,
            updated_at: now,
        };
        db.snippets().create(&snip).await.unwrap();
        assert_eq!(db.snippets().search("df").await.unwrap().len(), 1);

        let task = TransferTask {
            id: new_id(),
            host_id: "host-1".into(),
            direction: "upload".into(),
            local_path: "/tmp/a".into(),
            remote_path: "/root/a".into(),
            status: "pending".into(),
            bytes_total: Some(100),
            bytes_done: 0,
            error: None,
            created_at: now,
            updated_at: now,
        };
        db.transfers().create(&task).await.unwrap();
        db.transfers()
            .update_progress(&task.id, TransferStatus::Running, 50, Some(100), None)
            .await
            .unwrap();
        let got = db.transfers().get(&task.id).await.unwrap();
        assert_eq!(got.progress(), Some(0.5));
        assert_eq!(db.transfers().list_unfinished().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn metrics_flow() {
        let db = mem().await;
        let now = now_unix();
        for i in 0..3 {
            db.metrics()
                .insert(&MetricSnapshot {
                    id: None,
                    host_id: "h1".into(),
                    collected_at: now + i,
                    cpu_percent: Some(10.0 + i as f64),
                    memory_used_bytes: Some(100),
                    memory_total_bytes: Some(1000),
                    disk_used_bytes: None,
                    disk_total_bytes: None,
                    load_1: Some(0.5),
                    load_5: None,
                    load_15: None,
                    network_rx_bytes: None,
                    network_tx_bytes: None,
                })
                .await
                .unwrap();
        }
        let recent = db.metrics().recent_for_host("h1", 10).await.unwrap();
        assert_eq!(recent.len(), 3);
        assert!(recent[0].collected_at < recent[2].collected_at);
        assert_eq!(recent[0].memory_ratio(), Some(0.1));
        let pruned = db.metrics().prune_before(now + 2).await.unwrap();
        assert_eq!(pruned, 2);
    }
}
