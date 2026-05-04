use crate::commands::types::AuditTimeRange;
use crate::tui::state::audit_state::AuditFilter;
use crate::types::AuditOperation;

// ── Filter debounce ─────────────────────────────────────────────────────────

/// Number of 50 ms ticks before a pending search is flushed.
pub(super) const DEBOUNCE_TICKS: usize = 3;

#[derive(Debug, Default)]
pub(super) struct FilterState {
    pub(super) pending_search: Option<String>,
    pub(super) debounce_counter: Option<usize>,
}

impl FilterState {
    pub(super) fn on_search_input(&mut self, text: String) {
        self.pending_search = Some(text);
        self.debounce_counter = Some(DEBOUNCE_TICKS);
    }

    /// Tick the debounce counter. Returns a fully populated filter when the
    /// debounce window expires, so the caller can dispatch a reload command.
    pub(super) fn tick(&mut self, current_filter: &AuditFilter) -> Option<AuditFilter> {
        if let Some(ref mut counter) = self.debounce_counter {
            *counter = counter.saturating_sub(1);
            if *counter == 0 {
                self.debounce_counter = None;
                let search = self.pending_search.take().unwrap_or_default();
                return Some(AuditFilter {
                    search,
                    operation: current_filter.operation,
                    time_range: current_filter.time_range,
                });
            }
        }
        None
    }
}

// ── Helper functions ────────────────────────────────────────────────────────

pub(super) fn operation_display_name(op: &AuditOperation) -> String {
    match op {
        AuditOperation::RecordCreate => crate::t!("tui.audit.action_create").to_string(),
        AuditOperation::RecordUpdate => crate::t!("tui.audit.action_update").to_string(),
        AuditOperation::RecordDelete => crate::t!("tui.audit.action_delete").to_string(),
        AuditOperation::RecordRestore => crate::t!("tui.audit.action_restore").to_string(),
        AuditOperation::RecordDestroy => crate::t!("tui.audit.action_permanent_delete").to_string(),
        AuditOperation::RecordViewPassword => {
            crate::t!("tui.audit.action_view_password").to_string()
        }
        AuditOperation::RecordCopyPassword => crate::t!("tui.audit.action_copy").to_string(),
        AuditOperation::RecordCopyField => crate::t!("tui.audit.action_copy_field").to_string(),
        AuditOperation::VaultUnlock => crate::t!("tui.audit.action_unlock").to_string(),
        AuditOperation::VaultLock => crate::t!("tui.audit.action_lock").to_string(),
        AuditOperation::VaultExport => crate::t!("tui.audit.action_export").to_string(),
        AuditOperation::VaultImport => crate::t!("tui.audit.action_import").to_string(),
        AuditOperation::MasterPasswordChange => {
            crate::t!("tui.audit.action_change_password").to_string()
        }
        AuditOperation::TrashEmpty => crate::t!("tui.audit.action_empty_trash").to_string(),
        AuditOperation::SyncConflictResolved => {
            crate::t!("tui.audit.action_resolve_conflict").to_string()
        }
        AuditOperation::SyncBatchConflictsResolved => {
            crate::t!("tui.audit.action_batch_resolve_conflict").to_string()
        }
        AuditOperation::DekRotated => crate::t!("tui.audit.action_rotate_key").to_string(),
        AuditOperation::DekRotationFailed => {
            crate::t!("tui.audit.action_rotate_failed").to_string()
        }
    }
}

pub(super) fn operation_color(op: &AuditOperation) -> ratatui::style::Color {
    match op {
        AuditOperation::RecordCopyPassword
        | AuditOperation::RecordCopyField
        | AuditOperation::RecordViewPassword => ratatui::style::Color::Blue,
        AuditOperation::RecordCreate | AuditOperation::RecordRestore => {
            ratatui::style::Color::Green
        }
        AuditOperation::RecordUpdate => ratatui::style::Color::Yellow,
        AuditOperation::RecordDelete
        | AuditOperation::RecordDestroy
        | AuditOperation::TrashEmpty => ratatui::style::Color::Red,
        _ => ratatui::style::Color::DarkGray,
    }
}

pub(super) fn time_range_display(tr: &AuditTimeRange) -> String {
    match tr {
        AuditTimeRange::Today => crate::t!("tui.audit.filter_today").to_string(),
        AuditTimeRange::LastWeek => crate::t!("tui.audit.filter_last_7d").to_string(),
        AuditTimeRange::LastMonth => crate::t!("tui.audit.filter_last_30d").to_string(),
        AuditTimeRange::LastYear => crate::t!("tui.audit.filter_last_1y").to_string(),
        AuditTimeRange::All => crate::t!("tui.audit.filter_all_time").to_string(),
    }
}

pub(super) const TIME_RANGES: [AuditTimeRange; 5] = [
    AuditTimeRange::All,
    AuditTimeRange::Today,
    AuditTimeRange::LastWeek,
    AuditTimeRange::LastMonth,
    AuditTimeRange::LastYear,
];

#[cfg(test)]
pub(super) fn time_range_index(tr: Option<&AuditTimeRange>) -> usize {
    match tr {
        None | Some(AuditTimeRange::All) => 0,
        Some(AuditTimeRange::Today) => 1,
        Some(AuditTimeRange::LastWeek) => 2,
        Some(AuditTimeRange::LastMonth) => 3,
        Some(AuditTimeRange::LastYear) => 4,
    }
}

/// Format a `DateTime<Utc>` into a compact display string.
pub(super) fn format_timestamp(dt: &chrono::DateTime<chrono::Utc>) -> String {
    let local = dt.with_timezone(&chrono::Local);
    local.format("%m-%d %H:%M").to_string()
}
