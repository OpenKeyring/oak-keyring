use crate::commands::types::{ExportFormat, ImportSource};
use crate::t;

// ── Enums ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportExportMode {
    Import,
    Export,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportStep {
    SourceSelect,
    Preview,
    Importing,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportStep {
    Form,
    MasterPasswordConfirm,
    Exporting,
    Complete,
}

/// Focus targets within the Import SourceSelect step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFocus {
    SourceList,
    FilePath,
    Password,
    // CSV column mapping fields
    CsvName,
    CsvUsername,
    CsvPassword,
    CsvUrl,
    CsvNotes,
    CsvTags,
    CsvSkipHeader,
}

/// Focus targets within the Export Form step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFocus {
    ExportPassword,
    ConfirmPassword,
    OutputPath,
}

// ── Source metadata ─────────────────────────────────────────────────────────

pub fn import_sources() -> Vec<(ImportSource, String, bool, (String, ScopeHintStyle))> {
    vec![
        (
            ImportSource::KeePass,
            t!("tui.import_export.source_keepass").to_string(),
            true,
            (
                t!("tui.import_export.source_keepass_scope").to_string(),
                ScopeHintStyle::Full,
            ),
        ),
        (
            ImportSource::OnePassword1pux,
            t!("tui.import_export.source_1password_1pux").to_string(),
            false,
            (
                t!("tui.import_export.source_1password_scope").to_string(),
                ScopeHintStyle::Partial,
            ),
        ),
        (
            ImportSource::OnePasswordOpvault,
            t!("tui.import_export.source_1password_opvault").to_string(),
            true,
            (
                t!("tui.import_export.source_1password_scope").to_string(),
                ScopeHintStyle::Partial,
            ),
        ),
        (
            ImportSource::Bitwarden,
            t!("tui.import_export.source_bitwarden").to_string(),
            true,
            (
                t!("tui.import_export.source_bitwarden_scope").to_string(),
                ScopeHintStyle::Limited,
            ),
        ),
        (
            ImportSource::Csv,
            t!("tui.import_export.source_csv").to_string(),
            false,
            (
                t!("tui.import_export.source_csv_scope").to_string(),
                ScopeHintStyle::Full,
            ),
        ),
        (
            ImportSource::OpenKeyringBackup,
            t!("tui.import_export.source_okb").to_string(),
            false,
            (
                t!("tui.import_export.source_okb_scope").to_string(),
                ScopeHintStyle::Full,
            ),
        ),
    ]
}

/// Visual style category for a source's scope hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeHintStyle {
    /// All data types fully supported
    Full,
    /// Some data types have limitations
    Partial,
    /// Some data types are unsupported
    Limited,
}

#[allow(dead_code)]
pub(super) fn source_display(source: ImportSource) -> String {
    import_sources()
        .iter()
        .find(|(s, _, _, _)| *s == source)
        .map(|(_, name, _, _)| name.clone())
        .unwrap_or_else(|| t!("tui.import_export.source_unknown").to_string())
}

pub fn source_needs_password(source: ImportSource) -> bool {
    import_sources()
        .iter()
        .find(|(s, _, _, _)| *s == source)
        .map(|(_, _, pw, _)| *pw)
        .unwrap_or(false)
}

pub(super) fn default_export_path(format: ExportFormat) -> String {
    let ext = match format {
        ExportFormat::Okb => "okb",
        ExportFormat::Csv => "csv",
    };
    crate::paths::document_dir()
        .unwrap_or_else(crate::paths::data_dir_fallback)
        .join(format!("keyring-backup.{ext}"))
        .to_string_lossy()
        .to_string()
}

// ── Entry point ─────────────────────────────────────────────────────────────

/// Entry point for import screen — determines title and navigation context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImportEntryPoint {
    #[default]
    ConfigPage,
    Onboarding {
        step: usize,
    },
}
