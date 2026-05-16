// Tag management (list_tags, rename_tag, delete_tag, batch_add, batch_remove)

use crate::services::vault::VaultServiceImpl;
use rusqlite::Connection;
use uuid::Uuid;

use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::types::tag::{Tag, TagSortMeta};

use crate::services::vault::record::db_error_to_vault;

impl VaultServiceImpl {
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

    /// List all tags with usage statistics for sorting.
    ///
    /// Returns tags with [`TagSortMeta`] containing:
    /// - `record_count`: Number of non-deleted records with this tag
    /// - `last_used_at`: Unix timestamp of the most recently updated record (0 if none)
    ///
    /// Results are ordered alphabetically by tag name.
    pub fn list_tags_with_stats(&self) -> Result<Vec<(Tag, TagSortMeta)>, VaultError> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.name, COUNT(r.id) as record_count,
                    COALESCE(MAX(r.updated_at), 0) as last_used_at
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
                TagSortMeta {
                    record_count: row.get(2)?,
                    last_used_at: row.get(3)?,
                },
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

    /// Add a tag to multiple records in a single transaction.
    ///
    /// Creates the tag if it does not already exist. For each record, the tag is
    /// attached with `INSERT OR IGNORE` so already-tagged records are skipped
    /// without error.
    ///
    /// Returns the number of **new** associations actually created.
    pub fn batch_add_tag(
        &mut self,
        record_ids: &[Uuid],
        tag_name: &str,
    ) -> Result<usize, VaultError> {
        let tx = self.conn.unchecked_transaction()?;

        let tag = queries::get_or_create_tag(&tx, tag_name).map_err(db_error_to_vault)?;

        let mut added = 0usize;
        for record_id in record_ids {
            let rows_before = count_record_tags(&tx, record_id)?;
            queries::attach_tag(&tx, record_id, tag.id).map_err(db_error_to_vault)?;
            let rows_after = count_record_tags(&tx, record_id)?;
            if rows_after > rows_before {
                added += 1;
            }
        }

        tx.commit()?;
        Ok(added)
    }

    /// Remove a tag from multiple records in a single transaction.
    ///
    /// Returns the number of associations actually removed. After removal, if
    /// the tag is no longer associated with **any** record, it is automatically
    /// deleted from the `tags` table.
    ///
    /// Returns `VaultError::TagNotFound` if no tag with the given name exists.
    pub fn batch_remove_tag(
        &mut self,
        record_ids: &[Uuid],
        tag_name: &str,
    ) -> Result<usize, VaultError> {
        let tx = self.conn.unchecked_transaction()?;

        let tag = queries::get_tag_by_name(&tx, tag_name)
            .map_err(db_error_to_vault)?
            .ok_or_else(|| VaultError::TagNotFound(tag_name.to_string()))?;

        let mut removed = 0usize;
        for record_id in record_ids {
            let rows_before = count_record_tags(&tx, record_id)?;
            queries::detach_tag(&tx, record_id, tag.id).map_err(db_error_to_vault)?;
            let rows_after = count_record_tags(&tx, record_id)?;
            if rows_before > rows_after {
                removed += 1;
            }
        }

        // Check if the tag still has any associations; delete if orphaned.
        let remaining: i64 = tx.query_row(
            "SELECT COUNT(*) FROM record_tags WHERE tag_id = ?1",
            rusqlite::params![tag.id],
            |row| row.get(0),
        )?;
        if remaining == 0 {
            tx.execute("DELETE FROM tags WHERE id = ?1", rusqlite::params![tag.id])?;
        }

        tx.commit()?;
        Ok(removed)
    }
}

/// Count the number of tag associations for a given record.
fn count_record_tags(conn: &Connection, record_id: &Uuid) -> Result<usize, VaultError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM record_tags WHERE record_id = ?1",
            rusqlite::params![record_id.to_string()],
            |row| row.get(0),
        )
        .map_err(VaultError::DatabaseError)?;
    Ok(count as usize)
}
