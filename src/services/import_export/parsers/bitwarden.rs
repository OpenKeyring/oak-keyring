use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::commands::types::ImportSource;
use crate::errors::mapping::import_export::ImportExportError;
use crate::services::import_export::parser::{validate_file_common, FormatParser, ParsedItem};
use crate::types::SecureStr;

// ---------------------------------------------------------------------------
// Deserialization structs for Bitwarden JSON export
// ---------------------------------------------------------------------------

/// Top-level Bitwarden JSON export structure.
#[derive(Deserialize)]
struct BitwardenExport {
    encrypted: Option<bool>,
    items: Vec<BitwardenItem>,
}

/// A single item in a Bitwarden export.
#[derive(Deserialize)]
struct BitwardenItem {
    #[serde(rename = "type")]
    item_type: u32,
    name: Option<String>,
    login: Option<BitwardenLogin>,
    notes: Option<String>,
    fields: Option<Vec<BitwardenField>>,
    // Ignored: collectionIds, folderId, organizationId, etc.
}

/// Login data within a Bitwarden item.
#[derive(Deserialize)]
struct BitwardenLogin {
    username: Option<String>,
    password: Option<String>,
    uris: Option<Vec<BitwardenUri>>,
    totp: Option<String>,
}

/// A URI entry in a Bitwarden login.
#[derive(Deserialize)]
struct BitwardenUri {
    uri: Option<String>,
}

/// A custom field in a Bitwarden item.
#[derive(Deserialize)]
#[allow(dead_code)] // field_type kept for format fidelity; may be used later.
struct BitwardenField {
    name: Option<String>,
    value: Option<String>,
    #[serde(rename = "type")]
    field_type: Option<u32>,
}

// ---------------------------------------------------------------------------
// BitwardenParser — parses Bitwarden .json exports
// ---------------------------------------------------------------------------

/// Parser for Bitwarden JSON export files.
///
/// Supports plaintext (unencrypted) exports only. Encrypted exports return
/// [`ImportExportError::PasswordRequired`].
///
/// Type mapping:
/// - type 1 (Login): maps username, password, first URI, notes, custom fields.
/// - type 2 (SecureNote): maps name and notes only.
/// - type 3 (Card) and type 4 (Identity): skipped (unsupported).
pub struct BitwardenParser;

/// Bitwarden item type constants.
const TYPE_LOGIN: u32 = 1;
const TYPE_SECURE_NOTE: u32 = 2;
const TYPE_CARD: u32 = 3;
const TYPE_IDENTITY: u32 = 4;

impl FormatParser for BitwardenParser {
    fn format(&self) -> ImportSource {
        ImportSource::Bitwarden
    }

    fn parse(
        &self,
        path: &Path,
        _password: Option<&SecureStr>,
        _csv_mapping: Option<&CsvColumnMapping>,
    ) -> Result<Vec<ParsedItem>, ImportExportError> {
        // Read and deserialize the JSON file.
        let content =
            std::fs::read_to_string(path).map_err(|e| ImportExportError::FileReadError {
                path: path.to_path_buf(),
                reason: e.to_string(),
            })?;

        let export: BitwardenExport =
            serde_json::from_str(&content).map_err(|e| ImportExportError::ParseError {
                format: "Bitwarden JSON".to_string(),
                reason: e.to_string(),
            })?;

        // Check if the export is encrypted.
        if export.encrypted == Some(true) {
            return Err(ImportExportError::PasswordRequired);
        }

        // Process each item based on its type.
        let mut items = Vec::new();

        for (index, bw_item) in export.items.iter().enumerate() {
            match bw_item.item_type {
                TYPE_LOGIN => {
                    items.push(parse_login_item(index, bw_item));
                }
                TYPE_SECURE_NOTE => {
                    items.push(parse_secure_note_item(index, bw_item));
                }
                TYPE_CARD | TYPE_IDENTITY => {
                    // Skip unsupported types.
                }
                _ => {
                    // Skip unknown types.
                }
            }
        }

        Ok(items)
    }

    fn requires_password(&self) -> bool {
        false
    }

    fn validate_file(&self, path: &Path) -> Result<(), ImportExportError> {
        validate_file_common(path, "json")
    }
}

// ---------------------------------------------------------------------------
// Item parsing helpers
// ---------------------------------------------------------------------------

