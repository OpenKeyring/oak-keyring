use std::collections::HashMap;
use std::path::Path;

use csv::ReaderBuilder;

use crate::commands::types::{CsvColumnMapping, ImportSource};
use crate::errors::mapping::import_export::ImportExportError;
use crate::services::import_export::parser::{validate_file_common, FormatParser, ParsedItem};
use crate::types::SecureStr;

// ---------------------------------------------------------------------------
// CsvParser — parses CSV files using user-provided column mapping
// ---------------------------------------------------------------------------

/// Parser for plain-text CSV files.
///
/// CSV files are not encrypted, so the `password` parameter is ignored.
/// A [`CsvColumnMapping`] must be provided so the parser knows which columns
/// map to name, username, password, url, notes, and optional tags.
pub struct CsvParser;

impl FormatParser for CsvParser {
    fn format(&self) -> ImportSource {
        ImportSource::Csv
    }

    fn parse(
        &self,
        path: &Path,
        password: Option<&SecureStr>,
        csv_mapping: Option<&CsvColumnMapping>,
    ) -> Result<Vec<ParsedItem>, ImportExportError> {
        // CSV is never encrypted — password is not applicable.
        let _ = password;

        // Column mapping is required for CSV parsing.
        let mapping = csv_mapping.ok_or_else(|| {
            ImportExportError::InvalidFormat("CSV column mapping required".to_string())
        })?;

        // Build a CSV reader. `has_headers` controls whether the first row is
        // treated as a header or as data.
        let mut rdr = ReaderBuilder::new()
            .has_headers(mapping.skip_header)
            .from_path(path)
            .map_err(|e| ImportExportError::FileReadError {
                path: path.to_path_buf(),
                reason: e.to_string(),
            })?;

        // Collect header names into a Vec for index resolution.
        let headers = rdr.headers().map_err(|e| ImportExportError::ParseError {
            format: "CSV".to_string(),
            reason: format!("failed to read headers: {e}"),
        })?;

        let header_vec: Vec<String> = headers.iter().map(|s| s.to_string()).collect();

        // Resolve column indices from the mapping.
        let name_idx = resolve_column(&header_vec, &mapping.name_column, "name")?;
        let username_idx = resolve_column(&header_vec, &mapping.username_column, "username")?;
        let password_idx = resolve_column(&header_vec, &mapping.password_column, "password")?;
        let url_idx = resolve_column(&header_vec, &mapping.url_column, "url")?;
        let notes_idx = resolve_column(&header_vec, &mapping.notes_column, "notes")?;
        let tags_idx = mapping
            .tags_column
            .as_ref()
            .and_then(|col| find_column(&header_vec, col));
        let totp_idx = mapping
            .totp_column
            .as_ref()
            .and_then(|col| find_column(&header_vec, col));

        // Iterate over data rows and build ParsedItems.
        let mut items = Vec::new();
        let mut row_number: usize = 0;

        let mut record = csv::StringRecord::new();
        while rdr
            .read_record(&mut record)
            .map_err(|e| ImportExportError::FileReadError {
                path: path.to_path_buf(),
                reason: e.to_string(),
            })?
        {
            row_number += 1;

            let mut fields = build_fields(
                &record,
                name_idx,
                username_idx,
                password_idx,
                url_idx,
                notes_idx,
            );
            // TOTP is an optional login field; only populated when a totp column
            // is mapped and the cell is non-empty, mirroring the structured
            // `fields["totp"]` key the executor reads on import.
            if let Some(totp) = extract_optional(&record, totp_idx) {
                fields.insert("totp".to_string(), totp);
            }

            let tags = extract_tags(&record, tags_idx);

            items.push(ParsedItem {
                source_id: format!("row-{row_number}"),
                fields,
                tags,
            });
        }

        Ok(items)
    }

    fn requires_password(&self) -> bool {
        false
    }

