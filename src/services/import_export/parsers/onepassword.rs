//! 1Password import parsers for .1pux (ZIP+JSON) and .opvault formats.
//!
//! **OnePuxParser** — reads `.1pux` files, which are ZIP archives containing
//! an `export.data` JSON file. Only specific item categories are mapped:
//! - `"001"` (Login): all fields
//! - `"005"` (Password): mapped as Login
//! - `"003"` (Secure Note): name + notes only
//!
//! Items with `state: "archived"` are skipped.
//!
//! **OpVaultParser** — parses `.opvault` directories (delegated to opvault module).

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
// Deserialization structs for .1pux JSON (real 1Password export format)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct OnePuxExport {
    accounts: Vec<OnePuxAccount>,
}

#[derive(Deserialize)]
struct OnePuxAccount {
    vaults: Vec<OnePuxVault>,
}

#[derive(Deserialize)]
struct OnePuxVault {
    items: Vec<OnePuxItem>,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct OnePuxItem {
    uuid: Option<String>,
    categoryUuid: String,
    state: Option<String>,
    details: Option<OnePuxDetails>,
    overview: Option<OnePuxOverview>,
}

#[derive(Deserialize, Default)]
#[allow(non_snake_case)]
struct OnePuxDetails {
    #[serde(default)]
    loginFields: Vec<OnePuxLoginField>,
    #[serde(default, rename = "notesPlain")]
    notes_plain: Option<String>,
    #[serde(default)]
    sections: Vec<OnePuxSection>,
    #[serde(default)]
    password: Option<String>,
}

#[derive(Deserialize)]
struct OnePuxLoginField {
    designation: Option<String>,
    value: Option<String>,
}

#[derive(Deserialize)]
struct OnePuxOverview {
    title: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    url: Option<String>,
    #[serde(default, rename = "URLs")]
    urls: Option<Vec<OnePuxOverviewUrl>>,
}

#[derive(Deserialize)]
struct OnePuxOverviewUrl {
    url: Option<String>,
}

#[derive(Deserialize)]
struct OnePuxSection {
    #[serde(default)]
    fields: Option<Vec<OnePuxSectionField>>,
}

#[derive(Deserialize)]
struct OnePuxSectionField {
    id: Option<String>,
    title: Option<String>,
    value: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Category constants
// ---------------------------------------------------------------------------

const CATEGORY_LOGIN: &str = "001";
const CATEGORY_PASSWORD: &str = "005";
const CATEGORY_SECURE_NOTE: &str = "003";

// ---------------------------------------------------------------------------
// OnePuxParser — parses .1pux (ZIP + JSON) files
// ---------------------------------------------------------------------------

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
                    // Skip archived items.
                    if op_item.state.as_deref() == Some("archived") {
                        global_index += 1;
                        continue;
                    }