/// Parse a Bitwarden Login item (type 1) into a ParsedItem.
fn parse_login_item(index: usize, item: &BitwardenItem) -> ParsedItem {
    let mut fields = HashMap::new();

    fields.insert("name".to_string(), item.name.clone().unwrap_or_default());

    if let Some(ref login) = item.login {
        fields.insert(
            "username".to_string(),
            login.username.clone().unwrap_or_default(),
        );
        fields.insert(
            "password".to_string(),
            login.password.clone().unwrap_or_default(),
        );
        if let Some(totp) = login.totp.as_ref().filter(|value| !value.is_empty()) {
            fields.insert("totp".to_string(), totp.clone());
        }

        // Use the first URI if available.
        let url = login
            .uris
            .as_ref()
            .and_then(|uris| uris.first())
            .and_then(|u| u.uri.clone())
            .unwrap_or_default();
        fields.insert("url".to_string(), url);
    } else {
        fields.insert("username".to_string(), String::new());
        fields.insert("password".to_string(), String::new());
        fields.insert("url".to_string(), String::new());
    }

    // Build notes: combine existing notes with custom fields.
    let notes = build_notes(&item.notes, &item.fields);
    fields.insert("notes".to_string(), notes);

    ParsedItem {
        source_id: format!("bw-{index}"),
        fields,
        tags: Vec::new(),
    }
}

/// Parse a Bitwarden SecureNote item (type 2) into a ParsedItem.
fn parse_secure_note_item(index: usize, item: &BitwardenItem) -> ParsedItem {
    let mut fields = HashMap::new();

    fields.insert("name".to_string(), item.name.clone().unwrap_or_default());

    // SecureNotes have no login fields — set them empty.
    fields.insert("username".to_string(), String::new());
    fields.insert("password".to_string(), String::new());
    fields.insert("url".to_string(), String::new());

    // Build notes: combine existing notes with custom fields.
    let notes = build_notes(&item.notes, &item.fields);
    fields.insert("notes".to_string(), notes);

    ParsedItem {
        source_id: format!("bw-{index}"),
        fields,
        tags: Vec::new(),
    }
}

/// Combine base notes with custom Bitwarden fields appended as
/// "Field: name = value" lines.
fn build_notes(base_notes: &Option<String>, fields: &Option<Vec<BitwardenField>>) -> String {
    let mut result = base_notes.clone().unwrap_or_default();

    if let Some(ref custom_fields) = fields {
        for field in custom_fields {
            let name = field.name.as_deref().unwrap_or("unnamed");
            let value = field.value.as_deref().unwrap_or("");
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&format!("Field: {name} = {value}"));
        }
    }

    result
}

