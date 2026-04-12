//! KeePass .kdbx database import parser.
//!
//! Parses encrypted KeePass database files using the `keepass` crate,
//! extracting all entries with their fields, group paths as tags,
//! and custom fields appended to notes.

use std::collections::HashMap;
use std::path::Path;

use keepass::error::DatabaseKeyError;
use keepass::error::DatabaseOpenError;

use crate::commands::types::CsvColumnMapping;
use crate::commands::types::ImportSource;
use crate::errors::mapping::import_export::ImportExportError;
use crate::services::import_export::parser::{validate_file_common, FormatParser, ParsedItem};
use crate::types::SecureStr;

// Standard KeePass field names used for direct extraction.
const KP_TITLE: &str = "Title";
const KP_USERNAME: &str = "UserName";
const KP_PASSWORD: &str = "Password";
const KP_URL: &str = "URL";
const KP_NOTES: &str = "Notes";

// ---------------------------------------------------------------------------
// KeePassParser
// ---------------------------------------------------------------------------

/// Parser for KeePass .kdbx database files.
///
/// Opens the database using an optional password, then recursively walks
/// all groups and extracts entries into [`ParsedItem`] values.
///
/// Group hierarchy is flattened into tags (e.g. "General/Finance" becomes
/// tags `["General", "Finance"]`). Custom string fields beyond the standard
/// five are appended to the notes field as "Custom: key = value".
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
        // Build the database key from the optional password.
        let key = build_database_key(password)?;

        // Open the .kdbx database.
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

        // Walk the group tree and collect entries.
        let mut items = Vec::new();
        walk_group(&db.root, "", &mut items);

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
///
/// Returns `PasswordRequired` if no password was provided (all .kdbx files
/// require at least one key component).
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
        DatabaseOpenError::Key(key_err) => match key_err {
            DatabaseKeyError::IncorrectKey => ImportExportError::InvalidPassword,
            _ => ImportExportError::DecryptionFailed(key_err.to_string()),
        },
        DatabaseOpenError::DatabaseIntegrity(integrity_err) => ImportExportError::ParseError {
            format: "KeePass .kdbx".to_string(),
            reason: integrity_err.to_string(),
        },
        DatabaseOpenError::UnsupportedVersion => {
            ImportExportError::InvalidFormat("unsupported KeePass database version".to_string())
        }
        _ => ImportExportError::FileReadError {
            path: path.to_path_buf(),
            reason: err.to_string(),
        },
    }
}

/// Recursively walk a KeePass group tree, converting entries to `ParsedItem`.
fn walk_group(group: &keepass::db::Group, group_path: &str, items: &mut Vec<ParsedItem>) {
    for node in &group.children {
        match node {
            keepass::db::Node::Entry(entry) => {
                items.push(entry_to_parsed_item(entry, group_path));
            }
            keepass::db::Node::Group(child_group) => {
                let child_path = if group_path.is_empty() {
                    child_group.name.clone()
                } else {
                    format!("{}/{}", group_path, child_group.name)
                };
                walk_group(child_group, &child_path, items);
            }
        }
    }
}

/// Convert a KeePass `Entry` into a `ParsedItem`.
///
/// Standard fields (Title, UserName, Password, URL, Notes) are extracted
/// directly. Any remaining string fields are appended to notes as
/// "Custom: key = value". The group path is split into tags.
fn entry_to_parsed_item(entry: &keepass::db::Entry, group_path: &str) -> ParsedItem {
    let uuid = entry.get_uuid().to_string();

    // Extract standard fields.
    let title = entry.get_title().unwrap_or("").to_string();
    let username = entry.get_username().unwrap_or("").to_string();
    let password = entry.get_password().unwrap_or("").to_string();
    let url = entry.get_url().unwrap_or("").to_string();
    let base_notes = entry.get(KP_NOTES).unwrap_or("").to_string();

    // Collect standard field keys for custom field filtering.
    let standard_keys: &[&str] = &[KP_TITLE, KP_USERNAME, KP_PASSWORD, KP_URL, KP_NOTES];

    // Build custom fields from non-standard entries.
    let custom_notes = build_custom_notes(entry, standard_keys);

    // Combine base notes and custom field notes.
    let notes = if custom_notes.is_empty() {
        base_notes
    } else if base_notes.is_empty() {
        custom_notes
    } else {
        format!("{}\n{}", base_notes, custom_notes)
    };

    // Build the fields map.
    let mut fields = HashMap::new();
    fields.insert("name".to_string(), title);
    fields.insert("username".to_string(), username);
    fields.insert("password".to_string(), password);
    fields.insert("url".to_string(), url);
    fields.insert("notes".to_string(), notes);

    // Split group path into tags.
    let tags = if group_path.is_empty() {
        Vec::new()
    } else {
        group_path.split('/').map(String::from).collect()
    };

    ParsedItem {
        source_id: uuid,
        fields,
        tags,
    }
}

