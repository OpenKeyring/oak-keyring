use chrono::Utc;
use rusqlite::{Connection, ToSql};
use uuid::Uuid;

use crate::commands::types::{
    RecordCategoryCounts, RecordFilter, RecordSort, SortDirection, SortField,
};
use crate::db::models::{datetime_to_timestamp, RecordRow};
use crate::db::queries::DbError;
use crate::types::record::StoredRecord;

use super::Result;

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Get tag names for a record via JOIN on `record_tags` + `tags`.
pub(super) fn get_record_tags_inner(conn: &Connection, record_id: &Uuid) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.name FROM tags t
         INNER JOIN record_tags rt ON rt.tag_id = t.id
         WHERE rt.record_id = ?1",
    )?;

    let rows = stmt.query_map(rusqlite::params![record_id.to_string()], |row| {
        row.get::<_, String>(0)
    })?;

    let mut tags = Vec::new();
    for tag in rows {
        tags.push(tag?);
    }

    Ok(tags)
}

#[derive(Debug, Clone)]
pub struct RecordListPageRow {
    pub record: StoredRecord,
    pub name: String,
    pub subtitle: String,
}

fn search_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|term| term.to_lowercase())
        .filter(|term| !term.is_empty())
        .collect()
}

fn record_list_where_clause(filter: &RecordFilter) -> (String, Vec<String>) {
    let mut clauses = vec![if matches!(filter, RecordFilter::Trash) {
        "r.deleted = 1".to_string()
    } else {
        "r.deleted = 0".to_string()
    }];
    let mut params = Vec::new();

    match filter {
        RecordFilter::Favorites => clauses.push("r.is_favorite = 1".to_string()),
        RecordFilter::Tag(tag_name) => {
            clauses.push(
                "EXISTS (
                    SELECT 1
                    FROM record_tags rt
                    INNER JOIN tags t ON t.id = rt.tag_id
                    WHERE rt.record_id = r.id AND t.name = ?
                )"
                .to_string(),
            );
            params.push(tag_name.clone());
        }
        RecordFilter::Search(query) => {
            for term in search_terms(query) {
                clauses.push("i.search_text LIKE ?".to_string());
                params.push(format!("%{term}%"));
            }
        }
        RecordFilter::Expired => clauses.push(
            "EXISTS (
                SELECT 1
                FROM record_health_state rhs
                WHERE rhs.record_id = r.id AND rhs.expired = 1
            )"
            .to_string(),
        ),
        RecordFilter::HealthIssues => clauses.push(
            "EXISTS (
                SELECT 1
                FROM record_health_state rhs
                WHERE rhs.record_id = r.id
                  AND (
                    rhs.weak_password = 1
                    OR rhs.compromised = 1
                    OR rhs.expired = 1
                    OR rhs.duplicate_group_size IS NOT NULL
                  )
            )"
            .to_string(),
        ),
        RecordFilter::All | RecordFilter::Trash => {}
    }

    (clauses.join(" AND "), params)
}

fn record_list_order_clause(sort: &RecordSort) -> String {
    let direction = match sort.direction {
        SortDirection::Asc => "ASC",
        SortDirection::Desc => "DESC",
    };
    let primary = match sort.field {
        SortField::Name => format!("i.name_sort_key COLLATE NOCASE {direction}"),
        SortField::CreatedAt => format!("r.created_at {direction}"),
        SortField::UpdatedAt => format!("r.updated_at {direction}"),
        SortField::UsageFrequency => {
            format!("(SELECT COUNT(*) FROM audit_log a WHERE a.record_id = r.id) {direction}")
        }
    };
    format!("{primary}, r.updated_at DESC, r.id ASC")
}

// ---------------------------------------------------------------------------
// Record CRUD queries
// ---------------------------------------------------------------------------

