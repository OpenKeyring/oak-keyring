// Record listing, filtering, and sorting

use crate::commands::types::{RecordFilter, RecordSort, SortField};
use crate::crypto::payload;
use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::services::vault::search;
use crate::services::vault::VaultServiceImpl;
use crate::types::record::{StoredRecord, TuiRecord};
use crate::types::sync::SyncStatus;

use super::helpers::{apply_sort, db_error_to_vault};

impl VaultServiceImpl {
    /// List records matching a filter, with decryption and sorting.
    ///
    /// Queries encrypted records from the database, decrypts name and subtitle
    /// for each record, and applies the requested sort order.
    ///
    /// # Filter behavior
    /// - `All` — all active (non-deleted) records
    /// - `Favorites` — active records where `is_favorite = true`
    /// - `Expired` — returns all active records; executor filters using health_report
    /// - `Trash` — soft-deleted records
    /// - `Tag(name)` — active records with the specified tag
    /// - `Search(query)` — delegates to search module for filtering
    /// - `HealthIssues` — returns all active records; executor filters using health_report
    ///
    /// # Sort behavior
    /// - `Name` — sorted at application layer after decryption
    /// - `CreatedAt` / `UpdatedAt` — sorted at application layer on timestamps
    /// - `UsageFrequency` — sorted by audit_log access count (descending by default)
    ///
    /// # Errors
    /// - `VaultError::NotUnlocked` if the vault is locked.
    /// - `VaultError::DatabaseError` if a query fails.
    /// - `VaultError::CryptoError` if decryption fails.
    pub fn list_records(
        &self,
        filter: &RecordFilter,
        sort: &RecordSort,
    ) -> Result<Vec<TuiRecord>, VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        // Fetch raw records based on filter
        let stored_records = match filter {
            RecordFilter::Trash => {
                queries::list_deleted_records(&self.conn).map_err(db_error_to_vault)?
            }
            RecordFilter::Search(query) => {
                // Fetch all active records, decrypt names/subtitles, then apply search filter
                let stored_records =
                    queries::list_active_records(&self.conn).map_err(db_error_to_vault)?;

                let sync_map = self.load_sync_status_map();
                let mut tui_records: Vec<TuiRecord> = Vec::with_capacity(stored_records.len());
                for stored in &stored_records {
                    let aad = format!("record:{}", stored.id);
                    let name = payload::decrypt_name_only(
                        &self.crypto,
                        &stored.encrypted_data,
                        &stored.nonce,
                        aad.as_bytes(),
                        stored.dek_version,
                    )
                    .map_err(VaultError::CryptoError)?;

                    let subtitle = payload::decrypt_subtitle(
                        &self.crypto,
                        &stored.encrypted_data,
                        &stored.nonce,
                        aad.as_bytes(),
                        stored.credential_type,
                        stored.dek_version,
                    )
                    .map_err(VaultError::CryptoError)?;

                    tui_records.push(TuiRecord {
                        id: stored.id,
                        credential_type: stored.credential_type,
                        name,
                        subtitle,
                        is_favorite: stored.is_favorite,
                        is_expired: false, // populated by executor from health_report
                        expires_at: stored.expires_at,
                        has_weak_password: false, // populated by executor from health_report
                        is_compromised: false,    // populated by executor from health_report
                        duplicate_group_size: None, // populated by executor from health_report
                        created_at: stored.created_at,
                        updated_at: stored.updated_at,
                        deleted: stored.deleted,
                        deleted_at: stored.deleted_at,
                        tags: stored.tags.clone(),
                        sync_status: sync_map.get(&stored.id.to_string()).copied(),
                    });
                }

                let mut filtered = search::search_records(&tui_records, query);
                let freq_map = if matches!(sort.field, SortField::UsageFrequency) {
                    let ids: Vec<String> = filtered.iter().map(|r| r.id.to_string()).collect();
                    Some(self.get_access_frequencies(&ids))
                } else {
                    None
                };
                apply_sort(&mut filtered, sort, freq_map.as_ref());
                return Ok(filtered);
            }
            _ => {
                // All, Favorites, Expired, HealthIssues, Tag — start from active records.
                // HealthIssues returns all active records here; the executor enriches
                // them with health data and filters at its level.
                queries::list_active_records(&self.conn).map_err(db_error_to_vault)?
            }
        };

        let sync_map = self.load_sync_status_map();

