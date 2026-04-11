// Password history (get_password_history, decrypt_history_password, save_conflict_history)

use chrono::Utc;
use uuid::Uuid;

use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::services::vault::record::db_error_to_vault;

use super::VaultService;

impl VaultService {
    /// Save the old encrypted payload to password history.
    ///
    /// Stores the existing encrypted blob as-is, avoiding the need to decrypt
    /// and re-encrypt.  If the history for this record exceeds 10 entries,
    /// the oldest entry is pruned.
    pub(crate) fn _save_password_history(
        &mut self,
        record_id: Uuid,
        old_encrypted_data: &[u8],
        old_nonce: &[u8; 24],
        old_dek_version: u32,
    ) -> Result<(), VaultError> {
        // Enforce max 10 history entries
        let count =
            queries::count_password_history(&self.conn, &record_id).map_err(db_error_to_vault)?;
        if count >= 10 {
            queries::delete_oldest_password_history(&self.conn, &record_id)
                .map_err(db_error_to_vault)?;
        }

        let now_ts = Utc::now().timestamp();
        queries::insert_password_history(
            &self.conn,
            &record_id,
            old_encrypted_data,
            old_nonce,
            old_dek_version,
            now_ts,
        )
        .map_err(db_error_to_vault)?;

        Ok(())
    }
}