/// Insert a new record with all fields using a parameterized query.
///
/// Wrapped in a transaction so that record + tags are atomic — a partial
/// failure rolls back the entire insert.
pub fn insert_record(conn: &Connection, record: &StoredRecord) -> Result<()> {
    let tx = conn.unchecked_transaction()?;

    let aad_param: Option<&[u8]> = if record.aad.is_empty() {
        None
    } else {
        Some(&record.aad)
    };

    tx.execute(
        "INSERT INTO records
            (id, credential_type, encrypted_data, nonce, dek_version, aad,
             is_favorite, expires_at, created_at, updated_at, updated_by,
             version, deleted, deleted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        rusqlite::params![
            record.id.to_string(),
            record.credential_type.to_db_str(),
            record.encrypted_data,
            record.nonce.as_slice(),
            record.dek_version,
            aad_param,
            record.is_favorite as i64,
            record.expires_at.map(|dt| datetime_to_timestamp(&dt)),
            datetime_to_timestamp(&record.created_at),
            datetime_to_timestamp(&record.updated_at),
            record.updated_by,
            record.version as i64,
            record.deleted as i64,
            record.deleted_at.map(|dt| datetime_to_timestamp(&dt)),
        ],
    )?;

    // Insert tags: ensure each tag exists in `tags` table and link via `record_tags`.
    for tag_name in &record.tags {
        tx.execute(
            "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
            rusqlite::params![tag_name],
        )?;

        let tag_id: i64 = tx.query_row(
            "SELECT id FROM tags WHERE name = ?1",
            rusqlite::params![tag_name],
            |row| row.get(0),
        )?;

        tx.execute(
            "INSERT OR IGNORE INTO record_tags (record_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![record.id.to_string(), tag_id],
        )?;
    }

    tx.commit()?;
    Ok(())
}

pub fn upsert_record_list_index(
    conn: &Connection,
    record_id: &Uuid,
    name: &str,
    subtitle: &str,
) -> Result<()> {
    let search_text = format!("{} {}", name.to_lowercase(), subtitle.to_lowercase());
    let name_sort_key = name.to_lowercase();
    conn.execute(
        "INSERT INTO record_list_index
            (record_id, name, subtitle, search_text, name_sort_key)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(record_id) DO UPDATE SET
            name = excluded.name,
            subtitle = excluded.subtitle,
            search_text = excluded.search_text,
            name_sort_key = excluded.name_sort_key",
        rusqlite::params![
            record_id.to_string(),
            name,
            subtitle,
            search_text,
            name_sort_key
        ],
    )?;
    Ok(())
}

pub fn clear_record_list_index(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM record_list_index", [])?;
    Ok(())
}

pub fn count_record_list_index(conn: &Connection) -> Result<usize> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM record_list_index", [], |row| {
        row.get(0)
    })?;
    Ok(count as usize)
}

pub fn count_all_records(conn: &Connection) -> Result<usize> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))?;
    Ok(count as usize)
}

pub fn list_all_records(conn: &Connection) -> Result<Vec<StoredRecord>> {
    let mut stmt = conn.prepare("SELECT * FROM records ORDER BY updated_at DESC")?;
    let rows = stmt.query_map([], RecordRow::from_row)?;

    let mut records = Vec::new();
    for row_result in rows {
        let row = row_result?;
        let id = Uuid::parse_str(&row.id).map_err(DbError::Uuid)?;
        let tags = get_record_tags_inner(conn, &id)?;
        let record = row.to_stored_record(tags)?;
        records.push(record);
    }

    Ok(records)
}

pub fn count_record_list_page(conn: &Connection, filter: &RecordFilter) -> Result<usize> {
    let (where_clause, values) = record_list_where_clause(filter);
    let sql = format!(
        "SELECT COUNT(*)
         FROM records r
         INNER JOIN record_list_index i ON i.record_id = r.id
         WHERE {where_clause}"
    );
    let params: Vec<&dyn ToSql> = values.iter().map(|value| value as &dyn ToSql).collect();
    let count: i64 = conn.query_row(&sql, params.as_slice(), |row| row.get(0))?;
    Ok(count as usize)
}

pub fn list_record_page_rows(
    conn: &Connection,
    filter: &RecordFilter,
    sort: &RecordSort,
    limit: usize,
    offset: usize,
) -> Result<Vec<RecordListPageRow>> {
    let (where_clause, values) = record_list_where_clause(filter);
    let order_clause = record_list_order_clause(sort);
    let limit = limit.min(i64::MAX as usize) as i64;
    let offset = offset.min(i64::MAX as usize) as i64;
    let sql = format!(
        "SELECT
            r.id,
            r.credential_type,
            r.encrypted_data,
            r.nonce,
            r.dek_version,
            r.aad,
            r.is_favorite,
            r.expires_at,
            r.created_at,
            r.updated_at,
            r.updated_by,
            r.version,
            r.deleted,
            r.deleted_at,
            i.name AS list_name,
            i.subtitle AS list_subtitle
         FROM records r
         INNER JOIN record_list_index i ON i.record_id = r.id
         WHERE {where_clause}
         ORDER BY {order_clause}
         LIMIT ? OFFSET ?"
    );
    let mut params: Vec<&dyn ToSql> = values.iter().map(|value| value as &dyn ToSql).collect();
    params.push(&limit);
    params.push(&offset);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params.as_slice(), |row| {
        let record_row = RecordRow::from_row(row)?;
        let name = row.get::<_, String>("list_name")?;
        let subtitle = row.get::<_, String>("list_subtitle")?;
        Ok((record_row, name, subtitle))
    })?;

    let mut page = Vec::new();
    for row_result in rows {
        let (record_row, name, subtitle) = row_result?;
        let id = Uuid::parse_str(&record_row.id).map_err(DbError::Uuid)?;
        let tags = get_record_tags_inner(conn, &id)?;
        let record = record_row.to_stored_record(tags)?;
        page.push(RecordListPageRow {
            record,
            name,
            subtitle,
        });
    }

    Ok(page)
}

