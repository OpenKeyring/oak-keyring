use crate::commands::{Command, CommandResult, Message};
use crate::errors::ErrorCode;

use super::{
    clipboard, config, health, import_export, record, rotation, sync, vault, CommandExecutor,
};

impl CommandExecutor {
    /// Execute a single command.
    ///
    /// Performs pre-check validation, dispatches to the appropriate handler,
    /// runs post-hook logging, and sends the result back to the UI layer.
    pub async fn execute(&mut self, command: Command) {
        // Step 1: Pre-check
        if let Some(error_result) = self.pre_check(&command) {
            let _ = self
                .result_tx
                .send(Message::CommandCompleted(error_result))
                .await;
            return;
        }

        // Step 2: Dispatch
        let result = self.dispatch(command).await;

        // Step 3: Post-hook
        self.post_hook(&result);

        // Step 4: Send result
        if self
            .result_tx
            .send(Message::CommandCompleted(result))
            .await
            .is_err()
        {
            tracing::error!("Failed to send command result: channel closed");
        }
    }

    /// Pre-check: validate vault lock state before dispatching.
    ///
    /// Commands that require an unlocked vault will return an error
    /// if the vault is currently locked. Certain commands (unlock,
    /// recovery key unlock, initialize vault, load config) are exempt.
    fn pre_check(&self, command: &Command) -> Option<CommandResult> {
        let needs_unlock = !matches!(
            command,
            Command::UnlockVault { .. }
                | Command::UnlockWithRecoveryKey { .. }
                | Command::InitializeVault { .. }
                | Command::LoadConfig
                | Command::ValidateRecoveryWords { .. }
                | Command::RebuildKeyFileFromRecovery { .. }
                | Command::RestoreDatabaseFromOkb { .. }
                | Command::RestoreDatabaseFromCloud
                | Command::ValidateRestoredDatabase
        );

        if needs_unlock && !self.vault.is_unlocked() {
            return Some(CommandResult::Error {
                code: ErrorCode::ExecutorVaultLocked,
                context: crate::errors::ErrorContext::default(),
                message_key: "error.vault_locked",
                fallback: String::from("Vault is locked. Please unlock first."),
            });
        }
        None
    }

    /// Post-hook: log errors for observability.
    ///
    /// Records command execution failures as warnings in the structured log.
    pub(super) fn post_hook(&mut self, result: &CommandResult) {
        if let CommandResult::Error { code, fallback, .. } = result {
            tracing::warn!(error_code = ?code, message = %fallback, "Command execution failed");
        }

        // Spec S5: Update cached health report on completion
        if let CommandResult::HealthCheckCompleted { report } = result {
            self.health_report = Some(report.clone());
            self.last_health_check_time = Some(chrono::Utc::now());
        }
    }

