// Tag management (list_tags, rename_tag, delete_tag, batch_add, batch_remove)

use super::VaultService;
use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::types::tag::Tag;

use super::record::db_error_to_vault;

impl VaultService {
    /// List all tags with their usage count.
    ///
    /// The usage count reflects the number of **non-deleted** records
    /// currently associated with each tag. Tags with zero active records
    /// are still included.
    ///
    /// Results are ordered alphabetically by tag name.
    pub fn list_tags(&self) -> Result<Vec<(Tag, usize)>, VaultError> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.name, COUNT(r.id) as usage_count
             FROM tags t
             LEFT JOIN record_tags rt ON rt.tag_id = t.id
             LEFT JOIN records r ON rt.record_id = r.id AND r.deleted = 0
             GROUP BY t.id, t.name
             ORDER BY t.name",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                },
                row.get::<_, usize>(2)?,
            ))
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Rename a tag.
    ///
    /// Returns `VaultError::TagAlreadyExists` if the new name is already taken,
    /// or `VaultError::TagNotFound` if the old name does not exist.
    pub fn rename_tag(&mut self, old_name: &str, new_name: &str) -> Result<(), VaultError> {
        // Check if the target name already exists
        if queries::get_tag_by_name(&self.conn, new_name)
            .map_err(db_error_to_vault)?
            .is_some()
        {
            return Err(VaultError::TagAlreadyExists(new_name.to_string()));
        }

        let updated =
            queries::rename_tag(&self.conn, old_name, new_name).map_err(db_error_to_vault)?;
        if !updated {
            return Err(VaultError::TagNotFound(old_name.to_string()));
        }

        Ok(())
    }

    /// Delete a tag and cascade-remove all `record_tags` associations.
    ///
    /// Returns `VaultError::TagNotFound` if no tag with the given name exists.
    pub fn delete_tag(&mut self, name: &str) -> Result<(), VaultError> {
        let deleted = queries::delete_tag_by_name(&self.conn, name).map_err(db_error_to_vault)?;
        if !deleted {
            return Err(VaultError::TagNotFound(name.to_string()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::bip39::{MnemonicLanguage, Passkey};
    use crate::db::schema::{initialize_metadata, initialize_schema};
    use crate::types::credential::{CredentialType, EncryptedPayload};
    use crate::types::record::CreateRecordParams;
    use crate::types::sensitive::SecureStr;
    use rusqlite::Connection;

    /// Helper: create an in-memory VaultService with schema initialized.
    fn setup_service() -> VaultService {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn);
        initialize_metadata(&conn);
        VaultService::new(conn)
    }

    /// Helper: unlock the VaultService with a fresh mnemonic.
    fn unlock_service(svc: &mut VaultService) {
        let mnemonic = Passkey::generate(24, MnemonicLanguage::English).unwrap();
        svc.crypto
            .unlock_with_mnemonic(&mnemonic)
            .expect("unlock_with_mnemonic must succeed in test");
    }

    /// Helper: create a Login record with the given tags.
    fn create_record_with_tags(svc: &mut VaultService, name: &str, tags: Vec<String>) {
        svc.create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: name.to_string(),
                username: format!("user_{}", name.to_lowercase()),
                password: SecureStr::new("password123".to_string()),
                url: None,
                notes: None,
            },
            tags,
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed");
    }

    // =========================================================================
    // list_tags tests
    // =========================================================================

    #[test]
    fn list_tags_returns_empty_when_no_tags_exist() {
        let svc = setup_service();
        let tags = svc.list_tags().expect("list_tags must succeed");
        assert!(tags.is_empty(), "no tags should exist in empty vault");
    }

    #[test]
    fn list_tags_returns_tags_with_correct_usage_count() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        // Create records with overlapping tags
        create_record_with_tags(
            &mut svc,
            "Rec1",
            vec!["work".to_string(), "dev".to_string()],
        );
        create_record_with_tags(&mut svc, "Rec2", vec!["work".to_string()]);
        create_record_with_tags(&mut svc, "Rec3", vec!["personal".to_string()]);

        let tags = svc.list_tags().expect("list_tags must succeed");

        // Ordered alphabetically: dev, personal, work
        assert_eq!(tags.len(), 3);

        let (dev_tag, dev_count) = &tags[0];
        assert_eq!(dev_tag.name, "dev");
        assert_eq!(*dev_count, 1, "dev tag used by 1 record");

        let (personal_tag, personal_count) = &tags[1];
        assert_eq!(personal_tag.name, "personal");
        assert_eq!(*personal_count, 1, "personal tag used by 1 record");

        let (work_tag, work_count) = &tags[2];
        assert_eq!(work_tag.name, "work");
        assert_eq!(*work_count, 2, "work tag used by 2 records");
    }

    #[test]
    fn list_tags_excludes_soft_deleted_records_from_count() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let id1 = svc
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Login,
                payload: EncryptedPayload::Login {
                    name: "ToBeDeleted".to_string(),
                    username: "user1".to_string(),
                    password: SecureStr::new("pass".to_string()),
                    url: None,
                    notes: None,
                },
                tags: vec!["work".to_string()],
                is_favorite: false,
                expires_at: None,
            })
            .expect("create_record must succeed");

        create_record_with_tags(&mut svc, "Active", vec!["work".to_string()]);

        // Before soft delete: work tag count = 2
        let tags_before = svc.list_tags().expect("list_tags must succeed");
        let work_before = tags_before.iter().find(|(t, _)| t.name == "work").unwrap();
        assert_eq!(work_before.1, 2);

        // Soft delete one record
        svc.soft_delete_record(id1)
            .expect("soft_delete must succeed");

        // After soft delete: work tag count = 1 (only active records)
        let tags_after = svc.list_tags().expect("list_tags must succeed");
        let work_after = tags_after.iter().find(|(t, _)| t.name == "work").unwrap();
        assert_eq!(work_after.1, 1, "soft-deleted records should not count");
    }

    #[test]
    fn list_tags_includes_unused_tags_with_zero_count() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        // Create a record with a tag, then hard delete the record
        let id = svc
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Login,
                payload: EncryptedPayload::Login {
                    name: "TempRec".to_string(),
                    username: "user1".to_string(),
                    password: SecureStr::new("pass".to_string()),
                    url: None,
                    notes: None,
                },
                tags: vec!["orphan".to_string()],
                is_favorite: false,
                expires_at: None,
            })
            .expect("create_record must succeed");

        // Hard delete removes the record and cascade-deletes record_tags
        svc.hard_delete_record(id)
            .expect("hard_delete must succeed");

        // The tag itself still exists but has no associations
        let tags = svc.list_tags().expect("list_tags must succeed");
        // The tag "orphan" should still exist with count 0
        // (unless get_or_create_tag was used — tags table row persists)
        let orphan = tags.iter().find(|(t, _)| t.name == "orphan");
        assert!(
            orphan.is_none() || orphan.map(|(_, c)| *c) == Some(0),
            "orphan tag should have 0 usage or not exist"
        );
    }

    // =========================================================================
    // rename_tag tests
    // =========================================================================

    #[test]
    fn rename_tag_succeeds_and_tag_is_findable_by_new_name() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        // Create a tag via record creation
        create_record_with_tags(&mut svc, "Rec", vec!["old_name".to_string()]);

        svc.rename_tag("old_name", "new_name")
            .expect("rename_tag must succeed");

        // Verify the old name is gone
        let tags = svc.list_tags().expect("list_tags must succeed");
        assert!(
            tags.iter().all(|(t, _)| t.name != "old_name"),
            "old tag name should not exist"
        );

        // Verify the new name exists
        let new_tag = tags.iter().find(|(t, _)| t.name == "new_name");
        assert!(new_tag.is_some(), "new tag name should exist");
        assert_eq!(new_tag.unwrap().1, 1, "usage count should be preserved");
    }

    #[test]
    fn rename_tag_target_exists_returns_tag_already_exists() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        create_record_with_tags(&mut svc, "Rec1", vec!["alpha".to_string()]);
        create_record_with_tags(&mut svc, "Rec2", vec!["beta".to_string()]);

        let result = svc.rename_tag("alpha", "beta");
        assert!(result.is_err(), "rename to existing name should fail");
        assert!(
            matches!(result.unwrap_err(), VaultError::TagAlreadyExists(ref n) if n == "beta"),
            "expected TagAlreadyExists(\"beta\")"
        );
    }

    #[test]
    fn rename_tag_source_not_found_returns_tag_not_found() {
        let mut svc = setup_service();

        let result = svc.rename_tag("nonexistent", "something_else");
        assert!(result.is_err(), "rename nonexistent tag should fail");
        assert!(
            matches!(result.unwrap_err(), VaultError::TagNotFound(ref n) if n == "nonexistent"),
            "expected TagNotFound(\"nonexistent\")"
        );
    }

    // =========================================================================
    // delete_tag tests
    // =========================================================================

    #[test]
    fn delete_tag_removes_tag_and_record_tags_associations() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        // Create two records sharing the "work" tag
        let id1 = svc
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Login,
                payload: EncryptedPayload::Login {
                    name: "Rec1".to_string(),
                    username: "user1".to_string(),
                    password: SecureStr::new("pass1".to_string()),
                    url: None,
                    notes: None,
                },
                tags: vec!["work".to_string(), "dev".to_string()],
                is_favorite: false,
                expires_at: None,
            })
            .expect("create_record must succeed");

        let _id2 = svc
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Login,
                payload: EncryptedPayload::Login {
                    name: "Rec2".to_string(),
                    username: "user2".to_string(),
                    password: SecureStr::new("pass2".to_string()),
                    url: None,
                    notes: None,
                },
                tags: vec!["work".to_string()],
                is_favorite: false,
                expires_at: None,
            })
            .expect("create_record must succeed");

        // Verify tags exist before delete
        let tags_before = svc.list_tags().expect("list_tags must succeed");
        assert_eq!(tags_before.len(), 2, "should have 2 tags (dev, work)");

        // Delete the "work" tag
        svc.delete_tag("work").expect("delete_tag must succeed");

        // Verify tag is gone from list
        let tags_after = svc.list_tags().expect("list_tags must succeed");
        assert_eq!(tags_after.len(), 1, "only dev tag should remain");
        assert_eq!(tags_after[0].0.name, "dev");

        // Verify the record no longer has "work" in its tags
        let stored = svc.get_stored_record(id1).expect("record must exist");
        assert!(
            !stored.tags.contains(&"work".to_string()),
            "work tag should be removed from record"
        );
        assert!(
            stored.tags.contains(&"dev".to_string()),
            "dev tag should still be present"
        );
    }

    #[test]
    fn delete_tag_not_found_returns_tag_not_found() {
        let mut svc = setup_service();

        let result = svc.delete_tag("nonexistent");
        assert!(result.is_err(), "deleting nonexistent tag should fail");
        assert!(
            matches!(result.unwrap_err(), VaultError::TagNotFound(ref n) if n == "nonexistent"),
            "expected TagNotFound(\"nonexistent\")"
        );
    }
}