pub fn record_category_counts(conn: &Connection) -> Result<RecordCategoryCounts> {
    let all = count_scalar(conn, "SELECT COUNT(*) FROM records WHERE deleted = 0")?;
    let favorites = count_scalar(
        conn,
        "SELECT COUNT(*) FROM records WHERE deleted = 0 AND is_favorite = 1",
    )?;
    let expired = count_scalar(
        conn,
        "SELECT COUNT(*)
         FROM records r
         INNER JOIN record_health_state rhs ON rhs.record_id = r.id
         WHERE r.deleted = 0 AND rhs.expired = 1",
    )?;
    let health_issues = count_scalar(
        conn,
        "SELECT COUNT(*)
         FROM records r
         INNER JOIN record_health_state rhs ON rhs.record_id = r.id
         WHERE r.deleted = 0
           AND (
             rhs.weak_password = 1
             OR rhs.compromised = 1
             OR rhs.expired = 1
             OR rhs.duplicate_group_size IS NOT NULL
           )",
    )?;
    let trash = count_scalar(conn, "SELECT COUNT(*) FROM records WHERE deleted = 1")?;

    Ok(RecordCategoryCounts {
        all,
        favorites,
        expired,
        health_issues,
        trash,
    })
}

fn count_scalar(conn: &Connection, sql: &str) -> Result<usize> {
    let count: i64 = conn.query_row(sql, [], |row| row.get(0))?;
    Ok(count as usize)
}

