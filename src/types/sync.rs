use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncStatus {
    Pending = 0,
    Synced = 1,
    Conflict = 2,
}

#[derive(Debug, Clone)]
pub struct SyncState {
    pub record_id: Uuid,
    pub cloud_updated_at: Option<DateTime<Utc>>,
    pub local_updated_at: DateTime<Utc>,
    pub sync_status: SyncStatus,
    pub conflict_data: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
pub struct SyncStats {
    pub total: i64,
    pub pending: i64,
    pub synced: i64,
    pub conflicts: i64,
}
