mod audit;
mod history;
mod metadata;
mod record;
mod search;
mod tag;
mod trash;

use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

use crate::commands::types::{RecordFilter, RecordSort, SortDirection, SortField};
use crate::types::credential::CredentialType;
use crate::types::record::TuiRecord;

pub struct VaultService {
    conn: Connection,
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
        Self { conn, device_id }
    }

    pub fn create_record(
        &mut self,
        credential_type: CredentialType,
        encrypted_data: Vec<u8>,
        nonce: [u8; 24],
        dek_version: u32,
        aad: Option<Vec<u8>>,
    ) -> Result<Uuid, String> {
        let now = Utc::now().timestamp();
        let id = Uuid::new_v4();

        self.conn.execute(
            "INSERT INTO records (id, credential_type, encrypted_data, nonce, dek_version, aad, is_favorite, created_at, updated_at, updated_by, version, deleted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8, ?9, 1, 0)",
            rusqlite::params![
                id.to_string(),
                credential_type.to_db_str(),
                encrypted_data,
                nonce.to_vec(),
                dek_version,
                aad,
                now,
                now,
                self.device_id,
            ],
        ).map_err(|e| e.to_string())?;

        Ok(id)
    }

    pub fn list_records(
        &self,
        filter: &RecordFilter,
        sort: &RecordSort,
    ) -> Result<Vec<TuiRecord>, String> {
        let (mut query, _is_search) = match filter {
            RecordFilter::All => {
                ("SELECT r.id, r.credential_type, r.is_favorite, r.expires_at, r.created_at, r.updated_at, r.version, r.deleted FROM records r WHERE r.deleted = 0".to_string(), false)
            }
            RecordFilter::Favorites => {
                ("SELECT r.id, r.credential_type, r.is_favorite, r.expires_at, r.created_at, r.updated_at, r.version, r.deleted FROM records r WHERE r.deleted = 0 AND r.is_favorite = 1".to_string(), false)
            }
            RecordFilter::Expired => {
                let now = Utc::now().timestamp();
                (format!("SELECT r.id, r.credential_type, r.is_favorite, r.expires_at, r.created_at, r.updated_at, r.version, r.deleted FROM records r WHERE r.deleted = 0 AND r.expires_at IS NOT NULL AND r.expires_at < {}", now), false)
            }
            RecordFilter::Tag(tag) => {
                let escaped = tag.replace('\'', "''");
                (format!("SELECT r.id, r.credential_type, r.is_favorite, r.expires_at, r.created_at, r.updated_at, r.version, r.deleted FROM records r WHERE r.deleted = 0 AND r.id IN (SELECT rt.record_id FROM record_tags rt JOIN tags t ON rt.tag_id = t.id WHERE t.name = '{}')", escaped), false)
            }
            RecordFilter::Search(_term) => {
                ("SELECT r.id, r.credential_type, r.is_favorite, r.expires_at, r.created_at, r.updated_at, r.version, r.deleted FROM records r WHERE r.deleted = 0".to_string(), true)
            }
            _ => {
                ("SELECT r.id, r.credential_type, r.is_favorite, r.expires_at, r.created_at, r.updated_at, r.version, r.deleted FROM records r WHERE r.deleted = 0".to_string(), false)
            }
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

        let mut stmt = self.conn.prepare(&query).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
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
            })
            .map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn soft_delete(&mut self, id: Uuid) -> Result<(), String> {
        let now = Utc::now().timestamp();
        self.conn.execute(
            "UPDATE records SET deleted = 1, deleted_at = ?1, updated_at = ?2, version = version + 1 WHERE id = ?3 AND deleted = 0",
            rusqlite::params![now, now, id.to_string()],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn restore(&mut self, id: Uuid) -> Result<(), String> {
        let now = Utc::now().timestamp();
        self.conn.execute(
            "UPDATE records SET deleted = 0, deleted_at = NULL, updated_at = ?1, version = version + 1 WHERE id = ?2 AND deleted = 1",
            rusqlite::params![now, id.to_string()],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn write_audit_entry(
        &mut self,
        operation: crate::types::AuditOperation,
        record_id: Option<Uuid>,
        record_name: Option<String>,
        detail: Option<String>,
    ) -> Result<(), String> {
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
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn create_tag(&mut self, name: &str) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO tags (name) VALUES (?1)",
                rusqlite::params![name],
            )
            .map_err(|e| e.to_string())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn add_tag_to_record(&mut self, record_id: Uuid, tag_id: i64) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO record_tags (record_id, tag_id) VALUES (?1, ?2)",
                rusqlite::params![record_id.to_string(), tag_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_metadata(&self, key: &str) -> Result<Option<String>, String> {
        let result = self.conn.query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            rusqlite::params![key],
            |r| r.get::<_, String>(0),
        );
        match result {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn set_metadata(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO metadata (key, value) VALUES (?1, ?2)",
                rusqlite::params![key, value],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
