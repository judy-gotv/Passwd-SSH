//! Conflict detection and resolution (docs §7.3).
//!
//! Default policy is last-write-wins on `updated_at`; divergent versions
//! are additionally reported as [`Conflict`]s so the UI can offer a manual
//! merge choice.
//!
//! Author: Passwd Word

use serde::{Deserialize, Serialize};

use crate::model::SyncRecord;

/// How a single conflict was (or should be) resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    /// Default: the newer `updated_at` wins.
    LastWriteWins,
    /// Keep the local version.
    KeepLocal,
    /// Take the remote version.
    TakeRemote,
}

/// A divergent change detected during sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    pub record_id: String,
    pub collection: String,
    pub local_updated_at: i64,
    pub remote_updated_at: i64,
    pub local_device: String,
    pub remote_device: String,
    pub resolution: Resolution,
}

/// Decide the winner between a local and a remote version of the same record.
///
/// Returns `(winner_is_remote, conflict)` — `conflict` is `Some` when both
/// sides changed the record since the last common version (true divergence),
/// so the UI can surface it even though last-write-wins applies automatically.
pub fn resolve(
    local: &SyncRecord,
    remote: &SyncRecord,
    local_changed_since_base: bool,
) -> (bool, Option<Conflict>) {
    let winner_is_remote = remote.updated_at >= local.updated_at;
    let conflict = if local_changed_since_base && remote.version > local.version {
        Some(Conflict {
            record_id: local.id.clone(),
            collection: local.collection.clone(),
            local_updated_at: local.updated_at,
            remote_updated_at: remote.updated_at,
            local_device: local.device_id.clone(),
            remote_device: remote.device_id.clone(),
            resolution: Resolution::LastWriteWins,
        })
    } else {
        None
    };
    (winner_is_remote, conflict)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SyncRecord;

    fn rec(updated_at: i64, version: u64, device: &str) -> SyncRecord {
        SyncRecord {
            id: "hosts:1".into(),
            collection: "hosts".into(),
            entity_id: "1".into(),
            version,
            updated_at,
            deleted: false,
            device_id: device.into(),
            nonce: vec![0; 12],
            ciphertext: vec![],
        }
    }

    #[test]
    fn newer_remote_wins() {
        let local = rec(100, 1, "a");
        let remote = rec(200, 2, "b");
        let (winner_is_remote, conflict) = resolve(&local, &remote, true);
        assert!(winner_is_remote);
        assert!(conflict.is_some());
    }

    #[test]
    fn no_conflict_when_local_unchanged() {
        let local = rec(100, 1, "a");
        let remote = rec(200, 2, "b");
        let (winner_is_remote, conflict) = resolve(&local, &remote, false);
        assert!(winner_is_remote);
        assert!(conflict.is_none());
    }

    #[test]
    fn ties_prefer_remote() {
        let local = rec(100, 1, "a");
        let remote = rec(100, 2, "b");
        let (winner_is_remote, _) = resolve(&local, &remote, true);
        assert!(winner_is_remote);
    }
}