/// Build a string with custom (non-standard) fields formatted as
/// "Custom: key = value", one per line.
fn build_custom_notes(entry: &keepass::db::Entry, standard_keys: &[&str]) -> String {
    let mut custom_parts: Vec<String> = Vec::new();

    for key in entry.fields.keys() {
        if standard_keys.contains(&key.as_str()) {
            continue;
        }
        // Use entry.get() to automatically unprotect protected values.
        if let Some(val) = entry.get(key) {
            custom_parts.push(format!("Custom: {} = {}", key, val));
        }
    }

    custom_parts.join("\n")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Trait method tests ---------------------------------------------------

    #[test]
    fn format_returns_keepass() {
        let parser = KeePassParser;
        assert_eq!(parser.format(), ImportSource::KeePass);
    }

    #[test]
    fn requires_password_returns_false() {
        let parser = KeePassParser;
        assert!(!parser.requires_password());
    }

    // -- validate_file tests --------------------------------------------------

    #[test]
    fn validate_file_kdbx_extension_returns_ok() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("database.kdbx");
        std::fs::write(&path, b"fake-kdbx").expect("write");

        let parser = KeePassParser;
        assert!(parser.validate_file(&path).is_ok());
    }

    #[test]
    fn validate_file_wrong_extension_returns_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("data.txt");
        std::fs::write(&path, b"not-kdbx").expect("write");

        let parser = KeePassParser;
        let result = parser.validate_file(&path);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("expected .kdbx"),
            "expected extension error, got: {msg}"
        );
    }

    #[test]
    fn validate_file_nonexistent_returns_file_not_found() {
        let path = Path::new("/tmp/__oak_test_nonexistent_kdbx_42__.kdbx");
        let parser = KeePassParser;
        let result = parser.validate_file(path);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ImportExportError::FileNotFound(_)
        ),);
    }

    // -- parse error tests ----------------------------------------------------

    #[test]
    fn parse_nonexistent_file_returns_file_not_found() {
        let parser = KeePassParser;
        let path = Path::new("/tmp/__oak_test_no_such_file__.kdbx");
        let pw = SecureStr::new("test".into());
        let result = parser.parse(path, Some(&pw), None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ImportExportError::FileNotFound(_)
        ),);
    }

    #[test]
    fn parse_no_password_returns_password_required() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.kdbx");
        std::fs::write(&path, b"not-a-real-kdbx").expect("write");

        let parser = KeePassParser;
        let result = parser.parse(&path, None, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ImportExportError::PasswordRequired
        ),);
    }

    #[test]
    fn parse_invalid_kdbx_with_wrong_password_returns_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.kdbx");
        // Write invalid data that looks like a .kdbx file but is not.
        std::fs::write(&path, b"\x03\xd9\xa2\x9a\x67\xfb\x4b\xb5").expect("write");

        let parser = KeePassParser;
        let pw = SecureStr::new("wrong-password".into());
        let result = parser.parse(&path, Some(&pw), None);
        assert!(result.is_err());
        // Should be either InvalidPassword, DecryptionFailed, or ParseError.
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            matches!(
                err,
                ImportExportError::InvalidPassword
                    | ImportExportError::DecryptionFailed(_)
                    | ImportExportError::ParseError { .. }
                    | ImportExportError::FileReadError { .. }
            ),
            "expected open-related error, got: {err:?} (msg: {msg})"
        );
    }

    // -- Helper function tests ------------------------------------------------

    #[test]
    fn build_custom_notes_skips_standard_fields() {
        let mut entry = keepass::db::Entry::new();
        entry
            .fields
            .insert(KP_TITLE.into(), keepass::db::Value::Unprotected("T".into()));
        entry.fields.insert(
            KP_USERNAME.into(),
            keepass::db::Value::Unprotected("U".into()),
        );
        entry.fields.insert(
            "API Key".into(),
            keepass::db::Value::Unprotected("abc123".into()),
        );

        let standard: &[&str] = &[KP_TITLE, KP_USERNAME, KP_PASSWORD, KP_URL, KP_NOTES];
        let result = build_custom_notes(&entry, standard);
        assert!(result.contains("Custom: API Key = abc123"), "got: {result}");
        assert!(!result.contains("Title"), "should skip standard fields");
        assert!(!result.contains("UserName"), "should skip standard fields");
    }

    #[test]
    fn build_custom_notes_empty_when_all_standard() {
        let mut entry = keepass::db::Entry::new();
        entry
            .fields
            .insert(KP_TITLE.into(), keepass::db::Value::Unprotected("T".into()));

        let standard: &[&str] = &[KP_TITLE, KP_USERNAME, KP_PASSWORD, KP_URL, KP_NOTES];
        let result = build_custom_notes(&entry, standard);
        assert!(result.is_empty());
    }

    #[test]
    fn entry_to_parsed_item_extracts_standard_fields() {
        let mut entry = keepass::db::Entry::new();
        let uuid = *entry.get_uuid();
        entry.fields.insert(
            KP_TITLE.into(),
            keepass::db::Value::Unprotected("Gmail".into()),
        );
        entry.fields.insert(
            KP_USERNAME.into(),
            keepass::db::Value::Unprotected("user@gmail.com".into()),
        );
        entry.fields.insert(
            KP_PASSWORD.into(),
            keepass::db::Value::Unprotected("s3cret".into()),
        );
        entry.fields.insert(
            KP_URL.into(),
            keepass::db::Value::Unprotected("https://gmail.com".into()),
        );
        entry.fields.insert(
            KP_NOTES.into(),
            keepass::db::Value::Unprotected("Personal email".into()),
        );

        let item = entry_to_parsed_item(&entry, "General/Email");

        assert_eq!(item.source_id, uuid.to_string());
        assert_eq!(item.fields.get("name").unwrap(), "Gmail");
        assert_eq!(item.fields.get("username").unwrap(), "user@gmail.com");
        assert_eq!(item.fields.get("password").unwrap(), "s3cret");
        assert_eq!(item.fields.get("url").unwrap(), "https://gmail.com");
        assert_eq!(item.fields.get("notes").unwrap(), "Personal email");
        assert_eq!(item.tags, vec!["General", "Email"]);
    }

    #[test]
    fn entry_to_parsed_item_root_group_has_no_tags() {
        let entry = keepass::db::Entry::new();
        let item = entry_to_parsed_item(&entry, "");
        assert!(item.tags.is_empty());
    }

    #[test]
    fn entry_to_parsed_item_custom_fields_appended_to_notes() {
        let mut entry = keepass::db::Entry::new();
        entry.fields.insert(
            KP_TITLE.into(),
            keepass::db::Value::Unprotected("Test".into()),
        );
        entry.fields.insert(
            "API Key".into(),
            keepass::db::Value::Unprotected("key123".into()),
        );
        entry.fields.insert(
            "Region".into(),
            keepass::db::Value::Unprotected("us-east-1".into()),
        );

        let item = entry_to_parsed_item(&entry, "");
        let notes = item.fields.get("notes").unwrap();
        assert!(notes.contains("Custom: API Key = key123"), "got: {notes}");
        assert!(notes.contains("Custom: Region = us-east-1"), "got: {notes}");
    }

    #[test]
    fn entry_to_parsed_item_protected_field_is_unprotected() {
        let mut entry = keepass::db::Entry::new();
        // Use Bytes value for Password — entry.get() returns None for Bytes,
        // so the password field in ParsedItem will be empty.
        entry
            .fields
            .insert(KP_PASSWORD.into(), keepass::db::Value::Bytes(vec![1, 2, 3]));

        let item = entry_to_parsed_item(&entry, "");
        // Bytes values are not returned by entry.get(), so password will be empty.
        assert_eq!(item.fields.get("password").unwrap(), "");
    }
}