    fn validate_file(&self, path: &Path) -> Result<(), ImportExportError> {
        validate_file_common(path, "csv")
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Find the index of `column_name` in `headers`, returning a descriptive
/// `ParseError` if not found.
fn resolve_column(
    headers: &[String],
    column_name: &str,
    field_label: &str,
) -> Result<usize, ImportExportError> {
    find_column(headers, column_name).ok_or_else(|| ImportExportError::ParseError {
        format: "CSV".to_string(),
        reason: format!(
            "column '{}' for field '{}' not found in CSV headers",
            column_name, field_label
        ),
    })
}

/// Search for a column name in headers (case-sensitive).
fn find_column(headers: &[String], column_name: &str) -> Option<usize> {
    headers.iter().position(|h| h == column_name)
}

/// Extract field values from a CSV record at the resolved column indices.
fn build_fields(
    record: &csv::StringRecord,
    name_idx: usize,
    username_idx: usize,
    password_idx: usize,
    url_idx: usize,
    notes_idx: usize,
) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    fields.insert("name".to_string(), get_or_empty(record, name_idx));
    fields.insert("username".to_string(), get_or_empty(record, username_idx));
    fields.insert("password".to_string(), get_or_empty(record, password_idx));
    fields.insert("url".to_string(), get_or_empty(record, url_idx));
    fields.insert("notes".to_string(), get_or_empty(record, notes_idx));
    fields
}

/// Get the value at `idx` from the record, or an empty string if out of bounds.
fn get_or_empty(record: &csv::StringRecord, idx: usize) -> String {
    record.get(idx).unwrap_or("").to_string()
}

/// Get an optional field value from a CSV record: returns `Some(value)` only
/// when the column index is present and the cell is non-empty.
fn extract_optional(record: &csv::StringRecord, idx: Option<usize>) -> Option<String> {
    let idx = idx?;
    let raw = record.get(idx)?;
    if raw.is_empty() {
        None
    } else {
        Some(raw.to_string())
    }
}

/// Extract tags from a CSV record if the tags column exists and has content.
/// Tags are split by comma and trimmed of whitespace.
fn extract_tags(record: &csv::StringRecord, tags_idx: Option<usize>) -> Vec<String> {
    let Some(idx) = tags_idx else {
        return Vec::new();
    };
    let Some(raw) = record.get(idx) else {
        return Vec::new();
    };
    if raw.is_empty() {
        return Vec::new();
    }
    raw.split(',')
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    use crate::services::import_export::export::{write_csv, ExportPayload, ExportRecord};

    /// Helper: create a temporary CSV file with the given content.
    fn create_csv_file(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".csv")
            .tempfile()
            .expect("create temp csv");
        f.write_all(content.as_bytes()).expect("write csv");
        f
    }

    /// Helper: default CsvColumnMapping with standard column names.
    fn default_mapping() -> CsvColumnMapping {
        CsvColumnMapping {
            name_column: "name".to_string(),
            username_column: "username".to_string(),
            password_column: "password".to_string(),
            url_column: "url".to_string(),
            notes_column: "notes".to_string(),
            totp_column: None,
            tags_column: None,
            skip_header: true,
        }
    }

    // -- Round-trip: CSV export -> import preserves login totp ----------------

