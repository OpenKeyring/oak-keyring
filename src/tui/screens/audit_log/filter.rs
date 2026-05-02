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

pub(super) fn operation_display_name(op: &AuditOperation) -> &'static str {
    match op {
        AuditOperation::RecordCreate => "添加密码",
        AuditOperation::RecordUpdate => "修改密码",
        AuditOperation::RecordDelete => "删除密码",
        AuditOperation::RecordRestore => "恢复密码",
        AuditOperation::RecordDestroy => "永久删除",
        AuditOperation::RecordViewPassword => "查看密码",
        AuditOperation::RecordCopyPassword => "复制密码",
        AuditOperation::RecordCopyField => "复制字段",
        AuditOperation::VaultUnlock => "解锁",
        AuditOperation::VaultLock => "锁定",
        AuditOperation::VaultExport => "导出",
        AuditOperation::VaultImport => "导入",
        AuditOperation::MasterPasswordChange => "改密",
        AuditOperation::TrashEmpty => "清空回收站",
        AuditOperation::SyncConflictResolved => "解决冲突",
        AuditOperation::SyncBatchConflictsResolved => "批量解决冲突",
        AuditOperation::DekRotated => "密钥轮换",
        AuditOperation::DekRotationFailed => "轮换失败",
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

pub(super) fn time_range_display(tr: &AuditTimeRange) -> &'static str {
    match tr {
        AuditTimeRange::Today => "今天",
        AuditTimeRange::LastWeek => "最近一周",
        AuditTimeRange::LastMonth => "最近一月",
        AuditTimeRange::LastYear => "最近一年",
        AuditTimeRange::All => "全部时间",
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
