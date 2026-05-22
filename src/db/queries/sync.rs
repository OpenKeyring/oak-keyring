use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

use crate::db::models::SyncStateRow;
use crate::types::sync::{SyncState, SyncStatus};

use std::collections::HashMap;

use super::Result;

/// Load all sync states from the `sync_state` table.
///
/// Returns a map from record_id (string) to `SyncState`.
/// Returns an empty map if the table doesn't exist or has no rows.
pub fn load_sync_states(conn: &Connection) -> Result<HashMap<String, SyncState>> {
    let mut map = HashMap::new();

    let mut stmt = match conn.prepare(
        "SELECT record_id, cloud_updated_at, local_updated_at, sync_status, conflict_data FROM sync_state",
    ) {
        Ok(s) => s,
        Err(_) => return Ok(map),
    };

    let rows = stmt.query_map([], SyncStateRow::from_row)?;

    for row in rows {
        let row = match row {
            Ok(r) => r,
            Err(_) => continue,
        };
        match row.to_sync_state() {
            Ok(state) => {
                map.insert(state.record_id.to_string(), state);
            }
            Err(_) => continue,
        }
    }

    Ok(map)
}

/// Load all sync statuses from the `sync_state` table.
///
/// Returns a map from record_id (string) to `SyncStatus`.
/// Returns an empty map if the table doesn't exist or has no rows.
pub fn load_sync_status_map(conn: &Connection) -> HashMap<String, SyncStatus> {
    load_sync_states(conn)
        .unwrap_or_default()
        .into_iter()
        .map(|(id, state)| (id, state.sync_status))
        .collect()
}

/// Insert or update a sync state row, marking the record as pending sync.
///
/// If a row already exists for the given record, its `sync_status` is set to
/// `Pending` and `local_updated_at` is advanced to now. If no row exists, a
/// new one is created with `Pending` status.
pub fn upsert_sync_state_pending(conn: &Connection, record_id: &Uuid) -> Result<()> {
    let now_ts = Utc::now().timestamp();
    let id_str = record_id.to_string();
    let status_val = SyncStatus::Pending as i64;

    conn.execute(
        "INSERT INTO sync_state (record_id, local_updated_at, sync_status)
             VALUES (?1, ?2, ?3)
         ON CONFLICT(record_id) DO UPDATE SET
             local_updated_at = excluded.local_updated_at,
             sync_status = excluded.sync_status",
        rusqlite::params![id_str, now_ts, status_val],
    )?;

    Ok(())
}

/// Insert or update a sync state row, marking the record as synced.
///
/// Clears any stored conflict payload and advances both cloud/local timestamps
/// to the current time so subsequent sync runs treat the local row as clean.
pub fn mark_sync_state_synced(conn: &Connection, record_id: &Uuid) -> Result<()> {
    let now_ts = Utc::now().timestamp();
    let id_str = record_id.to_string();
    let status_val = SyncStatus::Synced as i64;

    conn.execute(
        "INSERT INTO sync_state
            (record_id, cloud_updated_at, local_updated_at, sync_status, conflict_data)
         VALUES (?1, ?2, ?3, ?4, NULL)
         ON CONFLICT(record_id) DO UPDATE SET
             cloud_updated_at = excluded.cloud_updated_at,
             local_updated_at = excluded.local_updated_at,
             sync_status = excluded.sync_status,
             conflict_data = NULL",
        rusqlite::params![id_str, now_ts, now_ts, status_val],
    )?;

    Ok(())
}

/// Insert or update a sync state row, marking the record as conflicted.
///
/// The serialized conflict payload is persisted for later KeepRemote
/// resolution. `local_updated_at` advances so the row reflects when the
/// conflict was observed.
pub fn mark_sync_state_conflict(
    conn: &Connection,
    record_id: &Uuid,
    conflict_data: &[u8],
) -> Result<()> {
    let now_ts = Utc::now().timestamp();
    let id_str = record_id.to_string();
    let status_val = SyncStatus::Conflict as i64;

    conn.execute(
        "INSERT INTO sync_state
            (record_id, local_updated_at, sync_status, conflict_data)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(record_id) DO UPDATE SET
             local_updated_at = excluded.local_updated_at,
             sync_status = excluded.sync_status,
             conflict_data = excluded.conflict_data",
        rusqlite::params![id_str, now_ts, status_val, conflict_data],
    )?;

    Ok(())
}
