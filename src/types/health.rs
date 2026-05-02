use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Persisted health state for a single record.
///
/// Each field uses tri-state semantics (`Option<bool>` / `Option<usize>`) so
/// that "not yet evaluated" is distinct from "evaluated as false / 0".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordHealthState {
    pub record_id: Uuid,
    pub record_version: u64,
    pub evaluated_at: Option<DateTime<Utc>>,
    pub weak_password: Option<bool>,
    pub duplicate_group_size: Option<usize>,
    pub compromised: Option<bool>,
    pub expired: Option<bool>,
}

/// Snapshot of a health-state change for a single record.
///
/// Used by callers that need to compute diffs or emit audit entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthStateDelta {
    pub record_id: Uuid,
    pub before: Option<RecordHealthState>,
    pub after: Option<RecordHealthState>,
}
