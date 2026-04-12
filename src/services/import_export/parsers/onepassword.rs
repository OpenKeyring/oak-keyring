//! 1Password import parsers for .1pux (ZIP+JSON) and .opvault (stub) formats.
//!
//! **OnePuxParser** — reads `.1pux` files, which are ZIP archives containing
//! an `export.data` JSON file. Only specific item categories are mapped:
//! - `"001"` (Login): all fields
//! - `"005"` (Password): mapped as Login
//! - `"110"` (Secure Note): name + notes only
//!
//! **OpVaultParser** — placeholder stub that returns `UnsupportedFormat`.
//! Full `.opvault` support will be added in a future task.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use serde::Deserialize;
use zip::ZipArchive;

use crate::commands::types::CsvColumnMapping;
use crate::commands::types::ImportSource;
use crate::errors::mapping::import_export::ImportExportError;
use crate::services::import_export::parser::{validate_file_common, FormatParser, ParsedItem};
use crate::types::SecureStr;

// ---------------------------------------------------------------------------
// Deserialization structs for .1pux JSON
// ---------------------------------------------------------------------------

/// Top-level .1pux export structure.
#[derive(Deserialize)]
struct OnePuxExport {
    accounts: Vec<OnePuxAccount>,
}

/// A single account in the export.
#[derive(Deserialize)]
struct OnePuxAccount {
    vaults: Vec<OnePuxVault>,
}

/// A vault within an account.
#[derive(Deserialize)]
struct OnePuxVault {
    items: Vec<OnePuxItem>,
}

/// A single item in a vault.
#[derive(Deserialize)]
#[allow(non_snake_case)]
struct OnePuxItem {
    categoryUuid: String,
    title: Option<String>,
    login: Option<OnePuxLogin>,
    notesPlain: Option<String>,
    tags: Option<Vec<String>>,
}

/// Login credentials within a 1Password item.
#[derive(Deserialize)]
struct OnePuxLogin {
    username: Option<String>,
    password: Option<String>,
    urls: Option<Vec<OnePuxUrl>>,
}

/// A URL entry within 1Password login data.
#[derive(Deserialize)]
struct OnePuxUrl {
    href: Option<String>,
}

// ---------------------------------------------------------------------------
// Category constants
// ---------------------------------------------------------------------------

/// 1Password category UUID for Login items.
const CATEGORY_LOGIN: &str = "001";
/// 1Password category UUID for Password items.
const CATEGORY_PASSWORD: &str = "005";
/// 1Password category UUID for Secure Note items.
const CATEGORY_SECURE_NOTE: &str = "110";

// ---------------------------------------------------------------------------
// OnePuxParser — parses .1pux (ZIP + JSON) files
// ---------------------------------------------------------------------------

/// Parser for 1Password `.1pux` export files.
///
/// A `.1pux` file is a ZIP archive containing an `export.data` JSON file.
/// This parser extracts items from supported categories and converts them
/// to [`ParsedItem`] values.
pub struct OnePuxParser;

impl FormatParser for OnePuxParser {
    fn format(&self) -> ImportSource {
        ImportSource::OnePassword1pux
    }