// Silence unused-import warning for CsvColumnMapping alias used in trait.
use crate::commands::types::CsvColumnMapping;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Helper: create a temporary JSON file with the given content.
    fn create_json_file(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".json")
            .tempfile()
            .expect("create temp json");
        f.write_all(content.as_bytes()).expect("write json");
        f
    }

    // -- Test 1: Multiple Login items ----------------------------------------

    #[test]
    fn multiple_login_items_produce_correct_parsed_items() {
        let json = r#"{
            "encrypted": false,
            "items": [
                {
                    "type": 1,
                    "name": "Gmail",
                    "login": {
                        "username": "user1@gmail.com",
                        "password": "pass1",
                        "uris": [{"uri": "https://gmail.com"}]
                    },
                    "notes": "Personal email"
                },
                {
                    "type": 1,
                    "name": "GitHub",
                    "login": {
                        "username": "dev@github.com",
                        "password": "pass2",
                        "uris": [{"uri": "https://github.com"}]
                    },
                    "notes": "Code repo"
                },
                {
                    "type": 1,
                    "name": "AWS",
                    "login": {
                        "username": "admin@aws.com",
                        "password": "pass3",
                        "uris": [{"uri": "https://aws.amazon.com"}]
                    },
                    "notes": "Cloud account"
                }
            ]
        }"#;
        let f = create_json_file(json);
        let parser = BitwardenParser;

        let items = parser
            .parse(f.path(), None, None)
            .expect("parse should succeed");

        assert_eq!(items.len(), 3);

        // Item 0
        assert_eq!(items[0].source_id, "bw-0");
        assert_eq!(items[0].fields.get("name").unwrap(), "Gmail");
        assert_eq!(items[0].fields.get("username").unwrap(), "user1@gmail.com");
        assert_eq!(items[0].fields.get("password").unwrap(), "pass1");
        assert_eq!(items[0].fields.get("url").unwrap(), "https://gmail.com");
        assert_eq!(items[0].fields.get("notes").unwrap(), "Personal email");

        // Item 1
        assert_eq!(items[1].source_id, "bw-1");
        assert_eq!(items[1].fields.get("name").unwrap(), "GitHub");

        // Item 2
        assert_eq!(items[2].source_id, "bw-2");
        assert_eq!(items[2].fields.get("name").unwrap(), "AWS");
    }

    // -- Test 2: SecureNote handling -----------------------------------------

    #[test]
    fn secure_note_produces_name_and_notes_only() {
        let json = r#"{
            "encrypted": false,
            "items": [
                {
                    "type": 2,
                    "name": "My Secret Note",
                    "notes": "This is a secure note with important info."
                }
            ]
        }"#;
        let f = create_json_file(json);
        let parser = BitwardenParser;

        let items = parser
            .parse(f.path(), None, None)
            .expect("parse should succeed");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].source_id, "bw-0");
        assert_eq!(items[0].fields.get("name").unwrap(), "My Secret Note");
        assert_eq!(
            items[0].fields.get("notes").unwrap(),
            "This is a secure note with important info."
        );
        // Login fields should be empty.
        assert_eq!(items[0].fields.get("username").unwrap(), "");
        assert_eq!(items[0].fields.get("password").unwrap(), "");
        assert_eq!(items[0].fields.get("url").unwrap(), "");
    }

    // -- Test 3: Card/Identity filtering -------------------------------------

    #[test]
    fn card_and_identity_items_are_skipped() {
        let json = r#"{
            "encrypted": false,
            "items": [
                {
                    "type": 1,
                    "name": "Login Item",
                    "login": {
                        "username": "user",
                        "password": "pass"
                    }
                },
                {
                    "type": 3,
                    "name": "My Visa Card",
                    "notes": "Card number redacted"
                },
                {
                    "type": 4,
                    "name": "John Doe Identity",
                    "notes": "Personal info"
                }
            ]
        }"#;
        let f = create_json_file(json);
        let parser = BitwardenParser;

        let items = parser
            .parse(f.path(), None, None)
            .expect("parse should succeed");

        // Only the Login item should be returned.
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].fields.get("name").unwrap(), "Login Item");
    }

    // -- Test 4: Custom fields appended to notes -----------------------------

    #[test]
    fn custom_fields_appended_to_notes() {
        let json = r#"{
            "encrypted": false,
            "items": [
                {
                    "type": 1,
                    "name": "Test Entry",
                    "login": {
                        "username": "user",
                        "password": "pass"
                    },
                    "notes": "Base note",
                    "fields": [
                        {"name": "API Key", "value": "abc123", "type": 0},
                        {"name": "Region", "value": "us-east-1", "type": 0}
                    ]
                }
            ]
        }"#;
        let f = create_json_file(json);
        let parser = BitwardenParser;

        let items = parser
            .parse(f.path(), None, None)
            .expect("parse should succeed");

        assert_eq!(items.len(), 1);
        let notes = items[0].fields.get("notes").unwrap();
        assert!(notes.contains("Base note"), "got: {notes}");
        assert!(notes.contains("Field: API Key = abc123"), "got: {notes}");
        assert!(notes.contains("Field: Region = us-east-1"), "got: {notes}");
    }

    // -- Test 5: Empty items array -------------------------------------------

    #[test]
    fn empty_items_array_returns_empty_vec() {
        let json = r#"{"encrypted": false, "items": []}"#;
        let f = create_json_file(json);
        let parser = BitwardenParser;

        let items = parser
            .parse(f.path(), None, None)
            .expect("parse should succeed");

        assert!(items.is_empty());
    }

    // -- Test 6: Encrypted JSON returns PasswordRequired ---------------------

    #[test]
    fn encrypted_json_returns_password_required() {
        let json = r#"{"encrypted": true, "items": []}"#;
        let f = create_json_file(json);
        let parser = BitwardenParser;

        let result = parser.parse(f.path(), None, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ImportExportError::PasswordRequired),
            "expected PasswordRequired, got: {err:?}"
        );
    }

    // -- Test 7: Invalid JSON returns ParseError -----------------------------

    #[test]
    fn invalid_json_returns_parse_error() {
        let json = "{this is not valid json}";
        let f = create_json_file(json);
        let parser = BitwardenParser;

        let result = parser.parse(f.path(), None, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ImportExportError::ParseError { .. }),
            "expected ParseError, got: {err:?}"
        );
    }

    // -- Test 8: Missing optional fields (null login) ------------------------

    #[test]
    fn missing_login_produces_empty_fields() {
        let json = r#"{
            "encrypted": false,
            "items": [
                {
                    "type": 1,
                    "name": "No Login Data"
                }
            ]
        }"#;
        let f = create_json_file(json);
        let parser = BitwardenParser;

        let items = parser
            .parse(f.path(), None, None)
            .expect("parse should succeed");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].fields.get("name").unwrap(), "No Login Data");
        assert_eq!(items[0].fields.get("username").unwrap(), "");
        assert_eq!(items[0].fields.get("password").unwrap(), "");
        assert_eq!(items[0].fields.get("url").unwrap(), "");
        assert_eq!(items[0].fields.get("notes").unwrap(), "");
    }

    // -- Test 9: Multiple URIs — first one used -----------------------------

    #[test]
    fn multiple_uris_uses_first() {
        let json = r#"{
            "encrypted": false,
            "items": [
                {
                    "type": 1,
                    "name": "Multi-URI",
                    "login": {
                        "username": "user",
                        "password": "pass",
                        "uris": [
                            {"uri": "https://primary.com"},
                            {"uri": "https://secondary.com"},
                            {"uri": "https://tertiary.com"}
                        ]
                    }
                }
            ]
        }"#;
        let f = create_json_file(json);
        let parser = BitwardenParser;

        let items = parser
            .parse(f.path(), None, None)
            .expect("parse should succeed");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].fields.get("url").unwrap(), "https://primary.com");
    }

    #[test]
    fn login_totp_is_preserved_as_structured_field() {
        let json = r#"{
            "encrypted": false,
            "items": [
                {
                    "type": 1,
                    "name": "GitHub",
                    "login": {
                        "username": "alice",
                        "password": "secret",
                        "totp": "otpauth://totp/GitHub:alice?secret=JBSWY3DPEHPK3PXP&issuer=GitHub"
                    }
                }
            ]
        }"#;
        let f = create_json_file(json);
        let parser = BitwardenParser;

        let items = parser
            .parse(f.path(), None, None)
            .expect("parse should succeed");

        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].fields.get("totp").map(String::as_str),
            Some("otpauth://totp/GitHub:alice?secret=JBSWY3DPEHPK3PXP&issuer=GitHub")
        );
        assert!(!items[0].fields.get("notes").unwrap().contains("TOTP"));
    }

    // -- Test 10: validate_file with .json extension -------------------------

    #[test]
    fn validate_file_json_extension_returns_ok() {
        let f = create_json_file("{}");
        let parser = BitwardenParser;
        assert!(parser.validate_file(f.path()).is_ok());
    }

    // -- Additional edge case tests -----------------------------------------

    #[test]
    fn no_encrypted_field_treated_as_plaintext() {
        // When "encrypted" field is absent, treat as plaintext.
        let json = r#"{
            "items": [
                {
                    "type": 1,
                    "name": "Legacy Export",
                    "login": {"username": "u", "password": "p"}
                }
            ]
        }"#;
        let f = create_json_file(json);
        let parser = BitwardenParser;

        let items = parser
            .parse(f.path(), None, None)
            .expect("parse should succeed");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].fields.get("name").unwrap(), "Legacy Export");
    }

    #[test]
    fn custom_fields_with_no_base_notes() {
        let json = r#"{
            "encrypted": false,
            "items": [
                {
                    "type": 2,
                    "name": "Note With Fields",
                    "fields": [
                        {"name": "Recovery Code", "value": "XYZ789", "type": 1}
                    ]
                }
            ]
        }"#;
        let f = create_json_file(json);
        let parser = BitwardenParser;

        let items = parser
            .parse(f.path(), None, None)
            .expect("parse should succeed");

        assert_eq!(items.len(), 1);
        let notes = items[0].fields.get("notes").unwrap();
        assert!(
            notes.contains("Field: Recovery Code = XYZ789"),
            "got: {notes}"
        );
        // Should NOT start with a newline.
        assert!(
            !notes.starts_with('\n'),
            "notes should not start with newline"
        );
    }

    #[test]
    fn format_returns_bitwarden() {
        let parser = BitwardenParser;
        assert_eq!(parser.format(), ImportSource::Bitwarden);
    }

    #[test]
    fn requires_password_returns_false() {
        let parser = BitwardenParser;
        assert!(!parser.requires_password());
    }

    #[test]
    fn validate_file_wrong_extension_returns_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("data.csv");
        std::fs::write(&path, b"{}").expect("write");

        let parser = BitwardenParser;
        let result = parser.validate_file(&path);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("expected .json"),
            "expected extension error, got: {msg}"
        );
    }

    #[test]
    fn field_with_null_name_uses_unnamed() {
        let json = r#"{
            "encrypted": false,
            "items": [
                {
                    "type": 1,
                    "name": "Null Field Name",
                    "login": {"username": "u", "password": "p"},
                    "notes": "base",
                    "fields": [
                        {"name": null, "value": "some_value", "type": 0}
                    ]
                }
            ]
        }"#;
        let f = create_json_file(json);
        let parser = BitwardenParser;

        let items = parser
            .parse(f.path(), None, None)
            .expect("parse should succeed");
        let notes = items[0].fields.get("notes").unwrap();
        assert!(
            notes.contains("Field: unnamed = some_value"),
            "got: {notes}"
        );
    }
}
