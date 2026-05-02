use rusqlite::Connection;

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