/// Fetch a single record by ID. Returns `None` if not found.
pub fn get_record(conn: &Connection, id: &Uuid) -> Result<Option<StoredRecord>> {
    let id_str = id.to_string();
    let result = conn.query_row(
        "SELECT * FROM records WHERE id = ?1",
        rusqlite::params![id_str],
        RecordRow::from_row,
    );

    match result {
        Ok(row) => {
            let tags = get_record_tags_inner(conn, id)?;
            let record = row.to_stored_record(tags)?;
            Ok(Some(record))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(DbError::from(e)),
    }
}

/// List all active (non-deleted) records, ordered by `updated_at` descending.
pub fn list_active_records(conn: &Connection) -> Result<Vec<StoredRecord>> {
    let mut stmt =
        conn.prepare("SELECT * FROM records WHERE deleted = 0 ORDER BY updated_at DESC")?;

    let rows = stmt.query_map([], RecordRow::from_row)?;

    let mut records = Vec::new();
    for row_result in rows {
        let row = row_result?;
        let id = Uuid::parse_str(&row.id).map_err(DbError::Uuid)?;
        let tags = get_record_tags_inner(conn, &id)?;
        let record = row.to_stored_record(tags)?;
        records.push(record);
    }

    Ok(records)
}

/// Soft-delete a record: set `deleted = 1` and `deleted_at` to now.
pub fn soft_delete_record(conn: &Connection, id: &Uuid) -> Result<()> {
    let now = datetime_to_timestamp(&Utc::now());
    conn.execute(
        "UPDATE records SET deleted = 1, deleted_at = ?1 WHERE id = ?2",
        rusqlite::params![now, id.to_string()],
    )?;
    Ok(())
}

/// Restore a soft-deleted record: set `deleted = 0` and `deleted_at = NULL`.
pub fn restore_record(conn: &Connection, id: &Uuid) -> Result<()> {
    conn.execute(
        "UPDATE records SET deleted = 0, deleted_at = NULL WHERE id = ?1",
        rusqlite::params![id.to_string()],
    )?;
    Ok(())
}

/// Hard-delete a record (and cascading record_tags via FK).
pub fn hard_delete_record(conn: &Connection, id: &Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM records WHERE id = ?1",
        rusqlite::params![id.to_string()],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Record update queries
// ---------------------------------------------------------------------------

/// Update a record with optimistic locking.
///
/// Returns `Ok(true)` if the update succeeded (version matched), or
/// `Ok(false)` if no rows were affected (version mismatch or record not found).
pub fn update_record(
    conn: &Connection,
    record: &StoredRecord,
    expected_version: u64,
) -> Result<bool> {
    let aad_param: Option<&[u8]> = if record.aad.is_empty() {
        None
    } else {
        Some(&record.aad)
    };

    let affected = conn.execute(
        "UPDATE records SET
            encrypted_data = ?1,
            nonce = ?2,
            dek_version = ?3,
            aad = ?4,
            is_favorite = ?5,
            expires_at = ?6,
            updated_at = ?7,
            updated_by = ?8,
            version = ?9
         WHERE id = ?10 AND version = ?11",
        rusqlite::params![
            record.encrypted_data,
            record.nonce.as_slice(),
            record.dek_version,
            aad_param,
            record.is_favorite as i64,
            record.expires_at.map(|dt| datetime_to_timestamp(&dt)),
            datetime_to_timestamp(&record.updated_at),
            record.updated_by,
            (record.version) as i64,
            record.id.to_string(),
            expected_version as i64,
        ],
    )?;

    Ok(affected > 0)
}

/// Update only the stored record version.
///
/// Used by sync conflict resolution when the local payload wins and the record
/// version must be advanced to the version pushed to cloud without re-encrypting
/// the unchanged payload.
pub fn update_record_version(conn: &Connection, id: &Uuid, version: u64) -> Result<()> {
    conn.execute(
        "UPDATE records SET version = ?1 WHERE id = ?2",
        rusqlite::params![version as i64, id.to_string()],
    )?;
    Ok(())
}

/// List all records (including soft-deleted) where `dek_version < target`.
/// Used for DEK rotation migration.
pub fn list_records_by_dek_version(
    conn: &Connection,
    target_version: u32,
) -> Result<Vec<StoredRecord>> {
    let mut stmt = conn.prepare("SELECT * FROM records WHERE dek_version < ?1")?;

    let rows = stmt.query_map(rusqlite::params![target_version], RecordRow::from_row)?;

    let mut records = Vec::new();
    for row_result in rows {
        let row = row_result?;
        let id = Uuid::parse_str(&row.id).map_err(DbError::Uuid)?;
        let tags = get_record_tags_inner(conn, &id)?;
        let record = row.to_stored_record(tags)?;
        records.push(record);
    }

    Ok(records)
}

/// List all soft-deleted records, ordered by `deleted_at` descending.
pub fn list_deleted_records(conn: &Connection) -> Result<Vec<StoredRecord>> {
    let mut stmt =
        conn.prepare("SELECT * FROM records WHERE deleted = 1 ORDER BY deleted_at DESC")?;

    let rows = stmt.query_map([], RecordRow::from_row)?;

    let mut records = Vec::new();
    for row_result in rows {
        let row = row_result?;
        let id = Uuid::parse_str(&row.id).map_err(DbError::Uuid)?;
        let tags = get_record_tags_inner(conn, &id)?;
        let record = row.to_stored_record(tags)?;
        records.push(record);
    }

    Ok(records)
}

// ---------------------------------------------------------------------------
// Batch operations
// ---------------------------------------------------------------------------

/// Batch soft-delete multiple records.
///
/// Returns the number of affected rows.
pub fn batch_soft_delete_records(
    conn: &Connection,
    ids: &[Uuid],
    deleted_by: &str,
) -> Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }

    let now = datetime_to_timestamp(&Utc::now());

    // Build dynamic parameterized IN clause: WHERE id IN (?3, ?4, ...)
    // Parameters ?1 = deleted_at, ?2 = updated_by, ?3.. = ids
    let placeholders: Vec<String> = (3..=(ids.len() + 2)).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "UPDATE records SET deleted = 1, deleted_at = ?1, updated_by = ?2 WHERE id IN ({})",
        placeholders.join(", ")
    );

    // Build parameter list: [now, deleted_by, id1, id2, ...]
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::with_capacity(ids.len() + 2);
    params.push(Box::new(now));
    params.push(Box::new(deleted_by.to_string()));
    for id in ids {
        params.push(Box::new(id.to_string()));
    }

    let affected = conn.execute(&sql, rusqlite::params_from_iter(params))?;
    Ok(affected)
}

/// Batch restore multiple soft-deleted records.
///
/// Only restores records where `deleted = 1`. Returns the number of affected rows.
pub fn batch_restore_records(conn: &Connection, ids: &[Uuid]) -> Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }

    let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "UPDATE records SET deleted = 0, deleted_at = NULL WHERE id IN ({}) AND deleted = 1",
        placeholders.join(", ")
    );

    let params: Vec<Box<dyn rusqlite::types::ToSql>> =
        ids.iter().map(|id| Box::new(id.to_string()) as _).collect();

    let affected = conn.execute(&sql, rusqlite::params_from_iter(params))?;
    Ok(affected)
}

/// Batch hard-delete multiple soft-deleted records (and cascading record_tags via FK).
///
/// Only deletes records where `deleted = 1`. Returns the number of affected rows.
pub fn batch_hard_delete_records(conn: &Connection, ids: &[Uuid]) -> Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }

    let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "DELETE FROM records WHERE id IN ({}) AND deleted = 1",
        placeholders.join(", ")
    );

    let params: Vec<Box<dyn rusqlite::types::ToSql>> =
        ids.iter().map(|id| Box::new(id.to_string()) as _).collect();

    let affected = conn.execute(&sql, rusqlite::params_from_iter(params))?;
    Ok(affected)
}