    /// Dispatch: exhaustive match on all Command variants.
    ///
    /// Routes each command to its handler module. The match is exhaustive
    /// so adding a new Command variant without a handler arm will cause
    /// a compile error.
    #[tracing::instrument(skip_all)]
    async fn dispatch(&mut self, command: Command) -> CommandResult {
        match command {
            // ── Vault Operations ──────────────────────────
            Command::UnlockVault { master_password } => {
                vault::handle_unlock(self, master_password).await
            }
            Command::UnlockWithRecoveryKey { words } => {
                vault::handle_unlock_with_recovery_key(self, words).await
            }
            Command::LockVault => {
                // Security: Cancel all in-flight operations holding decrypted data.
                // Does NOT cancel shutdown_token — the executor loop stays alive.
                self.operation_cancel_token.cancel();
                // Replace with a fresh child token for the next unlock session.
                self.operation_cancel_token = self.shutdown_token.child_token();
                vault::handle_lock(self)
            }
            Command::VerifyMasterPassword { password } => {
                vault::handle_verify_master_password(self, password)
            }
            Command::ChangeMasterPassword {
                current_password,
                new_password,
            } => vault::handle_change_master_password(self, current_password, new_password),
            Command::ClearVerifiedPassword => {
                self.verified_master_password = None;
                CommandResult::Void
            }
            Command::InitializeVault {
                master_password,
                recovery_words,
            } => vault::handle_initialize_vault(self, master_password, recovery_words).await,

            // ── Record CRUD ──────────────────────────────
            Command::CreateRecord {
                credential_type,
                payload,
                tags,
                is_favorite,
                expires_at,
            } => record::handle_create_record(
                self,
                credential_type,
                payload,
                tags,
                is_favorite,
                expires_at,
            ),
            Command::UpdateRecord {
                id,
                payload,
                tags,
                is_favorite,
                expires_at,
                expected_version,
            } => record::handle_update_record(
                self,
                id,
                payload,
                tags,
                is_favorite,
                expires_at,
                expected_version,
            ),
            Command::SoftDeleteRecord { id } => record::handle_soft_delete_record(self, id),
            Command::RestoreRecord { id } => record::handle_restore_record(self, id),
            Command::HardDeleteRecord { id } => record::handle_hard_delete_record(self, id),
            Command::ToggleFavorite { id, is_favorite } => {
                record::handle_toggle_favorite(self, id, is_favorite)
            }

            // ── Record Query ──────────────────────────────
            Command::LoadRecordList { filter, sort } => {
                record::handle_load_record_list(self, filter, sort)
            }
            Command::LoadRecordDetail { id } => record::handle_load_record_detail(self, id),
            Command::LoadRecordForEdit { id } => record::handle_load_record_for_edit(self, id),
            Command::DecryptField { id, field } => record::handle_decrypt_field(self, id, field),

            // ── Clipboard Operations ────────────────────
            Command::CopyToClipboard { id, field } => {
                clipboard::handle_copy_to_clipboard(self, id, field).await
            }
            Command::CopyRawToClipboard { value } => {
                clipboard::handle_copy_raw_to_clipboard(self, value).await
            }

            // ── Password History ──────────────────────────
            Command::LoadPasswordHistory { record_id } => {
                record::handle_load_password_history(self, record_id)
            }
            Command::CopyHistoryPassword { history_id } => {
                clipboard::handle_copy_history_password(self, history_id).await
            }

            // ── Tag Operations ────────────────────────────
            Command::LoadTags => record::handle_load_tags(self),
            Command::RenameTag { old_name, new_name } => {
                record::handle_rename_tag(self, old_name, new_name)
            }
            Command::DeleteTag { name } => record::handle_delete_tag(self, name),
            Command::BatchAddTag {
                record_ids,
                tag_name,
            } => record::handle_batch_add_tag(self, record_ids, tag_name),
            Command::BatchRemoveTag {
                record_ids,
                tag_name,
            } => record::handle_batch_remove_tag(self, record_ids, tag_name),

            // ── Batch Operations ──────────────────────────
            Command::BatchSoftDelete { record_ids } => {
                record::handle_batch_soft_delete(self, record_ids)
            }
            Command::EmptyTrash => record::handle_empty_trash(self),

            // ── Password Generation ───────────────────────
            Command::GeneratePassword {
                length,
                include_digits,
                include_uppercase,
                include_special,
            } => record::handle_generate_password(
                self,
                length,
                include_digits,
                include_uppercase,
                include_special,
            ),
            Command::GenerateMemorablePassword { word_count } => {
                record::handle_generate_memorable_password(self, word_count)
            }
            Command::GeneratePin { length } => record::handle_generate_pin(self, length),

            // ── Health Check ──────────────────────────────
            Command::RunHealthCheck { force } => health::handle_run_health_check(self, force),
            Command::CheckHibp { record_id } => health::handle_check_hibp(self, record_id).await,

            // ── Sync Operations ──────────────────────────
            Command::TriggerSync => sync::handle_trigger_sync(self).await,
            Command::ResolveConflict {
                record_id,
                resolution,
            } => sync::handle_resolve_conflict(self, record_id, resolution).await,
            Command::ResolveAllConflicts { resolution } => {
                sync::handle_resolve_all_conflicts(self, resolution).await
            }

            // ── Import/Export ─────────────────────────────
            Command::ValidateImportFile {
                source,
                path,
                password,
            } => import_export::handle_validate_import_file(self, source, path, password),
            Command::ExecuteImport {
                session_id,
                source,
                path,
                password,
                column_mapping,
                import_as_notes,
            } => import_export::handle_execute_import(
                self,
                session_id,
                source,
                path,
                password,
                column_mapping,
                import_as_notes,
            ),
            Command::ExecuteExport {
                scope,
                output_path,
                export_password,
                master_password,
                format,
            } => import_export::handle_execute_export(
                self,
                scope,
                output_path,
                export_password,
                master_password,
                format,
            ),

            // ── Config Operations ─────────────────────────
            Command::LoadConfig => config::handle_load_config(self),
            Command::SaveConfig { config } => config::handle_save_config(self, config),
            Command::TestSyncConnection { provider_config } => {
                config::handle_test_sync_connection(self, provider_config).await
            }

            Command::OAuth2AuthorizeGoogleDrive => {
                config::handle_oauth2_authorize_google_drive(self).await
            }

            // ── Audit Log ─────────────────────────────────
            Command::LoadAuditLog { filter } => config::handle_load_audit_log(self, filter),
            Command::NavigateToRecord { record_id } => {
                record::handle_load_record_detail(self, record_id)
            }

            // ── DEK Rotation ─────────────────────────────
            Command::TriggerRotation => rotation::handle_trigger_rotation(self).await,
            Command::CheckRotationTrigger => rotation::handle_check_rotation_trigger(self),

            // ── Partial Vault Recovery ─────────────────────
            Command::ValidateRecoveryWords { words } => {
                vault::handle_validate_recovery_words(words).await
            }
            Command::RebuildKeyFileFromRecovery {
                master_password,
                recovery_words,
            } => {
                vault::handle_rebuild_keyfile_from_recovery(self, master_password, recovery_words)
                    .await
            }
            Command::RestoreDatabaseFromOkb { path, password } => {
                import_export::handle_restore_database_from_okb(self, path, password).await
            }
            Command::RestoreDatabaseFromCloud => {
                sync::handle_restore_database_from_cloud(self).await
            }
            Command::ValidateRestoredDatabase => {
                vault::handle_validate_restored_database(self).await
            }
        }
    }
}

/// Handle the internal `HealthCheckCompleted` signal from a background task.
///
/// Persists the health report to DB, marks changed records for sync, and
/// updates cached metadata. Returns a `CommandResult` for the UI.
pub fn handle_internal_health_check_completed(
    executor: &mut CommandExecutor,
    report: crate::commands::types::HealthReport,
) -> CommandResult {
    let evaluated_at = chrono::Utc::now();

    match health::persist_health_report(executor, &report, evaluated_at) {
        Ok(deltas) => {
            if let Err(e) = health::schedule_health_resync_for_records(executor, &deltas) {
                tracing::warn!(
                    error = %e,
                    "Failed to mark health-changed records for sync"
                );
            }

            if let Err(e) = executor.vault.set_last_health_check_at(evaluated_at) {
                tracing::warn!(
                    error = %e,
                    "Failed to persist last_health_check_at"
                );
            }
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "Failed to persist health report — caching in-memory only"
            );
        }
    }

    // Update cached health report on the executor.
    executor.health_report = Some(report.clone());
    executor.last_health_check_time = Some(chrono::Utc::now());

    CommandResult::HealthCheckCompleted { report }
}