    fn parse(
        &self,
        path: &Path,
        _password: Option<&SecureStr>,
        _csv_mapping: Option<&CsvColumnMapping>,
    ) -> Result<Vec<ParsedItem>, ImportExportError> {
        let file = std::fs::File::open(path).map_err(|e| ImportExportError::FileReadError {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;

        let mut archive = ZipArchive::new(file).map_err(|e| ImportExportError::ParseError {
            format: "1Password .1pux".to_string(),
            reason: format!("failed to open ZIP archive: {e}"),
        })?;

        // Locate and read the export.data entry.
        let mut zip_entry =
            archive
                .by_name("export.data")
                .map_err(|e| ImportExportError::ParseError {
                    format: "1Password .1pux".to_string(),
                    reason: format!("export.data not found in archive: {e}"),
                })?;

        let mut json_content = String::new();
        zip_entry
            .read_to_string(&mut json_content)
            .map_err(|e| ImportExportError::ParseError {
                format: "1Password .1pux".to_string(),
                reason: format!("failed to read export.data: {e}"),
            })?;

        let export: OnePuxExport =
            serde_json::from_str(&json_content).map_err(|e| ImportExportError::ParseError {
                format: "1Password .1pux".to_string(),
                reason: format!("failed to parse JSON: {e}"),
            })?;

        let mut items = Vec::new();
        let mut global_index: usize = 0;

        for account in &export.accounts {
            for vault in &account.vaults {
                for op_item in &vault.items {
                    match op_item.categoryUuid.as_str() {
                        CATEGORY_LOGIN | CATEGORY_PASSWORD => {
                            items.push(parse_login_item(global_index, op_item));
                        }
                        CATEGORY_SECURE_NOTE => {
                            items.push(parse_secure_note_item(global_index, op_item));
                        }
                        _ => {
                            // Skip unsupported categories (Credit Card, Identity, etc.)
                        }
                    }
                    global_index += 1;
                }
            }
        }

        Ok(items)
    }

    fn requires_password(&self) -> bool {
        false
    }

    fn validate_file(&self, path: &Path) -> Result<(), ImportExportError> {
        validate_file_common(path, "1pux")
    }
}

// ---------------------------------------------------------------------------
// Item parsing helpers
// ---------------------------------------------------------------------------

/// Parse a 1Password Login/Password item into a ParsedItem.
fn parse_login_item(index: usize, item: &OnePuxItem) -> ParsedItem {
    let mut fields = HashMap::new();

    fields.insert("name".to_string(), item.title.clone().unwrap_or_default());

    if let Some(ref login) = item.login {
        fields.insert(
            "username".to_string(),
            login.username.clone().unwrap_or_default(),
        );
        fields.insert(
            "password".to_string(),
            login.password.clone().unwrap_or_default(),
        );

        let url = login
            .urls
            .as_ref()
            .and_then(|urls| urls.first())
            .and_then(|u| u.href.clone())
            .unwrap_or_default();
        fields.insert("url".to_string(), url);
    } else {
        fields.insert("username".to_string(), String::new());
        fields.insert("password".to_string(), String::new());
        fields.insert("url".to_string(), String::new());
    }

    fields.insert(
        "notes".to_string(),
        item.notesPlain.clone().unwrap_or_default(),
    );

    ParsedItem {
        source_id: format!("1p-{index}"),
        fields,
        tags: item.tags.clone().unwrap_or_default(),
    }
}

/// Parse a 1Password Secure Note item into a ParsedItem.
fn parse_secure_note_item(index: usize, item: &OnePuxItem) -> ParsedItem {
    let mut fields = HashMap::new();

    fields.insert("name".to_string(), item.title.clone().unwrap_or_default());

    // Secure Notes have no login fields.
    fields.insert("username".to_string(), String::new());
    fields.insert("password".to_string(), String::new());
    fields.insert("url".to_string(), String::new());

    fields.insert(
        "notes".to_string(),
        item.notesPlain.clone().unwrap_or_default(),
    );

    ParsedItem {
        source_id: format!("1p-{index}"),
        fields,
        tags: item.tags.clone().unwrap_or_default(),
    }
}

// ---------------------------------------------------------------------------
// OpVaultParser — stub for future .opvault support
// ---------------------------------------------------------------------------

/// Placeholder parser for 1Password `.opvault` format.
///
/// Returns [`ImportExportError::UnsupportedFormat`] for all operations.
/// Full implementation will be added in a future task.
pub struct OpVaultParser;

impl FormatParser for OpVaultParser {
    fn format(&self) -> ImportSource {
        ImportSource::OnePasswordOpvault
    }

    fn parse(
        &self,
        _path: &Path,
        _password: Option<&SecureStr>,
        _csv_mapping: Option<&CsvColumnMapping>,
    ) -> Result<Vec<ParsedItem>, ImportExportError> {
        Err(ImportExportError::UnsupportedFormat(
            ".opvault format is not yet supported".to_string(),
        ))
    }

    fn requires_password(&self) -> bool {
        true
    }

    fn validate_file(&self, path: &Path) -> Result<(), ImportExportError> {
        // .opvault is a directory, not a file.
        if !path.exists() {
            return Err(ImportExportError::FileNotFound(path.to_path_buf()));
        }
        if !path.is_dir() {
            return Err(ImportExportError::InvalidFormat(
                "expected .opvault directory".to_string(),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, Write};
    use zip::write::SimpleFileOptions;
    use zip::CompressionMethod;

    // -- Test helpers --------------------------------------------------------

    /// Create a temporary .1pux (ZIP) file containing the given JSON as
    /// `export.data`.
    fn create_1pux_file(json: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".1pux")
            .tempfile()
            .expect("create temp 1pux");

        write_export_data_to_zip(&mut f, json);
        f
    }

    /// Write `export.data` into a ZIP archive at the given file handle.
    fn write_export_data_to_zip(file: &mut tempfile::NamedTempFile, json: &str) {
        file.rewind().expect("rewind");
        let mut zip_writer = zip::ZipWriter::new(file.as_file_mut());
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip_writer
            .start_file("export.data", options)
            .expect("start_file");
        zip_writer.write_all(json.as_bytes()).expect("write_all");
        zip_writer.finish().expect("finish zip");
    }

    /// Create a .1pux file without an `export.data` entry.
    fn create_1pux_without_export_data() -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".1pux")
            .tempfile()
            .expect("create temp 1pux");

        f.rewind().expect("rewind");
        let mut zip_writer = zip::ZipWriter::new(f.as_file_mut());
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip_writer
            .start_file("other_file.txt", options)
            .expect("start_file");
        zip_writer.write_all(b"not export data").expect("write");
        zip_writer.finish().expect("finish zip");
        f
    }

    /// Sample JSON with 3 login items.
    fn three_login_items_json() -> &'static str {
        r#"{
            "accounts": [{
                "vaults": [{
                    "items": [
                        {
                            "categoryUuid": "001",
                            "title": "Gmail",
                            "login": {
                                "username": "user1@gmail.com",
                                "password": "pass1",
                                "urls": [{"href": "https://gmail.com"}]
                            },
                            "notesPlain": "Personal email",
                            "tags": ["email"]
                        },
                        {
                            "categoryUuid": "001",
                            "title": "GitHub",
                            "login": {
                                "username": "dev@github.com",
                                "password": "pass2",
                                "urls": [{"href": "https://github.com"}]
                            },
                            "notesPlain": "Code repo"
                        },
                        {
                            "categoryUuid": "001",
                            "title": "AWS",
                            "login": {
                                "username": "admin@aws.com",
                                "password": "pass3",
                                "urls": [{"href": "https://aws.amazon.com"}]
                            },
                            "notesPlain": "Cloud account"
                        }
                    ]
                }]
            }]
        }"#
    }

