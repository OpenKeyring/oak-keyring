use rusqlite::Connection;
use uuid::Uuid;

use crate::db::models::{datetime_to_timestamp, RecordHealthStateRow};
use crate::types::health::RecordHealthState;

use super::{DbError, Result};

// ---------------------------------------------------------------------------
// Read operations
// ---------------------------------------------------------------------------

/// Get the health state for a single record.
///
/// Returns `None` if no health state row exists for the given `record_id`.
pub fn get_record_health_state(
    conn: &Connection,
    record_id: &Uuid,
) -> Result<Option<RecordHealthState>> {
    let result = conn.query_row(
        "SELECT record_id, record_version, evaluated_at, weak_password,
                duplicate_group_size, compromised, expired
         FROM record_health_state
         WHERE record_id = ?1",
        rusqlite::params![record_id.to_string()],
        RecordHealthStateRow::from_row,
    );

    match result {
        Ok(row) => Ok(Some(row.to_health_state()?)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(DbError::from(e)),
    }
}

/// List all health states.
///
/// Returns an empty vector when no rows exist.
pub fn list_record_health_states(conn: &Connection) -> Result<Vec<RecordHealthState>> {
    let mut stmt = conn.prepare(
        "SELECT record_id, record_version, evaluated_at, weak_password,
                duplicate_group_size, compromised, expired
         FROM record_health_state",
    )?;

    let rows = stmt.query_map([], RecordHealthStateRow::from_row)?;

    let mut states = Vec::new();
    for row_result in rows {
        let row = row_result?;
        states.push(row.to_health_state()?);
    }

    Ok(states)
}

// ---------------------------------------------------------------------------
// Write operations
// ---------------------------------------------------------------------------

/// Insert or update the health state for a single record.
pub fn upsert_record_health_state(conn: &Connection, state: &RecordHealthState) -> Result<()> {
    let weak = state.weak_password.map(|b| b as i64);
    let dup = state.duplicate_group_size.map(|v| v as i64);
    let compromised = state.compromised.map(|b| b as i64);
    let expired = state.expired.map(|b| b as i64);
    let evaluated_at = state.evaluated_at.map(|dt| datetime_to_timestamp(&dt));

    conn.execute(
        "INSERT OR REPLACE INTO record_health_state
            (record_id, record_version, evaluated_at, weak_password,
             duplicate_group_size, compromised, expired)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            state.record_id.to_string(),
            state.record_version as i64,
            evaluated_at,
            weak,
            dup,
            compromised,
            expired,
        ],
    )?;

    Ok(())
}

/// Replace all health states in a single transaction.
///
/// Deletes every existing row, then inserts the provided slice. Use this
/// after a full health-check pass to atomically swap the old states for new
/// ones.
pub fn replace_record_health_states(conn: &Connection, states: &[RecordHealthState]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;

    tx.execute("DELETE FROM record_health_state", [])?;

    for state in states {
        let weak = state.weak_password.map(|b| b as i64);
        let dup = state.duplicate_group_size.map(|v| v as i64);
        let compromised = state.compromised.map(|b| b as i64);
        let expired = state.expired.map(|b| b as i64);
        let evaluated_at = state.evaluated_at.map(|dt| datetime_to_timestamp(&dt));

        tx.execute(
            "INSERT INTO record_health_state
                (record_id, record_version, evaluated_at, weak_password,
                 duplicate_group_size, compromised, expired)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                state.record_id.to_string(),
                state.record_version as i64,
                evaluated_at,
                weak,
                dup,
                compromised,
                expired,
            ],
        )?;
    }

    tx.commit()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Delete operations
// ---------------------------------------------------------------------------

/// Delete the health state for a single record.
///
/// Returns `true` if a row was deleted, `false` if no row existed.
pub fn delete_record_health_state(conn: &Connection, record_id: &Uuid) -> Result<bool> {
    let affected = conn.execute(
        "DELETE FROM record_health_state WHERE record_id = ?1",
        rusqlite::params![record_id.to_string()],
    )?;
    Ok(affected > 0)
}

/// Delete health states for multiple records.
///
/// Returns the number of deleted rows. An empty `record_ids` slice is a
/// no-op that returns `0`.
pub fn delete_record_health_states(conn: &Connection, record_ids: &[Uuid]) -> Result<usize> {
    if record_ids.is_empty() {
        return Ok(0);
    }

    let placeholders: Vec<String> = (1..=record_ids.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "DELETE FROM record_health_state WHERE record_id IN ({})",
        placeholders.join(", ")
    );

    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::with_capacity(record_ids.len());
    for id in record_ids {
        params.push(Box::new(id.to_string()));
    }

    let affected = conn.execute(&sql, rusqlite::params_from_iter(params))?;
    Ok(affected)
}

// ---------------------------------------------------------------------------
// Version bump
// ---------------------------------------------------------------------------

/// Advance the `record_version` on an existing health state row.
///
/// This is used when a record is updated *without* a password change (e.g.
/// editing a note or URL) so the existing health state carries forward to
/// the new version.
///
/// Returns `true` if a row was updated, `false` if no row existed.
pub fn copy_record_health_state_version(
    conn: &Connection,
    record_id: &Uuid,
    new_record_version: u64,
) -> Result<bool> {
    let affected = conn.execute(
        "UPDATE record_health_state
         SET record_version = ?1
         WHERE record_id = ?2",
        rusqlite::params![new_record_version as i64, record_id.to_string()],
    )?;

    Ok(affected > 0)
}
