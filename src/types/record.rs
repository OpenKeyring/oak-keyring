use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::credential::{CredentialType, EncryptedPayload};
use crate::types::sensitive::SecureStr;
use crate::types::sync::SyncStatus;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRecord {
    pub id: Uuid,
    pub credential_type: CredentialType,
    pub encrypted_data: Vec<u8>,
    pub nonce: [u8; 24],
    pub dek_version: u32,
    pub aad: Vec<u8>,
    pub is_favorite: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: String,
    pub version: u64,
    pub deleted: bool,
    pub deleted_at: Option<DateTime<Utc>>,
    pub tags: Vec<String>,
}

#[derive(Debug)]
pub enum DecryptedRecord {
    Login {
        id: Uuid,
        is_favorite: bool,
        expires_at: Option<DateTime<Utc>>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        version: u64,
        deleted: bool,
        deleted_at: Option<DateTime<Utc>>,
        tags: Vec<String>,
        name: String,
        username: String,
        password: SecureStr,
        url: Option<String>,
        notes: Option<String>,
    },
    Api {
        id: Uuid,
        is_favorite: bool,
        expires_at: Option<DateTime<Utc>>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        version: u64,
        deleted: bool,
        deleted_at: Option<DateTime<Utc>>,
        tags: Vec<String>,
        name: String,
        app_id: String,
        secret_key: SecureStr,
        url: Option<String>,
        notes: Option<String>,
    },
    Ssh {
        id: Uuid,
        is_favorite: bool,
        expires_at: Option<DateTime<Utc>>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        version: u64,
        deleted: bool,
        deleted_at: Option<DateTime<Utc>>,
        tags: Vec<String>,
        name: String,
        public_key: String,
        private_key: Option<SecureStr>,
        passphrase: Option<SecureStr>,
        notes: Option<String>,
    },
    SecureNote {
        id: Uuid,
        is_favorite: bool,
        expires_at: Option<DateTime<Utc>>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        version: u64,
        deleted: bool,
        deleted_at: Option<DateTime<Utc>>,
        tags: Vec<String>,
        name: String,
        notes: Option<String>,
    },
}

impl DecryptedRecord {
    pub fn id(&self) -> Uuid {
        match self {
            DecryptedRecord::Login { id, .. } => *id,
            DecryptedRecord::Api { id, .. } => *id,
            DecryptedRecord::Ssh { id, .. } => *id,
            DecryptedRecord::SecureNote { id, .. } => *id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            DecryptedRecord::Login { name, .. } => name,
            DecryptedRecord::Api { name, .. } => name,
            DecryptedRecord::Ssh { name, .. } => name,
            DecryptedRecord::SecureNote { name, .. } => name,
        }
    }

    pub fn credential_type(&self) -> CredentialType {
        match self {
            DecryptedRecord::Login { .. } => CredentialType::Login,
            DecryptedRecord::Api { .. } => CredentialType::Api,
            DecryptedRecord::Ssh { .. } => CredentialType::Ssh,
            DecryptedRecord::SecureNote { .. } => CredentialType::SecureNote,
        }
    }

    pub fn is_favorite(&self) -> bool {
        match self {
            DecryptedRecord::Login { is_favorite, .. } => *is_favorite,
            DecryptedRecord::Api { is_favorite, .. } => *is_favorite,
            DecryptedRecord::Ssh { is_favorite, .. } => *is_favorite,
            DecryptedRecord::SecureNote { is_favorite, .. } => *is_favorite,
        }
    }

    pub fn tags(&self) -> &[String] {
        match self {
            DecryptedRecord::Login { tags, .. } => tags,
            DecryptedRecord::Api { tags, .. } => tags,
            DecryptedRecord::Ssh { tags, .. } => tags,
            DecryptedRecord::SecureNote { tags, .. } => tags,
        }
    }

    pub fn is_expired(&self) -> bool {
        let expires_at = match self {
            DecryptedRecord::Login { expires_at, .. } => expires_at,
            DecryptedRecord::Api { expires_at, .. } => expires_at,
            DecryptedRecord::Ssh { expires_at, .. } => expires_at,
            DecryptedRecord::SecureNote { expires_at, .. } => expires_at,
        };
        expires_at.is_some_and(|t| t < Utc::now())
    }
}

#[derive(Debug, Clone)]
pub struct TuiRecord {
    pub id: Uuid,
    pub credential_type: CredentialType,
    pub name: String,
    pub subtitle: String,
    pub is_favorite: bool,
    pub is_expired: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub has_weak_password: bool,
    pub is_compromised: bool,
    pub duplicate_group_size: Option<usize>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted: bool,
    pub deleted_at: Option<DateTime<Utc>>,
    pub tags: Vec<String>,
    pub sync_status: Option<SyncStatus>,
}

impl PartialEq for TuiRecord {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.credential_type == other.credential_type
            && self.name == other.name
            && self.subtitle == other.subtitle
            && self.is_favorite == other.is_favorite
            && self.is_expired == other.is_expired
            && self.has_weak_password == other.has_weak_password
            && self.is_compromised == other.is_compromised
            && self.duplicate_group_size == other.duplicate_group_size
            && self.tags == other.tags
            && self.sync_status == other.sync_status
    }
}

/// Parameters for creating a new vault record.
#[derive(Debug)]
pub struct CreateRecordParams {
    pub credential_type: CredentialType,
    pub payload: EncryptedPayload,
    pub tags: Vec<String>,
    pub is_favorite: bool,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Parameters for updating an existing vault record.
#[derive(Debug)]
pub struct UpdateRecordParams {
    pub id: Uuid,
    pub payload: EncryptedPayload,
    pub tags: Vec<String>,
    pub is_favorite: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub expected_version: u64,
}
