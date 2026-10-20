//! Sync data model: encrypted, versioned records.
//!
//! Security properties (docs §7.3):
//! - The remote only ever receives [`SyncRecord`]s: nonce + ciphertext.
//! - The server cannot decrypt user data; it never sees plaintext.
//! - Credentials (passwords / private keys) and terminal logs are never
//!   synced — only host configs, groups and snippets.
//!
//! Author: Passwd Word

use opsdeck_storage::models::{Host, HostGroup, Snippet};
use opsdeck_vault::{EncryptedBlob, Vault};
use serde::{Deserialize, Serialize};

use crate::error::SyncError;

/// Collections that participate in sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncCollection {
    Hosts,
    HostGroups,
    Snippets,
}

impl SyncCollection {
    pub fn as_str(self) -> &'static str {
        match self {
            SyncCollection::Hosts => "hosts",
            SyncCollection::HostGroups => "host_groups",
            SyncCollection::Snippets => "snippets",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "hosts" => Some(SyncCollection::Hosts),
            "host_groups" => Some(SyncCollection::HostGroups),
            "snippets" => Some(SyncCollection::Snippets),
            _ => None,
        }
    }
}

/// One encrypted, versioned sync record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRecord {
    /// `"<collection>:<entity_id>"`.
    pub id: String,
    pub collection: String,
    pub entity_id: String,
    /// Monotonic version assigned by the originating device.
    pub version: u64,
    pub updated_at: i64,
    /// Tombstone for deletions.
    pub deleted: bool,
    /// Device that produced this version (for debugging conflicts).
    pub device_id: String,
    /// ChaCha20-Poly1305 nonce.
    pub nonce: Vec<u8>,
    /// Encrypted JSON of the entity.
    pub ciphertext: Vec<u8>,
}

impl SyncRecord {
    /// Encrypt an entity into a record.
    #[allow(clippy::too_many_arguments)]
    pub fn seal<T: Serialize>(
        vault: &Vault,
        collection: SyncCollection,
        entity_id: &str,
        version: u64,
        updated_at: i64,
        deleted: bool,
        device_id: &str,
        entity: &T,
    ) -> Result<Self, SyncError> {
        let plaintext = serde_json::to_vec(entity)?;
        let EncryptedBlob { nonce, ciphertext } = vault.encrypt(&plaintext)?;
        Ok(Self {
            id: format!("{}:{entity_id}", collection.as_str()),
            collection: collection.as_str().to_string(),
            entity_id: entity_id.to_string(),
            version,
            updated_at,
            deleted,
            device_id: device_id.to_string(),
            nonce: nonce.to_vec(),
            ciphertext,
        })
    }

    /// Decrypt the record payload.
    pub fn open<T: for<'de> Deserialize<'de>>(&self, vault: &Vault) -> Result<T, SyncError> {
        let blob = EncryptedBlob {
            nonce: self.nonce_vec()?,
            ciphertext: self.ciphertext.clone(),
        };
        let plaintext = vault
            .decrypt(&blob)
            .map_err(|_| SyncError::Decrypt(self.id.clone()))?;
        serde_json::from_slice(&plaintext)
            .map_err(|e| SyncError::Malformed(self.id.clone(), e.to_string()))
    }

    fn nonce_vec(&self) -> Result<[u8; 12], SyncError> {
        self.nonce
            .as_slice()
            .try_into()
            .map_err(|_| SyncError::Malformed(self.id.clone(), "bad nonce length".into()))
    }
}

/// Decrypted entity variants.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum SyncEntity {
    Host(Host),
    HostGroup(HostGroup),
    Snippet(Snippet),
}

/// Decrypt a record into its typed entity.
pub fn open_entity(vault: &Vault, record: &SyncRecord) -> Result<SyncEntity, SyncError> {
    let collection = SyncCollection::parse(&record.collection)
        .ok_or_else(|| SyncError::Malformed(record.id.clone(), "unknown collection".into()))?;
    match collection {
        SyncCollection::Hosts => Ok(SyncEntity::Host(record.open(vault)?)),
        SyncCollection::HostGroups => Ok(SyncEntity::HostGroup(record.open(vault)?)),
        SyncCollection::Snippets => Ok(SyncEntity::Snippet(record.open(vault)?)),
    }
}

/// Summary of a sync run, for the UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncSummary {
    pub pushed: usize,
    pub pulled: usize,
    pub applied: usize,
    pub conflicts: usize,
    pub errors: Vec<String>,
}
