use std::path::PathBuf;

use crate::commands::types::ImportSource;

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
    Scope,
    ExportPassword,
    ConfirmPassword,
    OutputPath,
}

/// Export scope selection index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportScopeOption {
    All,
    CurrentFilter,
    ByTag,
}

// ── Source metadata ─────────────────────────────────────────────────────────

pub const IMPORT_SOURCES: [(ImportSource, &str, bool, &str); 6] = [
    (
        ImportSource::KeePass,
        "KeePass (.kdbx)",
        true,
        "Password / URL / Notes",
    ),
    (
        ImportSource::OnePassword1pux,
        "1Password (.1pux)",
        false,
        "Password / TOTP  \u{26A0} Custom fields",
    ),
    (
        ImportSource::OnePasswordOpvault,
        "1Password (.opvault)",
        true,
        "Password / TOTP  \u{26A0} Custom fields",
    ),
    (
        ImportSource::Bitwarden,
        "Bitwarden (.json)",
        true,
        "Password / TOTP / URL  \u{2715} Attachments",
    ),
    (ImportSource::Csv, "CSV", false, "Column-mapped fields"),
    (
        ImportSource::OpenKeyringBackup,
        "OpenKeyring Backup (.okb)",
        false,
        "All data",
    ),
];

#[allow(dead_code)]
pub(super) fn source_display(source: ImportSource) -> &'static str {
    IMPORT_SOURCES
        .iter()
        .find(|(s, _, _, _)| *s == source)
        .map(|(_, name, _, _)| *name)
        .unwrap_or("Unknown")
}

pub fn source_needs_password(source: ImportSource) -> bool {
    IMPORT_SOURCES
        .iter()
        .find(|(s, _, _, _)| *s == source)
        .map(|(_, _, pw, _)| *pw)
        .unwrap_or(false)
}

pub fn default_export_path() -> String {
    dirs::document_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("keyring-backup.okb")
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