    #[test]
    fn csv_round_trip_preserves_login_totp() {
        // A Login's TOTP secret must survive a CSV export -> import cycle:
        // write_csv emits a `totp` column, and the parser reads it back into
        // the structured `fields["totp"]` key the executor consumes.
        let totp_uri = "otpauth://totp/GitHub:alice?secret=JBSWY3DPEHPK3PXP&issuer=GitHub";
        let payload = ExportPayload {
            version: "1.0".to_string(),
            vault_id: "vault-1".to_string(),
            exported_at: "2026-07-17T00:00:00Z".to_string(),
            records: vec![ExportRecord {
                id: "rec-1".to_string(),
                credential_type: "login".to_string(),
                name: "GitHub".to_string(),
                username: Some("alice".to_string()),
                password: Some("s3cret!".to_string()),
                url: Some("https://github.com".to_string()),
                notes: None,
                totp: Some(totp_uri.to_string()),
                tags: None,
                is_favorite: None,
                expires_at: None,
                public_key: None,
                private_key: None,
                passphrase: None,
                app_id: None,
                secret_key: None,
            }],
        };

        let csv_out = tempfile::Builder::new()
            .suffix(".csv")
            .tempfile()
            .expect("create temp csv output");
        write_csv(&payload, csv_out.path()).expect("write csv");

        // Re-import with a mapping that points totp_column at the emitted column.
        let mapping = CsvColumnMapping {
            totp_column: Some("totp".to_string()),
            ..default_mapping()
        };
        let parser = CsvParser;
        let items = parser
            .parse(csv_out.path(), None, Some(&mapping))
            .expect("re-import parse should succeed");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].fields.get("name").unwrap(), "GitHub");
        assert_eq!(
            items[0].fields.get("totp").unwrap(),
            totp_uri,
            "totp must survive the CSV round-trip"
        );
    }

    // -- Test 1: Normal CSV with 3 rows --------------------------------------

    #[test]
    fn normal_csv_produces_three_parsed_items() {
        let csv = "name,username,password,url,notes\n\
                   Gmail,user1@gmail.com,pass1,https://gmail.com,Personal email\n\
                   GitHub,dev@github.com,pass2,https://github.com,Code repo\n\
                   AWS,admin@aws.com,pass3,https://aws.amazon.com,Cloud account\n";
        let f = create_csv_file(csv);
        let parser = CsvParser;
        let mapping = default_mapping();

        let items = parser
            .parse(f.path(), None, Some(&mapping))
            .expect("parse should succeed");

        assert_eq!(items.len(), 3);

        // Row 1
        assert_eq!(items[0].source_id, "row-1");
        assert_eq!(items[0].fields.get("name").unwrap(), "Gmail");
        assert_eq!(items[0].fields.get("username").unwrap(), "user1@gmail.com");
        assert_eq!(items[0].fields.get("password").unwrap(), "pass1");
        assert_eq!(items[0].fields.get("url").unwrap(), "https://gmail.com");
        assert_eq!(items[0].fields.get("notes").unwrap(), "Personal email");
        assert!(items[0].tags.is_empty());

        // Row 2
        assert_eq!(items[1].source_id, "row-2");
        assert_eq!(items[1].fields.get("name").unwrap(), "GitHub");

        // Row 3
        assert_eq!(items[2].source_id, "row-3");
        assert_eq!(items[2].fields.get("name").unwrap(), "AWS");
    }

    // -- Test 2: Skip header -------------------------------------------------

    #[test]
    fn skip_header_true_excludes_header_from_data() {
        let csv = "name,username,password,url,notes\n\
                   SiteA,a,p,u,n\n";
        let f = create_csv_file(csv);
        let parser = CsvParser;
        let mapping = default_mapping();

        let items = parser
            .parse(f.path(), None, Some(&mapping))
            .expect("parse should succeed");

        // Only one data row — header should not appear as data.
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].fields.get("name").unwrap(), "SiteA");
    }

    // -- Test 3: With tags ---------------------------------------------------

    #[test]
    fn tags_column_split_by_comma() {
        let csv = "name,username,password,url,notes,tags\n\
                   SiteA,a,p,u,n,\"work, personal\"\n";
        let f = create_csv_file(csv);
        let parser = CsvParser;
        let mut mapping = default_mapping();
        mapping.tags_column = Some("tags".to_string());

        let items = parser
            .parse(f.path(), None, Some(&mapping))
            .expect("parse should succeed");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].tags, vec!["work", "personal"]);
    }

    // -- Test 4: Empty CSV (header only) -------------------------------------

    #[test]
    fn empty_csv_returns_empty_vec() {
        let csv = "name,username,password,url,notes\n";
        let f = create_csv_file(csv);
        let parser = CsvParser;
        let mapping = default_mapping();

        let items = parser
            .parse(f.path(), None, Some(&mapping))
            .expect("parse should succeed");

        assert!(items.is_empty());
    }

    // -- Test 5: Missing column in CSV ---------------------------------------

    #[test]
    fn missing_column_returns_parse_error() {
        // CSV has no "url" column.
        let csv = "name,username,password,notes\n\
                   SiteA,a,p,n\n";
        let f = create_csv_file(csv);
        let parser = CsvParser;
        let mapping = default_mapping();

        let result = parser.parse(f.path(), None, Some(&mapping));
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("url") && msg.contains("not found"),
            "expected column-not-found error, got: {msg}"
        );
    }

    // -- Test 6: No csv_mapping provided -------------------------------------

    #[test]
    fn no_csv_mapping_returns_invalid_format() {
        let csv = "name,value\na,1\n";
        let f = create_csv_file(csv);
        let parser = CsvParser;

        let result = parser.parse(f.path(), None, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("column mapping required"),
            "expected column mapping required error, got: {msg}"
        );
    }

    // -- Test 7: UTF-8 / Chinese characters ----------------------------------

    #[test]
    fn utf8_chinese_characters_preserved() {
        let csv = "name,username,password,url,notes\n\
                   \u{90ae}\u{7bb1},user@163.com,pwd123,https://163.com,\u{4e2a}\u{4eba}\u{90ae}\u{7bb1}\n";
        let f = create_csv_file(csv);
        let parser = CsvParser;
        let mapping = default_mapping();

        let items = parser
            .parse(f.path(), None, Some(&mapping))
            .expect("parse should succeed");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].fields.get("name").unwrap(), "\u{90ae}\u{7bb1}");
        assert_eq!(
            items[0].fields.get("notes").unwrap(),
            "\u{4e2a}\u{4eba}\u{90ae}\u{7bb1}"
        );
    }

    // -- Test 8: validate_file with correct/wrong extension ------------------

    #[test]
    fn validate_file_csv_extension_returns_ok() {
        let f = create_csv_file("name\nval\n");
        let parser = CsvParser;
        assert!(parser.validate_file(f.path()).is_ok());
    }

    #[test]
    fn validate_file_wrong_extension_returns_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("data.txt");
        std::fs::write(&path, b"name\nval\n").expect("write");

        let parser = CsvParser;
        let result = parser.validate_file(&path);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("expected .csv"),
            "expected extension error, got: {msg}"
        );
    }

    // -- Additional edge case tests -----------------------------------------

    #[test]
    fn empty_tags_value_produces_empty_vec() {
        let csv = "name,username,password,url,notes,tags\n\
                   SiteA,a,p,u,n,\n";
        let f = create_csv_file(csv);
        let parser = CsvParser;
        let mut mapping = default_mapping();
        mapping.tags_column = Some("tags".to_string());

        let items = parser
            .parse(f.path(), None, Some(&mapping))
            .expect("parse should succeed");

        assert_eq!(items.len(), 1);
        assert!(items[0].tags.is_empty());
    }

    #[test]
    fn tags_column_not_found_in_headers_produces_no_tags() {
        // Mapping references a tags column that does not exist in CSV.
        let csv = "name,username,password,url,notes\n\
                   SiteA,a,p,u,n\n";
        let f = create_csv_file(csv);
        let parser = CsvParser;
        let mut mapping = default_mapping();
        mapping.tags_column = Some("nonexistent_tags".to_string());

        let items = parser
            .parse(f.path(), None, Some(&mapping))
            .expect("parse should succeed");

        assert_eq!(items.len(), 1);
        assert!(items[0].tags.is_empty());
    }

    #[test]
    fn password_provided_is_ignored_for_csv() {
        let csv = "name,username,password,url,notes\n\
                   SiteA,a,p,u,n\n";
        let f = create_csv_file(csv);
        let parser = CsvParser;
        let mapping = default_mapping();
        let password = SecureStr::new("unused".to_string());

        let result = parser.parse(f.path(), Some(&password), Some(&mapping));
        assert!(result.is_ok());
    }

    #[test]
    fn skip_header_false_treats_first_row_as_data() {
        // When skip_header is false, the CSV reader treats the first row as
        // data and there are no header names. Column indices are positional.
        // However, our implementation reads headers() which returns the first
        // row regardless, so with has_headers=false the first row is both the
        // "header" AND data. The csv crate with has_headers(false) will still
        // return the first row from headers(), and then read_record() will
        // iterate ALL rows including the first one.
        let csv = "SiteA,a,p,u,n\nSiteB,b,q,v,o\n";
        let f = create_csv_file(csv);
        let parser = CsvParser;

        // Map column names to the actual first-row values used as "headers"
        let mapping = CsvColumnMapping {
            name_column: "SiteA".to_string(),
            username_column: "a".to_string(),
            password_column: "p".to_string(),
            url_column: "u".to_string(),
            notes_column: "n".to_string(),
            totp_column: None,
            tags_column: None,
            skip_header: false,
        };

        let items = parser
            .parse(f.path(), None, Some(&mapping))
            .expect("parse should succeed");

        // With has_headers(false), the csv crate returns first row from
        // headers() and then read_record iterates ALL rows. So we get 2 items.
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].fields.get("name").unwrap(), "SiteA");
        assert_eq!(items[1].fields.get("name").unwrap(), "SiteB");
    }

    #[test]
    fn format_returns_csv() {
        let parser = CsvParser;
        assert_eq!(parser.format(), ImportSource::Csv);
    }

    #[test]
    fn requires_password_returns_false() {
        let parser = CsvParser;
        assert!(!parser.requires_password());
    }
}