    // -- Test 1: Normal .1pux with 3 login items → 3 ParsedItems ---------------

    #[test]
    fn three_login_items_produce_correct_parsed_items() {
        let f = create_1pux_file(three_login_items_json());
        let parser = OnePuxParser;

        let items = parser
            .parse(f.path(), None, None)
            .expect("parse should succeed");

        assert_eq!(items.len(), 3);

        assert_eq!(items[0].source_id, "1p-0");
        assert_eq!(items[0].fields.get("name").unwrap(), "Gmail");
        assert_eq!(items[0].fields.get("username").unwrap(), "user1@gmail.com");
        assert_eq!(items[0].fields.get("password").unwrap(), "pass1");
        assert_eq!(items[0].fields.get("url").unwrap(), "https://gmail.com");
        assert_eq!(items[0].fields.get("notes").unwrap(), "Personal email");
        assert_eq!(items[0].tags, vec!["email"]);

        assert_eq!(items[1].source_id, "1p-1");
        assert_eq!(items[1].fields.get("name").unwrap(), "GitHub");

        assert_eq!(items[2].source_id, "1p-2");
        assert_eq!(items[2].fields.get("name").unwrap(), "AWS");
    }

    // -- Test 2: Category filtering ------------------------------------------

    #[test]
    fn category_filtering_includes_supported_skips_others() {
        let json = r#"{
            "accounts": [{
                "vaults": [{
                    "items": [
                        {
                            "categoryUuid": "001",
                            "title": "Login Item",
                            "login": {"username": "u1", "password": "p1"}
                        },
                        {
                            "categoryUuid": "005",
                            "title": "Password Item",
                            "login": {"username": "u2", "password": "p2"}
                        },
                        {
                            "categoryUuid": "110",
                            "title": "Secure Note",
                            "notesPlain": "secret info"
                        },
                        {
                            "categoryUuid": "002",
                            "title": "Credit Card",
                            "notesPlain": "4111..."
                        }
                    ]
                }]
            }]
        }"#;
        let f = create_1pux_file(json);
        let parser = OnePuxParser;