                    match op_item.categoryUuid.as_str() {
                        CATEGORY_LOGIN | CATEGORY_PASSWORD => {
                            items.push(parse_login_item(global_index, op_item));
                        }
                        CATEGORY_SECURE_NOTE => {
                            items.push(parse_secure_note_item(global_index, op_item));
                        }
                        _ => {}
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

fn parse_login_item(index: usize, item: &OnePuxItem) -> ParsedItem {
    let overview = item.overview.as_ref();
    let details = item.details.as_ref();

    let title = overview.and_then(|o| o.title.clone()).unwrap_or_default();
    let tags = overview.and_then(|o| o.tags.clone()).unwrap_or_default();
    let url = extract_url(overview);

    let mut username = String::new();
    let mut password = String::new();

    if let Some(details) = details {
        for field in &details.loginFields {
            match field.designation.as_deref() {
                Some("username") => username = field.value.clone().unwrap_or_default(),
                Some("password") => password = field.value.clone().unwrap_or_default(),
                _ => {}
            }
        }
        // Fallback: cat=005 stores password at details top level.
        if password.is_empty() {
            if let Some(ref pw) = details.password {
                password = pw.clone();
            }
        }
    }

    let notes = build_notes(details);

    let mut fields = HashMap::new();
    fields.insert("name".into(), title);
    fields.insert("username".into(), username);
    fields.insert("password".into(), password);
    if !url.is_empty() {
        fields.insert("url".into(), url);
    }
    if !notes.is_empty() {
        fields.insert("notes".into(), notes);
    }

    ParsedItem {
        source_id: item.uuid.clone().unwrap_or_else(|| format!("1p-{index}")),
        fields,
        tags,
    }
}

fn parse_secure_note_item(index: usize, item: &OnePuxItem) -> ParsedItem {
    let overview = item.overview.as_ref();
    let details = item.details.as_ref();

    let title = overview.and_then(|o| o.title.clone()).unwrap_or_default();
    let tags = overview.and_then(|o| o.tags.clone()).unwrap_or_default();
    let notes = build_notes(details);

    let mut fields = HashMap::new();
    fields.insert("name".into(), title);
    if !notes.is_empty() {
        fields.insert("notes".into(), notes);
    }

    ParsedItem {
        source_id: item.uuid.clone().unwrap_or_else(|| format!("1p-{index}")),
        fields,
        tags,
    }
}

/// Extract the primary URL from overview.
fn extract_url(overview: Option<&OnePuxOverview>) -> String {
    overview
        .and_then(|o| {
            o.url
                .as_deref()
                .filter(|u| !u.is_empty())
                .map(String::from)
                .or_else(|| {
                    o.urls
                        .as_ref()
                        .and_then(|urls| urls.first())
                        .and_then(|u| u.url.clone())
                })
        })
        .unwrap_or_default()
}

/// Build notes string from notesPlain + section fields (TOTP, custom fields).
fn build_notes(details: Option<&OnePuxDetails>) -> String {
    let details = match details {
        Some(d) => d,
        None => return String::new(),
    };

    let mut parts = Vec::new();

    if let Some(ref notes) = details.notes_plain {
        if !notes.is_empty() {
            parts.push(notes.clone());
        }
    }

    for section in &details.sections {
        if let Some(ref fields) = section.fields {
            for sf in fields {
                if let Some(val_str) = extract_section_value(&sf.value) {
                    if val_str.is_empty() {
                        continue;
                    }
                    if let Some(ref id) = sf.id {
                        if id.starts_with("TOTP_") {
                            parts.push(format!("TOTP: {val_str}"));
                            continue;
                        }
                    }
                    if let Some(ref title) = sf.title {
                        parts.push(format!("{title}: {val_str}"));
                    }
                }
            }
        }
    }

    parts.join("\n")
}

/// Extract a string value from a section field's JSON value.
/// Handles both plain strings and objects like {"totp": "..."}, {"string": "..."}.
fn extract_section_value(value: &Option<serde_json::Value>) -> Option<String> {
    let val = value.as_ref()?;
    match val {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(map) => {
            // Try known keys: "totp", "string", "concealed", "phone".
            for key in &["totp", "string", "concealed", "phone"] {
                if let Some(v) = map.get(*key) {
                    if let Some(s) = v.as_str() {
                        return Some(s.to_string());
                    }
                }
            }
            // Fallback: take first string value.
            for v in map.values() {
                if let Some(s) = v.as_str() {
                    return Some(s.to_string());
                }
                if let Some(inner) = v.as_object() {
                    for iv in inner.values() {
                        if let Some(s) = iv.as_str() {
                            return Some(s.to_string());
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// OpVaultParser — 1Password .opvault format parser
// ---------------------------------------------------------------------------

/// Parser for 1Password `.opvault` format (local vault directory).
///
/// Decrypts entries using PBKDF2-HMAC-SHA512 + AES-256-CBC and converts
/// to `ParsedItem`. Supports Login (001), Password (005), and Secure Note (003).
pub struct OpVaultParser;

impl FormatParser for OpVaultParser {
    fn format(&self) -> ImportSource {
        ImportSource::OnePasswordOpvault
    }

    fn parse(
        &self,
        path: &Path,
        password: Option<&SecureStr>,
        _csv_mapping: Option<&CsvColumnMapping>,
    ) -> Result<Vec<ParsedItem>, ImportExportError> {
        super::opvault::parser::parse_opvault(path, password)
    }

    fn requires_password(&self) -> bool {
        true
    }

    fn validate_file(&self, path: &Path) -> Result<(), ImportExportError> {
        if !path.exists() {
            return Err(ImportExportError::FileNotFound(path.to_path_buf()));
        }
        if !path.is_dir() {
            return Err(ImportExportError::InvalidFormat(
                "expected .opvault directory".to_string(),
            ));
        }
        let default_dir = path.join("default");
        if !default_dir.is_dir() {
            return Err(ImportExportError::InvalidFormat(
                "expected .opvault/default/ directory".to_string(),
            ));
        }
        if !default_dir.join("profile.js").exists() {
            return Err(ImportExportError::InvalidFormat(
                "expected .opvault/default/profile.js".to_string(),
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

    /// Sample JSON with 3 login items (real .1pux format).
    fn three_login_items_json() -> &'static str {
        r#"{
            "accounts": [{
                "vaults": [{
                    "items": [
                        {
                            "uuid": "uuid-gmail",
                            "categoryUuid": "001",
                            "state": "active",
                            "overview": {
                                "title": "Gmail",
                                "tags": ["email"],
                                "url": "https://gmail.com"
                            },
                            "details": {
                                "loginFields": [
                                    {"designation": "username", "value": "user1@gmail.com"},
                                    {"designation": "password", "value": "pass1"}
                                ],
                                "notesPlain": "Personal email"
                            }
                        },
                        {
                            "uuid": "uuid-github",
                            "categoryUuid": "001",
                            "state": "active",
                            "overview": {
                                "title": "GitHub",
                                "url": "https://github.com"
                            },
                            "details": {
                                "loginFields": [
                                    {"designation": "username", "value": "dev@github.com"},
                                    {"designation": "password", "value": "pass2"}
                                ],
                                "notesPlain": "Code repo"
                            }
                        },
                        {
                            "uuid": "uuid-aws",
                            "categoryUuid": "001",
                            "state": "active",
                            "overview": {
                                "title": "AWS",
                                "url": "https://aws.amazon.com"
                            },
                            "details": {
                                "loginFields": [
                                    {"designation": "username", "value": "admin@aws.com"},
                                    {"designation": "password", "value": "pass3"}
                                ],
                                "notesPlain": "Cloud account"
                            }
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

        assert_eq!(items[0].source_id, "uuid-gmail");
        assert_eq!(items[0].fields.get("name").unwrap(), "Gmail");
        assert_eq!(items[0].fields.get("username").unwrap(), "user1@gmail.com");
        assert_eq!(items[0].fields.get("password").unwrap(), "pass1");
        assert_eq!(items[0].fields.get("url").unwrap(), "https://gmail.com");
        assert_eq!(items[0].fields.get("notes").unwrap(), "Personal email");
        assert_eq!(items[0].tags, vec!["email"]);

        assert_eq!(items[1].source_id, "uuid-github");
        assert_eq!(items[1].fields.get("name").unwrap(), "GitHub");

        assert_eq!(items[2].source_id, "uuid-aws");
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
                            "uuid": "u1",
                            "categoryUuid": "001",
                            "overview": {"title": "Login Item"},
                            "details": {
                                "loginFields": [
                                    {"designation": "username", "value": "u1"},
                                    {"designation": "password", "value": "p1"}
                                ]
                            }
                        },
                        {
                            "uuid": "u2",
                            "categoryUuid": "005",
                            "overview": {"title": "Password Item"},
                            "details": {
                                "loginFields": [
                                    {"designation": "username", "value": "u2"},
                                    {"designation": "password", "value": "p2"}
                                ]
                            }
                        },
                        {
                            "uuid": "u3",
                            "categoryUuid": "003",
                            "overview": {"title": "Secure Note"},
                            "details": {"notesPlain": "secret info"}
                        },
                        {
                            "uuid": "u4",
                            "categoryUuid": "002",
                            "overview": {"title": "Credit Card"},
                            "details": {"notesPlain": "4111..."}
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

        assert_eq!(items.len(), 3, "only 001, 005, 003 should be included");
        assert_eq!(items[0].fields.get("name").unwrap(), "Login Item");
        assert_eq!(items[1].fields.get("name").unwrap(), "Password Item");
        assert_eq!(items[2].fields.get("name").unwrap(), "Secure Note");
    }

    // -- Test 3: URL extraction — overview.url preferred ----------------------

    #[test]
    fn multiple_urls_uses_overview_url() {
        let json = r#"{
            "accounts": [{
                "vaults": [{
                    "items": [{
                        "uuid": "u1",
                        "categoryUuid": "001",
                        "overview": {
                            "title": "Multi-URL",
                            "url": "https://primary.com",
                            "URLs": [
                                {"url": "https://primary.com"},
                                {"url": "https://secondary.com"}
                            ]
                        },
                        "details": {
                            "loginFields": [
                                {"designation": "username", "value": "user"},
                                {"designation": "password", "value": "pass"}
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

    // -- Test 6: OpVault requires password ----------------------------------

    #[test]
    fn opvault_parser_returns_password_required() {
        let parser = OpVaultParser;
        let dir = tempfile::tempdir().expect("tempdir");

        let result = parser.parse(dir.path(), None, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ImportExportError::PasswordRequired),
            "expected PasswordRequired, got: {err:?}"
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
    fn opvault_validate_file_directory_with_profile_returns_ok() {
        let parser = OpVaultParser;
        let dir = tempfile::tempdir().expect("tempdir");
        let default_dir = dir.path().join("default");
        std::fs::create_dir_all(&default_dir).expect("create default dir");
        std::fs::write(default_dir.join("profile.js"), "").expect("write profile.js");
        assert!(parser.validate_file(dir.path()).is_ok());
    }

    #[test]
    fn missing_login_produces_empty_fields() {
        let json = r#"{
            "accounts": [{
                "vaults": [{
                    "items": [{
                        "uuid": "u1",
                        "categoryUuid": "001",
                        "overview": {"title": "No Login Data"},
                        "details": {"loginFields": []}
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
        assert!(!items[0].fields.contains_key("url"));
        assert!(!items[0].fields.contains_key("notes"));
    }

    #[test]
    fn secure_note_has_name_and_notes_only() {
        let json = r#"{
            "accounts": [{
                "vaults": [{
                    "items": [{
                        "uuid": "u1",
                        "categoryUuid": "003",
                        "overview": {"title": "My Secret"},
                        "details": {"notesPlain": "shh"}
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
        assert!(!items[0].fields.contains_key("username"));
        assert!(!items[0].fields.contains_key("password"));
        assert!(!items[0].fields.contains_key("url"));
    }

    #[test]
    fn tags_are_carried_over() {
        let json = r#"{
            "accounts": [{
                "vaults": [{
                    "items": [{
                        "uuid": "u1",
                        "categoryUuid": "001",
                        "overview": {
                            "title": "Tagged",
                            "tags": ["work", "important"]
                        },
                        "details": {
                            "loginFields": [
                                {"designation": "username", "value": "u"},
                                {"designation": "password", "value": "p"}
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

    #[test]
    fn archived_items_are_skipped() {
        let json = r#"{
            "accounts": [{
                "vaults": [{
                    "items": [
                        {
                            "uuid": "u1",
                            "categoryUuid": "001",
                            "state": "active",
                            "overview": {"title": "Active"},
                            "details": {
                                "loginFields": [
                                    {"designation": "username", "value": "u"},
                                    {"designation": "password", "value": "p"}
                                ]
                            }
                        },
                        {
                            "uuid": "u2",
                            "categoryUuid": "001",
                            "state": "archived",
                            "overview": {"title": "Archived"},
                            "details": {
                                "loginFields": [
                                    {"designation": "username", "value": "u"},
                                    {"designation": "password", "value": "p"}
                                ]
                            }
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

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].fields.get("name").unwrap(), "Active");
    }

    #[test]
    fn password_item_uses_details_password_field() {
        let json = r#"{
            "accounts": [{
                "vaults": [{
                    "items": [{
                        "uuid": "u1",
                        "categoryUuid": "005",
                        "overview": {"title": "Standalone Password"},
                        "details": {
                            "loginFields": [],
                            "password": "secret123"
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
        assert_eq!(items[0].fields.get("name").unwrap(), "Standalone Password");
        assert_eq!(items[0].fields.get("password").unwrap(), "secret123");
    }

    #[test]
    fn real_1pux_parse() {
        let path = std::path::Path::new("tests/data/1PasswordExport.1pux");
        if !path.exists() {
            eprintln!("Skipping: test data not found at {:?}", path);
            return;
        }
        let parser = OnePuxParser;
        let items = parser.parse(path, None, None).expect("parse 1pux");

        assert_eq!(items.len(), 3, "expected 3 items, got {}", items.len());

        let by_id: std::collections::HashMap<&str, &ParsedItem> =
            items.iter().map(|i| (i.source_id.as_str(), i)).collect();

        // cat=001 Login — active, with TOTP in sections
        let login = by_id.get("55qbj3r6ftglrawryv46obdotq").expect("Login item");
        assert_eq!(login.fields.get("name").unwrap(), "Login");
        assert_eq!(login.fields.get("username").unwrap(), "user@example.com");
        assert_eq!(login.fields.get("password").unwrap(), "password");
        assert_eq!(login.fields.get("url").unwrap(), "https://example.com");
        let notes = login.fields.get("notes").unwrap();
        assert!(notes.contains("Note to self"), "notes: {notes}");
        assert!(
            notes.contains("TOTP:"),
            "notes should contain TOTP: {notes}"
        );
        assert_eq!(login.tags, vec!["website"]);

        // cat=005 Password — no uuid in data, source_id falls back to 1p-1
        let pw = items
            .iter()
            .find(|i| i.fields.get("name").unwrap() == "UUID 005 Password")
            .expect("Password item");
        assert_eq!(pw.fields.get("password").unwrap(), "uuid005password");
        assert_eq!(pw.fields.get("username").unwrap(), "");
        assert!(!pw.fields.contains_key("url"));

        // cat=003 Secure Note
        let note = by_id
            .get("fkfngfia2rvrrmtcbs5xvwud3i")
            .expect("Secure Note item");
        assert_eq!(note.fields.get("name").unwrap(), "Secure Note");
        assert_eq!(note.fields.get("notes").unwrap(), "This is a note");
        assert!(!note.fields.contains_key("username"));
        assert!(!note.fields.contains_key("password"));
        assert_eq!(note.tags, vec!["Note", "Secret Stuff"]);

        // Archived item NOT included
        assert!(
            by_id.get("kbdznu56agh3tucxbswi72hpzq").is_none(),
            "archived item should be skipped"
        );
    }

    #[test]
    fn _dump_real_1pux() {
        let path = std::path::Path::new("tests/data/1PasswordExport.1pux");
        if !path.exists() {
            eprintln!("Skipping: test data not found at {:?}", path);
            return;
        }
        let parser = OnePuxParser;
        let items = parser.parse(path, None, None).expect("parse 1pux");

        println!("\n=== 1PasswordExport.1pux 解析结果 ===");
        println!("总条目数: {}\n", items.len());
        for item in &items {
            println!("[{}] {{", item.source_id);
            let mut keys: Vec<&String> = item.fields.keys().collect();
            keys.sort();
            for key in keys {
                let val = &item.fields[key];
                if key == "notes" {
                    println!("  notes: |");
                    for line in val.lines() {
                        println!("    {}", line);
                    }
                } else {
                    println!("  {}: {}", key, val);
                }
            }
            if !item.tags.is_empty() {
                println!("  tags: {:?}", item.tags);
            }
            println!("}}\n");
        }
    }
}