        // Decrypt and build TuiRecords, applying application-layer filters
        let mut tui_records: Vec<TuiRecord> = Vec::with_capacity(stored_records.len());
        for stored in &stored_records {
            // Application-layer filter for Favorites
            if matches!(filter, RecordFilter::Favorites) && !stored.is_favorite {
                continue;
            }

            // Expired filtering is handled by the executor using the health report,
            // not by DB-level expires_at < now. See spec §11.2.

            // Application-layer filter for Tag
            if let RecordFilter::Tag(tag_name) = filter {
                if !stored.tags.contains(tag_name) {
                    continue;
                }
            }

            // Decrypt name and subtitle
            let aad = format!("record:{}", stored.id);
            let name = payload::decrypt_name_only(
                &self.crypto,
                &stored.encrypted_data,
                &stored.nonce,
                aad.as_bytes(),
                stored.dek_version,
            )
            .map_err(VaultError::CryptoError)?;

            let subtitle = payload::decrypt_subtitle(
                &self.crypto,
                &stored.encrypted_data,
                &stored.nonce,
                aad.as_bytes(),
                stored.credential_type,
                stored.dek_version,
            )
            .map_err(VaultError::CryptoError)?;

            // is_expired defaults to false; the executor overrides it from the
            // health report. We do NOT compute it from expires_at < now here.
            tui_records.push(TuiRecord {
                id: stored.id,
                credential_type: stored.credential_type,
                name,
                subtitle,
                is_favorite: stored.is_favorite,
                is_expired: false,
                expires_at: stored.expires_at,
                has_weak_password: false,
                is_compromised: false,
                duplicate_group_size: None,
                created_at: stored.created_at,
                updated_at: stored.updated_at,
                deleted: stored.deleted,
                deleted_at: stored.deleted_at,
                tags: stored.tags.clone(),
                sync_status: sync_map.get(&stored.id.to_string()).copied(),
            });
        }

        // Application-layer sort
        let freq_map = if matches!(sort.field, SortField::UsageFrequency) {
            let ids: Vec<String> = tui_records.iter().map(|r| r.id.to_string()).collect();
            Some(self.get_access_frequencies(&ids))
        } else {
            None
        };
        apply_sort(&mut tui_records, sort, freq_map.as_ref());

        Ok(tui_records)
    }

    /// List all active (non-deleted) stored records.
    ///
    /// Returns raw `StoredRecord` without decryption. Used by health check
    /// which needs the full encrypted record for batch analysis.
    pub fn list_all_stored_records(&self) -> Result<Vec<StoredRecord>, VaultError> {
        queries::list_active_records(&self.conn).map_err(db_error_to_vault)
    }

    /// Decrypt the name field from a `StoredRecord`.
    ///
    /// Used by the sync upload path to produce a valid `CloudRecord.metadata.name`
    /// without decrypting the full payload.
    pub fn decrypt_record_name_for_sync(
        &self,
        stored: &crate::types::record::StoredRecord,
    ) -> Result<String, VaultError> {
        let aad = format!("record:{}", stored.id);
        crate::crypto::payload::decrypt_name_only(
            &self.crypto,
            &stored.encrypted_data,
            &stored.nonce,
            aad.as_bytes(),
            stored.dek_version,
        )
        .map_err(VaultError::CryptoError)
    }

    /// Load all sync statuses from the `sync_state` table in a single query.
    ///
    /// Returns a map from record ID (as hyphenated string) to its `SyncStatus`.
    /// If the `sync_state` table is empty (sync never used), the map will be empty,
    /// which is the correct default — callers get `None` for all records.
    pub fn load_sync_status_map(&self) -> std::collections::HashMap<String, SyncStatus> {
        crate::db::queries::load_sync_status_map(&self.conn)
    }

    /// Query the `audit_log` table for access counts per record.
    ///
    /// Returns a map from record ID (hyphenated string) to the number of
    /// audit entries referencing that record. Records with no audit entries
    /// are absent from the map (callers treat missing as 0).
    fn get_access_frequencies(
        &self,
        record_ids: &[String],
    ) -> std::collections::HashMap<String, i64> {
        let mut map = std::collections::HashMap::with_capacity(record_ids.len());
        for id in record_ids {
            let count: i64 = self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE record_id = ?1",
                    rusqlite::params![id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            map.insert(id.clone(), count);
        }
        map
    }
}