        let items = parser
            .parse(f.path(), None, None)
            .expect("parse should succeed");

        assert_eq!(items.len(), 3, "only 001, 005, 110 should be included");
        assert_eq!(items[0].fields.get("name").unwrap(), "Login Item");
        assert_eq!(items[1].fields.get("name").unwrap(), "Password Item");
        assert_eq!(items[2].fields.get("name").unwrap(), "Secure Note");
    }

    // -- Test 3: URL extraction — first href used ----------------------------

    #[test]
    fn multiple_urls_uses_first_href() {
        let json = r#"{
            "accounts": [{
                "vaults": [{
                    "items": [{
                        "categoryUuid": "001",
                        "title": "Multi-URL",
                        "login": {
                            "username": "user",
                            "password": "pass",
                            "urls": [
                                {"href": "https://primary.com"},
                                {"href": "https://secondary.com"},
                                {"href": "https://tertiary.com"}
                            ]
                        }
                    }]
                }]
            }]
        }"#;
        let f = create_1pux_file(json);
        let parser = OnePuxParser;

        let items = parser
            .parse(f.path(), None, None)
            .expect("parse should succeed");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].fields.get("url").unwrap(), "https://primary.com");
    }

    // -- Test 4: Empty items → empty Vec -------------------------------------

    #[test]
    fn empty_vault_returns_empty_vec() {
        let json = r#"{
            "accounts": [{
                "vaults": [{
                    "items": []
                }]
            }]
        }"#;
        let f = create_1pux_file(json);
        let parser = OnePuxParser;

        let items = parser
            .parse(f.path(), None, None)
            .expect("parse should succeed");

        assert!(items.is_empty());
    }

    // -- Test 5: Missing export.data → ParseError ----------------------------

    #[test]
    fn missing_export_data_returns_parse_error() {
        let f = create_1pux_without_export_data();
        let parser = OnePuxParser;

        let result = parser.parse(f.path(), None, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ImportExportError::ParseError { .. }),
            "expected ParseError, got: {err:?}"
        );
    }

    // -- Test 6: OpVault stub returns UnsupportedFormat ----------------------

    #[test]
    fn opvault_parser_returns_unsupported_format() {
        let parser = OpVaultParser;
        let dir = tempfile::tempdir().expect("tempdir");

        let result = parser.parse(dir.path(), None, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ImportExportError::UnsupportedFormat(ref msg) if msg.contains("opvault")),
            "expected UnsupportedFormat mentioning opvault, got: {err:?}"
        );
    }

    // -- Test 7: format() returns correct ImportSource -----------------------

    #[test]
    fn format_returns_correct_import_source() {
        let onepux = OnePuxParser;
        assert_eq!(onepux.format(), ImportSource::OnePassword1pux);

        let opvault = OpVaultParser;
        assert_eq!(opvault.format(), ImportSource::OnePasswordOpvault);
    }

    // -- Test 8: validate_file with .1pux extension → Ok --------------------

    #[test]
    fn validate_file_1pux_extension_returns_ok() {
        let f = create_1pux_file("{}");
        let parser = OnePuxParser;
        assert!(parser.validate_file(f.path()).is_ok());
    }

    // -- Additional edge case tests -----------------------------------------

    #[test]
    fn requires_password_returns_false_for_onepux() {
        let parser = OnePuxParser;
        assert!(!parser.requires_password());
    }

    #[test]
    fn requires_password_returns_true_for_opvault() {
        let parser = OpVaultParser;
        assert!(parser.requires_password());
    }

    #[test]
    fn opvault_validate_file_nonexistent_returns_file_not_found() {
        let parser = OpVaultParser;
        let result = parser.validate_file(Path::new("/tmp/__oak_opvault_nonexistent__"));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ImportExportError::FileNotFound(_)
        ),);
    }

    #[test]
    fn opvault_validate_file_not_directory_returns_invalid_format() {
        let parser = OpVaultParser;
        let f = tempfile::NamedTempFile::new().expect("tempfile");
        let result = parser.validate_file(f.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ImportExportError::InvalidFormat(ref msg) if msg.contains("directory")),
            "expected InvalidFormat mentioning directory, got: {err:?}"
        );
    }

    #[test]
    fn opvault_validate_file_directory_returns_ok() {
        let parser = OpVaultParser;
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(parser.validate_file(dir.path()).is_ok());
    }

    #[test]
    fn missing_login_produces_empty_fields() {
        let json = r#"{
            "accounts": [{
                "vaults": [{
                    "items": [{
                        "categoryUuid": "001",
                        "title": "No Login Data"
                    }]
                }]
            }]
        }"#;
        let f = create_1pux_file(json);
        let parser = OnePuxParser;

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

    #[test]
    fn secure_note_has_empty_login_fields() {
        let json = r#"{
            "accounts": [{
                "vaults": [{
                    "items": [{
                        "categoryUuid": "110",
                        "title": "My Secret",
                        "notesPlain": "shh"
                    }]
                }]
            }]
        }"#;
        let f = create_1pux_file(json);
        let parser = OnePuxParser;

        let items = parser
            .parse(f.path(), None, None)
            .expect("parse should succeed");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].fields.get("name").unwrap(), "My Secret");
        assert_eq!(items[0].fields.get("notes").unwrap(), "shh");
        assert_eq!(items[0].fields.get("username").unwrap(), "");
        assert_eq!(items[0].fields.get("password").unwrap(), "");
        assert_eq!(items[0].fields.get("url").unwrap(), "");
    }

    #[test]
    fn tags_are_carried_over() {
        let json = r#"{
            "accounts": [{
                "vaults": [{
                    "items": [{
                        "categoryUuid": "001",
                        "title": "Tagged",
                        "login": {"username": "u", "password": "p"},
                        "tags": ["work", "important"]
                    }]
                }]
            }]
        }"#;
        let f = create_1pux_file(json);
        let parser = OnePuxParser;

        let items = parser
            .parse(f.path(), None, None)
            .expect("parse should succeed");

        assert_eq!(items[0].tags, vec!["work", "important"]);
    }

    #[test]
    fn invalid_json_returns_parse_error() {
        let f = create_1pux_file("{this is not valid json}");
        let parser = OnePuxParser;

        let result = parser.parse(f.path(), None, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ImportExportError::ParseError { ref reason, .. } if reason.contains("JSON")),
            "expected ParseError mentioning JSON, got: {err:?}"
        );
    }

    #[test]
    fn not_a_zip_returns_parse_error() {
        let mut f = tempfile::Builder::new()
            .suffix(".1pux")
            .tempfile()
            .expect("create temp");
        f.write_all(b"not a zip file").expect("write");

        // validate_file checks extension which is fine, but parse should fail.
        let parser = OnePuxParser;
        let result = parser.parse(f.path(), None, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ImportExportError::ParseError { .. }),
            "expected ParseError, got: {err:?}"
        );
    }

    #[test]
    fn validate_file_wrong_extension_returns_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("data.json");
        std::fs::write(&path, b"{}").expect("write");

        let parser = OnePuxParser;
        let result = parser.validate_file(&path);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("expected .1pux"),
            "expected extension error, got: {msg}"
        );
    }
}
