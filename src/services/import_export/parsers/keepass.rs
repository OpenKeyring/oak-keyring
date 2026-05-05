//! KeePass .kdbx database import parser.
//!
//! Parses encrypted KeePass database files using the `keepass` crate,
//! extracting all entries with their fields, group paths as tags,
//! and custom fields appended to notes.

use std::collections::HashMap;
use std::path::Path;

use keepass::db::DatabaseOpenError;

use crate::commands::types::CsvColumnMapping;
use crate::commands::types::ImportSource;
use crate::errors::mapping::import_export::ImportExportError;
use crate::services::import_export::parser::{validate_file_common, FormatParser, ParsedItem};
use crate::types::SecureStr;

// Standard KeePass field names used for direct extraction.
pub(super) const KP_TITLE: &str = "Title";
pub(super) const KP_USERNAME: &str = "UserName";
pub(super) const KP_PASSWORD: &str = "Password";
pub(super) const KP_URL: &str = "URL";
pub(super) const KP_NOTES: &str = "Notes";

// ---------------------------------------------------------------------------
// KeePassParser
// ---------------------------------------------------------------------------

/// Parser for KeePass .kdbx database files.
///
/// Opens the database using an optional password, then recursively walks
/// all groups and extracts entries into [`ParsedItem`] values.
///
/// Group hierarchy is flattened into tags (e.g. "General/Finance" becomes
/// tags `["General", "Finance"]`). Native KeePass tags from entries are
/// merged with group-path tags, deduplicated. Custom string fields beyond
/// the standard five are appended to the notes field as "Custom: key = value".
pub struct KeePassParser;

impl FormatParser for KeePassParser {
    fn format(&self) -> ImportSource {
        ImportSource::KeePass
    }

    fn parse(
        &self,
        path: &Path,
        password: Option<&SecureStr>,
        _csv_mapping: Option<&CsvColumnMapping>,
    ) -> Result<Vec<ParsedItem>, ImportExportError> {
        let key = build_database_key(password)?;

        let mut file = std::fs::File::open(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ImportExportError::FileNotFound(path.to_path_buf())
            } else {
                ImportExportError::FileReadError {
                    path: path.to_path_buf(),
                    reason: e.to_string(),
                }
            }
        })?;

        let db = keepass::Database::open(&mut file, key).map_err(|e| map_open_error(e, path))?;

        let mut items = Vec::new();
        walk_group(&db, &db.root(), "", &mut items);

        Ok(items)
    }

    fn requires_password(&self) -> bool {
        false // May or may not need a password depending on the file.
    }

    fn validate_file(&self, path: &Path) -> Result<(), ImportExportError> {
        validate_file_common(path, "kdbx")
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a `DatabaseKey` from the optional password.
fn build_database_key(
    password: Option<&SecureStr>,
) -> Result<keepass::DatabaseKey, ImportExportError> {
    match password {
        Some(pw) => Ok(keepass::DatabaseKey::new().with_password(pw.get())),
        None => Err(ImportExportError::PasswordRequired),
    }
}

/// Map a `DatabaseOpenError` to the appropriate `ImportExportError`.
fn map_open_error(err: DatabaseOpenError, path: &Path) -> ImportExportError {
    match &err {
        DatabaseOpenError::Key(key_err) => {
            if matches!(key_err, keepass::error::DatabaseKeyError::IncorrectKey) {
                ImportExportError::InvalidPassword
            } else {
                ImportExportError::DecryptionFailed(key_err.to_string())
            }
        }
        DatabaseOpenError::UnsupportedVersion | DatabaseOpenError::VersionParse(_) => {
            ImportExportError::InvalidFormat("unsupported KeePass database version".to_string())
        }
        _ => ImportExportError::FileReadError {
            path: path.to_path_buf(),
            reason: err.to_string(),
        },
    }
}

/// Recursively walk a KeePass group tree, converting entries to `ParsedItem`.
fn walk_group(
    db: &keepass::Database,
    group: &keepass::db::Group,
    group_path: &str,
    items: &mut Vec<ParsedItem>,
) {
    for entry_id in group.entry_ids() {
        if let Some(entry) = db.entry(entry_id) {
            items.push(entry_to_parsed_item(&entry, group_path));
        }
    }

    for group_id in group.group_ids() {
        if let Some(child) = db.group(group_id) {
            let child_path = if group_path.is_empty() {
                child.name.clone()
            } else {
                format!("{}/{}", group_path, child.name)
            };
            walk_group(db, &child, &child_path, items);
        }
    }
}

/// Convert a KeePass `Entry` into a `ParsedItem`.
///
/// Standard fields (Title, UserName, Password, URL, Notes) are extracted
/// directly. Any remaining string fields are appended to notes as
/// "Custom: key = value". Tags are built from group path + native entry
/// tags, deduplicated.
pub(super) fn entry_to_parsed_item(entry: &keepass::db::Entry, group_path: &str) -> ParsedItem {
    let uuid = entry.id().uuid().to_string();

    let title = entry.get_title().unwrap_or("").to_string();
    let username = entry.get_username().unwrap_or("").to_string();
    let password = entry.get_password().unwrap_or("").to_string();
    let url = entry.get_url().unwrap_or("").to_string();
    let base_notes = entry.get(KP_NOTES).unwrap_or("").to_string();

    let standard_keys: &[&str] = &[KP_TITLE, KP_USERNAME, KP_PASSWORD, KP_URL, KP_NOTES];
    let custom_notes = build_custom_notes(entry, standard_keys);

    let notes = if custom_notes.is_empty() {
        base_notes
    } else if base_notes.is_empty() {
        custom_notes
    } else {
        format!("{}\n{}", base_notes, custom_notes)
    };

    let mut fields = HashMap::new();
    fields.insert("name".to_string(), title);
    fields.insert("username".to_string(), username);
    fields.insert("password".to_string(), password);
    fields.insert("url".to_string(), url);
    fields.insert("notes".to_string(), notes);

    // Build tags: group path first, then native tags (deduplicated).
    let mut tags: Vec<String> = group_path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    for tag in &entry.tags {
        if !tags.contains(tag) {
            tags.push(tag.clone());
        }
    }

    ParsedItem {
        source_id: uuid,
        fields,
        tags,
    }
}

/// Build a string with custom (non-standard) fields formatted as
/// "Custom: key = value", one per line.
pub(super) fn build_custom_notes(entry: &keepass::db::Entry, standard_keys: &[&str]) -> String {
    let mut custom_parts: Vec<String> = Vec::new();

    for key in entry.fields.keys() {
        if standard_keys.contains(&key.as_str()) {
            continue;
        }
        if let Some(val) = entry.get(key) {
            custom_parts.push(format!("Custom: {} = {}", key, val));
        }
    }

    custom_parts.join("\n")
}
