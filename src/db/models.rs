use chrono::{DateTime, TimeZone, Utc};
use rusqlite::Row;
use uuid::Uuid;

use crate::db::queries::DbError;
use crate::types::audit::{AuditEntry, AuditOperation};
use crate::types::credential::{CredentialType, DataError};
use crate::types::health::{HealthStateDelta, RecordHealthState};
use crate::types::record::StoredRecord;
use crate::types::sync::{SyncState, SyncStatus};
use crate::types::tag::Tag;

// ---------------------------------------------------------------------------
// Timestamp helpers
// ---------------------------------------------------------------------------

/// Convert a Unix timestamp (seconds) to `DateTime<Utc>`.
pub(crate) fn timestamp_to_datetime(ts: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(ts, 0)
        .single()
        .expect("invalid timestamp")
}

/// Convert a `DateTime<Utc>` to a Unix timestamp (seconds).
pub(crate) fn datetime_to_timestamp(dt: &DateTime<Utc>) -> i64 {
    dt.timestamp()
}

// ---------------------------------------------------------------------------
// RecordRow
// ---------------------------------------------------------------------------

/// Row model for the `records` table.
pub(crate) struct RecordRow {
    pub(crate) id: String,
    pub(crate) credential_type: String,
    pub(crate) encrypted_data: Vec<u8>,
    pub(crate) nonce: Vec<u8>,
    pub(crate) dek_version: i64,
    pub(crate) aad: Option<Vec<u8>>,
    pub(crate) is_favorite: i64,
    pub(crate) expires_at: Option<i64>,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) updated_by: String,
    pub(crate) version: i64,
    pub(crate) deleted: i64,
    pub(crate) deleted_at: Option<i64>,
}

impl RecordRow {
    /// Read a `records` row by column name.
    pub(crate) fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(RecordRow {
            id: row.get("id")?,
            credential_type: row.get("credential_type")?,
            encrypted_data: row.get("encrypted_data")?,
            nonce: row.get("nonce")?,
            dek_version: row.get("dek_version")?,
            aad: row.get("aad")?,
            is_favorite: row.get("is_favorite")?,
            expires_at: row.get("expires_at")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            updated_by: row.get("updated_by")?,
            version: row.get("version")?,
            deleted: row.get("deleted")?,
            deleted_at: row.get("deleted_at")?,
        })
    }

    /// Convert into the domain `StoredRecord`, attaching the given tags.
    /// Takes `self` by value to consume the row model and transfer ownership.
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_stored_record(self, tags: Vec<String>) -> Result<StoredRecord, DataError> {
        let id = Uuid::parse_str(&self.id).map_err(|_| DataError::InvalidUuid(self.id))?;

        let credential_type = CredentialType::from_db_str(&self.credential_type)?;

        let nonce: [u8; 24] =
            self.nonce
                .try_into()
                .map_err(|v: Vec<u8>| DataError::FieldTooLong {
                    field: "nonce",
                    max: 24,
                    actual: v.len(),
                })?;

        Ok(StoredRecord {
            id,
            credential_type,
            encrypted_data: self.encrypted_data,
            nonce,
            dek_version: self.dek_version as u32,
            aad: self.aad.unwrap_or_default(),
            is_favorite: self.is_favorite != 0,
            expires_at: self.expires_at.map(timestamp_to_datetime),
            created_at: timestamp_to_datetime(self.created_at),
            updated_at: timestamp_to_datetime(self.updated_at),
            updated_by: self.updated_by,
            version: self.version as u64,
            deleted: self.deleted != 0,
            deleted_at: self.deleted_at.map(timestamp_to_datetime),
            tags,
        })
    }
}

// ---------------------------------------------------------------------------
// TagRow
// ---------------------------------------------------------------------------

/// Row model for the `tags` table.
pub(crate) struct TagRow {
    pub(crate) id: i64,
    pub(crate) name: String,
}

impl TagRow {
    // Used by integration tests; TagRow rows are read via inline closures in
    // queries.rs, so the service layer does not call this directly.
    #[cfg(test)]
    pub(crate) fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(TagRow {
            id: row.get("id")?,
            name: row.get("name")?,
        })
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_tag(self) -> Tag {
        Tag {
            id: self.id,
            name: self.name,
        }
    }
}

// ---------------------------------------------------------------------------
// AuditLogRow
// ---------------------------------------------------------------------------

/// Row model for the `audit_log` table.
pub(crate) struct AuditLogRow {
    pub(crate) id: i64,
    pub(crate) operation: String,
    pub(crate) record_id: Option<String>,
    pub(crate) record_name: Option<String>,
    pub(crate) detail: Option<String>,
    pub(crate) occurred_at: i64,
}

