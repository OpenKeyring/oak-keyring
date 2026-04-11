mod audit;
mod history;
mod metadata;
mod record;
mod search;
mod tag;
mod trash;

use std::path::Path;

use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

use crate::commands::types::{RecordFilter, RecordSort, SortDirection, SortField};
use crate::crypto::CryptoManager;
use crate::errors::mapping::vault::VaultError;
use crate::types::credential::CredentialType;
use crate::types::record::TuiRecord;

pub struct VaultService {
    conn: Connection,
    crypto: CryptoManager,
    device_id: String,
}

impl VaultService {
    pub fn new(conn: Connection) -> Self {
        let device_id = conn
            .query_row(
                "SELECT value FROM metadata WHERE key = 'device_id'",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap_or_else(|_| Uuid::new_v4().to_string());
        Self {
            conn,
            crypto: CryptoManager::new(),
            device_id,
        }
    }

    pub fn unlock(&mut self, path: &Path, cmk: &str) -> Result<(), VaultError> {
        self.crypto
            .unlock(path, cmk)
            .map_err(VaultError::CryptoError)
    }

    pub fn lock(&mut self) {
        self.crypto.lock();
    }

    pub fn is_unlocked(&self) -> bool {
        self.crypto.is_unlocked()
    }

    pub fn list_records(
        &self,
        filter: &RecordFilter,
        sort: &RecordSort,
    ) -> Result<Vec<TuiRecord>, VaultError> {
        let base_query = "SELECT r.id, r.credential_type, r.is_favorite, r.expires_at, r.created_at, r.updated_at, r.version, r.deleted FROM records r WHERE r.deleted = 0";

        let (mut query, _is_search, params): (String, bool, Vec<Box<dyn rusqlite::types::ToSql>>) =
            match filter {
                RecordFilter::All => (base_query.to_string(), false, vec![]),
                RecordFilter::Favorites => {
                    (format!("{base_query} AND r.is_favorite = 1"), false, vec![])
                }
                RecordFilter::Expired => {
                    let now = Utc::now().timestamp();
                    (
                        format!("{base_query} AND r.expires_at IS NOT NULL AND r.expires_at < ?1"),
                        false,
                        vec![Box::new(now)],
                    )
                }
                RecordFilter::Tag(tag) => (
                    format!(
                        "{base_query} AND r.id IN (SELECT rt.record_id FROM record_tags rt \
                         JOIN tags t ON rt.tag_id = t.id WHERE t.name = ?1)"
                    ),
                    false,
                    vec![Box::new(tag.clone())],
                ),
                RecordFilter::Search(_term) => (base_query.to_string(), true, vec![]),
                _ => (base_query.to_string(), false, vec![]),
            };

        let order = match (sort.field, sort.direction) {
            (SortField::Name, SortDirection::Asc) => " ORDER BY r.updated_at ASC",
            (SortField::Name, SortDirection::Desc) => " ORDER BY r.updated_at DESC",
            (SortField::CreatedAt, SortDirection::Asc) => " ORDER BY r.created_at ASC",
            (SortField::CreatedAt, SortDirection::Desc) => " ORDER BY r.created_at DESC",
            (SortField::UpdatedAt, SortDirection::Asc) => " ORDER BY r.updated_at ASC",
            (SortField::UpdatedAt, SortDirection::Desc) => " ORDER BY r.updated_at DESC",
            _ => " ORDER BY r.updated_at DESC",
        };
        query.push_str(order);

        let mut stmt = self.conn.prepare(&query)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params), |row| {
            let id: String = row.get(0)?;
            let ct_str: String = row.get(1)?;
            let ct = CredentialType::from_db_str(&ct_str).map_err(|_| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::other("invalid credential_type")),
                )
            })?;
            let is_favorite: bool = row.get::<_, i32>(2)? != 0;
            let expires_at: Option<i64> = row.get(3)?;
            let created_at: i64 = row.get(4)?;
            let updated_at: i64 = row.get(5)?;
            let _version: u64 = row.get(6)?;
            let deleted: bool = row.get::<_, i32>(7)? != 0;

            Ok(TuiRecord {
                id: id.parse().map_err(|_| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::other("invalid uuid")),
                    )
                })?,
                credential_type: ct,
                name: String::new(),
                subtitle: String::new(),
                is_favorite,
                is_expired: expires_at.is_some_and(|t| t < Utc::now().timestamp()),
                expires_at: expires_at
                    .map(|t| chrono::DateTime::from_timestamp(t, 0).unwrap_or_default()),
                has_weak_password: false,
                created_at: chrono::DateTime::from_timestamp(created_at, 0).unwrap_or_default(),
                updated_at: chrono::DateTime::from_timestamp(updated_at, 0).unwrap_or_default(),
                deleted,
                deleted_at: None,
                tags: vec![],
                sync_status: None,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(VaultError::DatabaseError)
    }

    pub fn soft_delete(&mut self, id: Uuid) -> Result<(), VaultError> {
        let now = Utc::now().timestamp();
        self.conn.execute(
            "UPDATE records SET deleted = 1, deleted_at = ?1, updated_at = ?2, version = version + 1 WHERE id = ?3 AND deleted = 0",
            rusqlite::params![now, now, id.to_string()],
        )?;
        Ok(())
    }

    pub fn restore(&mut self, id: Uuid) -> Result<(), VaultError> {
        let now = Utc::now().timestamp();
        self.conn.execute(
            "UPDATE records SET deleted = 0, deleted_at = NULL, updated_at = ?1, version = version + 1 WHERE id = ?2 AND deleted = 1",
            rusqlite::params![now, id.to_string()],
        )?;
        Ok(())
    }

    pub fn write_audit_entry(
        &mut self,
        operation: crate::types::AuditOperation,
        record_id: Option<Uuid>,
        record_name: Option<String>,
        detail: Option<String>,
    ) -> Result<(), VaultError> {
        let now = Utc::now().timestamp();
        self.conn.execute(
            "INSERT INTO audit_log (operation, record_id, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                operation.to_db_str(),
                record_id.map(|id| id.to_string()),
                record_name,
                detail,
                now,
            ],
        )?;
        Ok(())
    }

    pub fn create_tag(&mut self, name: &str) -> Result<i64, VaultError> {
        self.conn.execute(
            "INSERT INTO tags (name) VALUES (?1)",
            rusqlite::params![name],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn add_tag_to_record(&mut self, record_id: Uuid, tag_id: i64) -> Result<(), VaultError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO record_tags (record_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![record_id.to_string(), tag_id],
        )?;
        Ok(())
    }

    pub fn get_metadata(&self, key: &str) -> Result<Option<String>, VaultError> {
        let result = self.conn.query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            rusqlite::params![key],
            |r| r.get::<_, String>(0),
        );
        match result {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(VaultError::DatabaseError(e)),
        }
    }

    pub fn set_metadata(&mut self, key: &str, value: &str) -> Result<(), VaultError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO metadata (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::types::{RecordSort, SortDirection, SortField};
    use crate::crypto::bip39::{MnemonicLanguage, Passkey};
    use crate::db::schema::{initialize_metadata, initialize_schema};

    /// Helper: create an in-memory VaultService with schema ready.
    fn setup_service() -> VaultService {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn);
        initialize_metadata(&conn);
        VaultService::new(conn)
    }

    /// Helper: insert a bare record row directly (bypasses VaultService::create_record
    /// to avoid needing encrypted payload data).
    fn insert_record(
        conn: &Connection,
        id: &str,
        credential_type: &str,
        is_favorite: bool,
        expires_at: Option<i64>,
    ) {
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT INTO records (id, credential_type, encrypted_data, nonce, dek_version, is_favorite, created_at, updated_at, updated_by, version, deleted)
             VALUES (?1, ?2, X'00', X'0000000000000000000000000000000000000000000000', 1, ?3, ?4, ?5, 'test-device', 1, 0)",
            rusqlite::params![id, credential_type, is_favorite as i32, now, now],
        ).unwrap();
        if let Some(exp) = expires_at {
            conn.execute(
                "UPDATE records SET expires_at = ?1 WHERE id = ?2",
                rusqlite::params![exp, id],
            )
            .unwrap();
        }
    }

    // ─── Lock / Unlock Lifecycle Tests ──────────────────────────────────

    /// VaultService starts locked, and lock()/is_unlocked() reflect state correctly.
    #[test]
    fn vault_service_starts_locked() {
        let svc = setup_service();
        assert!(
            !svc.is_unlocked(),
            "new VaultService must start in locked state"
        );
    }

    /// lock() on a locked service is a no-op; is_unlocked() stays false.
    #[test]
    fn lock_when_already_locked_is_noop() {
        let mut svc = setup_service();
        assert!(!svc.is_unlocked());
        svc.lock();
        assert!(
            !svc.is_unlocked(),
            "locking an already-locked service must remain locked"
        );
    }

    /// Full lifecycle: unlock with mnemonic → is_unlocked(true) → lock → is_unlocked(false).
    /// This tests the delegation to CryptoManager without requiring a real keyfile on disk.
    #[test]
    fn unlock_with_mnemonic_then_lock_lifecycle() {
        let mut svc = setup_service();
        assert!(!svc.is_unlocked(), "must start locked");

        // Unlock via mnemonic (no file I/O needed).
        let mnemonic = Passkey::generate(24, MnemonicLanguage::English).unwrap();
        svc.crypto
            .unlock_with_mnemonic(&mnemonic)
            .expect("unlock_with_mnemonic must succeed in test");
        assert!(
            svc.is_unlocked(),
            "is_unlocked must return true after unlock_with_mnemonic"
        );

        // Lock and verify.
        svc.lock();
        assert!(
            !svc.is_unlocked(),
            "is_unlocked must return false after lock()"
        );
    }

    // ─── Error Type Migration Tests ─────────────────────────────────────

    /// Methods returning errors must produce VaultError variants, not String.
    #[test]
    fn get_metadata_returns_vault_error_on_db_failure() {
        // Use a connection without schema to trigger a database error.
        let conn = Connection::open_in_memory().unwrap();
        let svc = VaultService::new(conn);

        let result = svc.get_metadata("no_schema_key");
        // Without schema, the metadata table doesn't exist → DatabaseError variant.
        assert!(
            result.is_err(),
            "get_metadata on missing table must return error"
        );
        let err = result.unwrap_err();
        assert!(
            matches!(err, VaultError::DatabaseError(_)),
            "expected DatabaseError variant, got: {:?}",
            err
        );
    }

    // ─── SQL Injection Tests ────────────────────────────────────────────

    /// Tag filter with SQL special characters must not cause errors or data corruption.
    /// Before the fix, a tag like `test'; DROP TABLE records;--` would be injected
    /// into the SQL query string via format!().
    #[test]
    fn sql_injection_tag_filter_is_safe() {
        let mut svc = setup_service();

        // Insert a record that is legitimately tagged
        let record_id = Uuid::new_v4().to_string();
        insert_record(&svc.conn, &record_id, "login", false, None);

        // Create a normal tag and link it to the record
        svc.create_tag("normal_tag").unwrap();
        svc.add_tag_to_record(record_id.parse().unwrap(), 1)
            .unwrap();

        // Attempt to filter by a malicious tag name containing SQL injection
        let malicious_tag = "test'; DROP TABLE records;--".to_string();
        let filter = RecordFilter::Tag(malicious_tag);
        let sort = RecordSort {
            field: SortField::UpdatedAt,
            direction: SortDirection::Desc,
        };

        // list_records must succeed (no panic) and return zero results
        // because no tag with that name exists
        let result = svc.list_records(&filter, &sort);
        assert!(
            result.is_ok(),
            "list_records should not panic on SQL injection attempt"
        );
        let records = result.unwrap();
        assert!(
            records.is_empty(),
            "should return zero records for non-existent malicious tag"
        );

        // Verify the records table still exists and the original record is intact
        let all_result = svc.list_records(
            &RecordFilter::All,
            &RecordSort {
                field: SortField::UpdatedAt,
                direction: SortDirection::Desc,
            },
        );
        assert!(all_result.is_ok());
        assert_eq!(
            all_result.unwrap().len(),
            1,
            "original record should still exist"
        );
    }

    /// Tag filter must not allow bypassing the WHERE clause via injected SQL.
    /// Before the parameterized fix, a tag name like `' OR '1'='1` would turn
    /// the subquery into `WHERE t.name = '' OR '1'='1'` which matches all tags,
    /// leaking records that should not be visible under this filter.
    #[test]
    fn sql_injection_tag_cannot_bypass_where_clause() {
        let mut svc = setup_service();

        // Insert two records: one tagged "secret", one untagged
        let id_tagged = Uuid::new_v4().to_string();
        let id_untagged = Uuid::new_v4().to_string();
        insert_record(&svc.conn, &id_tagged, "login", false, None);
        insert_record(&svc.conn, &id_untagged, "login", false, None);

        svc.create_tag("secret").unwrap();
        svc.add_tag_to_record(id_tagged.parse().unwrap(), 1)
            .unwrap();

        let sort = RecordSort {
            field: SortField::UpdatedAt,
            direction: SortDirection::Desc,
        };

        // This malicious tag name would bypass the WHERE clause with string concat:
        //   WHERE t.name = '' OR '1'='1'
        // With parameterized query, it is treated as a literal string and matches nothing.
        let injection_tag = "' OR '1'='1".to_string();
        let result = svc
            .list_records(&RecordFilter::Tag(injection_tag), &sort)
            .unwrap();
        assert!(
            result.is_empty(),
            "SQL injection via OR '1'='1 should match zero records with parameterized query"
        );

        // Filtering by the actual tag should still return exactly one record
        let legit = svc
            .list_records(&RecordFilter::Tag("secret".into()), &sort)
            .unwrap();
        assert_eq!(
            legit.len(),
            1,
            "legitimate tag filter should return the tagged record"
        );
    }

    /// Tag filter should correctly match records by tag name using parameterized query.
    #[test]
    fn tag_filter_returns_matching_records() {
        let mut svc = setup_service();

        let id_a = Uuid::new_v4().to_string();
        let id_b = Uuid::new_v4().to_string();
        insert_record(&svc.conn, &id_a, "login", false, None);
        insert_record(&svc.conn, &id_b, "login", false, None);

        svc.create_tag("work").unwrap();
        svc.create_tag("personal").unwrap();
        svc.add_tag_to_record(id_a.parse().unwrap(), 1).unwrap(); // tag "work"
        svc.add_tag_to_record(id_b.parse().unwrap(), 2).unwrap(); // tag "personal"

        let sort = RecordSort {
            field: SortField::UpdatedAt,
            direction: SortDirection::Desc,
        };

        let work_records = svc
            .list_records(&RecordFilter::Tag("work".into()), &sort)
            .unwrap();
        assert_eq!(work_records.len(), 1);
        assert_eq!(work_records[0].id, id_a.parse::<Uuid>().unwrap());

        let personal_records = svc
            .list_records(&RecordFilter::Tag("personal".into()), &sort)
            .unwrap();
        assert_eq!(personal_records.len(), 1);
        assert_eq!(personal_records[0].id, id_b.parse::<Uuid>().unwrap());
    }

    /// Expired filter should use parameterized query and return only expired records.
    #[test]
    fn expired_filter_returns_only_expired() {
        let svc = setup_service();

        let now = Utc::now().timestamp();

        // Record expired 1000 seconds ago
        let id_expired = Uuid::new_v4().to_string();
        insert_record(&svc.conn, &id_expired, "login", false, Some(now - 1000));

        // Record expires 1000 seconds in the future
        let id_valid = Uuid::new_v4().to_string();
        insert_record(&svc.conn, &id_valid, "login", false, Some(now + 1000));

        // Record with no expiration
        let id_no_exp = Uuid::new_v4().to_string();
        insert_record(&svc.conn, &id_no_exp, "login", false, None);

        let sort = RecordSort {
            field: SortField::UpdatedAt,
            direction: SortDirection::Desc,
        };

        let expired = svc.list_records(&RecordFilter::Expired, &sort).unwrap();
        assert_eq!(
            expired.len(),
            1,
            "only the expired record should be returned"
        );
        assert_eq!(expired[0].id, id_expired.parse::<Uuid>().unwrap());
    }

    /// Tag filter with a tag name containing single quotes must be safe.
    #[test]
    fn sql_injection_tag_with_single_quotes() {
        let mut svc = setup_service();

        let record_id = Uuid::new_v4().to_string();
        insert_record(&svc.conn, &record_id, "login", false, None);

        // Create a tag with single quotes in the name
        svc.create_tag("it's a tag").unwrap();
        svc.add_tag_to_record(record_id.parse().unwrap(), 1)
            .unwrap();

        let sort = RecordSort {
            field: SortField::UpdatedAt,
            direction: SortDirection::Desc,
        };

        // Filtering by the same tag should return the record
        let result = svc
            .list_records(&RecordFilter::Tag("it's a tag".into()), &sort)
            .unwrap();
        assert_eq!(
            result.len(),
            1,
            "tag with single quotes should match correctly"
        );

        // Filtering by a different tag should return nothing
        let result2 = svc
            .list_records(&RecordFilter::Tag("it''s a tag".into()), &sort)
            .unwrap();
        assert!(result2.is_empty(), "different tag string should not match");
    }
}