impl AuditLogRow {
    pub(crate) fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(AuditLogRow {
            id: row.get("id")?,
            operation: row.get("operation")?,
            record_id: row.get("record_id")?,
            record_name: row.get("record_name")?,
            detail: row.get("detail")?,
            occurred_at: row.get("occurred_at")?,
        })
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_audit_entry(self) -> Result<AuditEntry, DataError> {
        let operation = AuditOperation::from_db_str(&self.operation)?;
        let record_id = self
            .record_id
            .map(|s| Uuid::parse_str(&s).map_err(|_| DataError::InvalidUuid(s)))
            .transpose()?;

        Ok(AuditEntry {
            id: self.id,
            operation,
            record_id,
            record_name: self.record_name,
            detail: self.detail,
            occurred_at: timestamp_to_datetime(self.occurred_at),
        })
    }
}

// ---------------------------------------------------------------------------
// SyncStateRow
// ---------------------------------------------------------------------------

/// Row model for the `sync_state` table.
///
/// Not yet consumed by the service layer — will be used when the sync pipeline
/// reads/writes local sync state.
pub(crate) struct SyncStateRow {
    pub(crate) record_id: String,
    pub(crate) cloud_updated_at: Option<i64>,
    pub(crate) local_updated_at: i64,
    pub(crate) sync_status: i64,
    pub(crate) conflict_data: Option<Vec<u8>>,
}

impl SyncStateRow {
    pub(crate) fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(SyncStateRow {
            record_id: row.get("record_id")?,
            cloud_updated_at: row.get("cloud_updated_at")?,
            local_updated_at: row.get("local_updated_at")?,
            sync_status: row.get("sync_status")?,
            conflict_data: row.get("conflict_data")?,
        })
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_sync_state(self) -> Result<SyncState, DataError> {
        let record_id =
            Uuid::parse_str(&self.record_id).map_err(|_| DataError::InvalidUuid(self.record_id))?;

        let sync_status = match self.sync_status {
            0 => SyncStatus::Pending,
            1 => SyncStatus::Synced,
            2 => SyncStatus::Conflict,
            _ => return Err(DataError::InvalidSyncStatus(self.sync_status)),
        };

        Ok(SyncState {
            record_id,
            cloud_updated_at: self.cloud_updated_at.map(timestamp_to_datetime),
            local_updated_at: timestamp_to_datetime(self.local_updated_at),
            sync_status,
            conflict_data: self.conflict_data,
        })
    }
}

// ---------------------------------------------------------------------------
// RecordHealthStateRow
// ---------------------------------------------------------------------------

/// Row model for the `record_health_state` table.
pub(crate) struct RecordHealthStateRow {
    pub(crate) record_id: String,
    pub(crate) record_version: i64,
    pub(crate) evaluated_at: Option<i64>,
    pub(crate) weak_password: Option<i64>,
    pub(crate) duplicate_group_size: Option<i64>,
    pub(crate) compromised: Option<i64>,
    pub(crate) expired: Option<i64>,
}

impl RecordHealthStateRow {
    pub(crate) fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(RecordHealthStateRow {
            record_id: row.get("record_id")?,
            record_version: row.get("record_version")?,
            evaluated_at: row.get("evaluated_at")?,
            weak_password: row.get("weak_password")?,
            duplicate_group_size: row.get("duplicate_group_size")?,
            compromised: row.get("compromised")?,
            expired: row.get("expired")?,
        })
    }

    /// Convert into the domain `RecordHealthState`.
    ///
    /// Uses `bool_from_int` / `int_from_bool` helpers to bridge the
    /// SQLite INTEGER ↔ Rust `Option<bool>` gap.
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_health_state(self) -> Result<RecordHealthState, DbError> {
        let record_id = Uuid::parse_str(&self.record_id).map_err(DbError::Uuid)?;

        Ok(RecordHealthState {
            record_id,
            record_version: self.record_version as u64,
            evaluated_at: self.evaluated_at.map(timestamp_to_datetime),
            weak_password: self.weak_password.map(bool_from_int),
            duplicate_group_size: self.duplicate_group_size.map(|v| v as usize),
            compromised: self.compromised.map(bool_from_int),
            expired: self.expired.map(bool_from_int),
        })
    }
}

// ---------------------------------------------------------------------------
// Int <-> Bool conversion helpers
// ---------------------------------------------------------------------------

/// Convert an SQLite INTEGER (0/1) to a Rust `bool`.
fn bool_from_int(v: i64) -> bool {
    v != 0
}

// ---------------------------------------------------------------------------
// HealthStateDelta helpers
// ---------------------------------------------------------------------------

/// Build a `HealthStateDelta` from optional before/after snapshots.
#[allow(dead_code)]
pub(crate) fn build_health_delta(
    record_id: Uuid,
    before: Option<RecordHealthState>,
    after: Option<RecordHealthState>,
) -> HealthStateDelta {
    HealthStateDelta {
        record_id,
        before,
        after,
    }
}
