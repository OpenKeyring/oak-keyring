use std::collections::HashMap;
use std::path::Path;

use crate::commands::types::ImportSource;
use crate::errors::mapping::import_export::ImportExportError;
use crate::services::import_export::parser::{FormatParser, ParsedItem};
use crate::services::import_export::parsers::keepass::{
    build_custom_notes, entry_to_parsed_item, KeePassParser,
};
use crate::types::SecureStr;

use super::keepass::{KP_NOTES, KP_PASSWORD, KP_TITLE, KP_URL, KP_USERNAME};

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
    std::fs::write(&path, b"\x03\xd9\xa2\x9a\x67\xfb\x4b\xb5").expect("write");

    let parser = KeePassParser;
    let pw = SecureStr::new("wrong-password".into());
    let result = parser.parse(&path, Some(&pw), None);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        matches!(
            err,
            ImportExportError::InvalidPassword
                | ImportExportError::DecryptionFailed(_)
                | ImportExportError::ParseError { .. }
                | ImportExportError::InvalidFormat(_)
                | ImportExportError::FileReadError { .. }
        ),
        "expected open-related error, got: {err:?} (msg: {msg})"
    );
}

// -- Helper function tests ------------------------------------------------

/// Helper: create a Database with one entry configured by `f`.
/// Uses the keepass 0.12 public builder API since `Entry::new()` is `pub(crate)`.
fn make_entry(f: impl FnOnce(&mut keepass::db::EntryMut<'_>)) -> keepass::Database {
    let mut db = keepass::Database::new();
    db.root_mut().add_entry().edit(f);
    db
}

#[test]
fn build_custom_notes_skips_standard_fields() {
    let db = make_entry(|e| {
        e.set_unprotected(KP_TITLE, "T");
        e.set_unprotected(KP_USERNAME, "U");
        e.set_protected(KP_PASSWORD, "P");
        e.set_unprotected(KP_URL, "http://x");
        e.set_unprotected(KP_NOTES, "N");
    });
    let entry = db.entry(db.root().entry_ids().next().unwrap()).unwrap();

    let standard: &[&str] = &[KP_TITLE, KP_USERNAME, KP_PASSWORD, KP_URL, KP_NOTES];
    let result = build_custom_notes(&entry, standard);
    assert!(result.is_empty(), "expected no custom notes, got: {result}");
}

#[test]
fn build_custom_notes_empty_when_all_standard() {
    let db = make_entry(|e| {
        e.set_unprotected(KP_TITLE, "T");
    });
    let entry = db.entry(db.root().entry_ids().next().unwrap()).unwrap();

    let standard: &[&str] = &[KP_TITLE, KP_USERNAME, KP_PASSWORD, KP_URL, KP_NOTES];
    let result = build_custom_notes(&entry, standard);
    assert!(result.is_empty());
}

#[test]
fn build_custom_notes_includes_non_standard_fields() {
    let db = make_entry(|e| {
        e.set_unprotected(KP_TITLE, "T");
        e.set_unprotected("CustomField1", "val1");
        e.set_unprotected("CustomField2", "val2");
    });
    let entry = db.entry(db.root().entry_ids().next().unwrap()).unwrap();

    let standard: &[&str] = &[KP_TITLE, KP_USERNAME, KP_PASSWORD, KP_URL, KP_NOTES];
    let result = build_custom_notes(&entry, standard);
    assert!(
        result.contains("Custom: CustomField1 = val1"),
        "got: {result}"
    );
    assert!(
        result.contains("Custom: CustomField2 = val2"),
        "got: {result}"
    );
}

#[test]
fn entry_to_parsed_item_extracts_standard_fields() {
    let db = make_entry(|e| {
        e.set_unprotected(KP_TITLE, "MyTitle");
        e.set_unprotected(KP_USERNAME, "user@test.com");
        e.set_protected(KP_PASSWORD, "s3cret");
        e.set_unprotected(KP_URL, "https://example.com");
        e.set_unprotected(KP_NOTES, "some notes");
    });
    let entry = db.entry(db.root().entry_ids().next().unwrap()).unwrap();

    let item = entry_to_parsed_item(&entry, "");
    assert_eq!(item.fields.get("name").unwrap(), "MyTitle");
    assert_eq!(item.fields.get("username").unwrap(), "user@test.com");
    assert_eq!(item.fields.get("password").unwrap(), "s3cret");
    assert_eq!(item.fields.get("url").unwrap(), "https://example.com");
    assert_eq!(item.fields.get("notes").unwrap(), "some notes");
}

#[test]
fn entry_to_parsed_item_root_group_has_no_tags() {
    let db = make_entry(|e| {
        e.set_unprotected(KP_TITLE, "Root Entry");
    });
    let entry = db.entry(db.root().entry_ids().next().unwrap()).unwrap();

    let item = entry_to_parsed_item(&entry, "");
    assert!(item.tags.is_empty(), "root group path yields no tags");
}

#[test]
fn entry_to_parsed_item_group_path_becomes_tags() {
    let db = make_entry(|e| {
        e.set_unprotected(KP_TITLE, "Nested");
    });
    let entry = db.entry(db.root().entry_ids().next().unwrap()).unwrap();

    let item = entry_to_parsed_item(&entry, "Finance/Banking");
    assert_eq!(item.tags, vec!["Finance", "Banking"]);
}

#[test]
fn entry_to_parsed_item_tags_deduplicated_with_group_path() {
    let db = make_entry(|e| {
        e.set_unprotected(KP_TITLE, "Dup");
        e.tags = vec!["Finance".to_string(), "Extra".to_string()];
    });
    let entry = db.entry(db.root().entry_ids().next().unwrap()).unwrap();

    let item = entry_to_parsed_item(&entry, "Finance");
    assert_eq!(item.tags, vec!["Finance", "Extra"]);
}

#[test]
fn entry_to_parsed_item_custom_fields_appended_to_notes() {
    let db = make_entry(|e| {
        e.set_unprotected(KP_TITLE, "Custom");
        e.set_unprotected(KP_NOTES, "base note");
        e.set_unprotected("API Key", "abc123");
    });
    let entry = db.entry(db.root().entry_ids().next().unwrap()).unwrap();

    let item = entry_to_parsed_item(&entry, "");
    let notes = item.fields.get("notes").unwrap();
    assert!(notes.starts_with("base note"), "got: {notes}");
    assert!(notes.contains("Custom: API Key = abc123"), "got: {notes}");
}

#[test]
fn entry_to_parsed_item_protected_field_is_unprotected() {
    let db = make_entry(|e| {
        e.set_unprotected(KP_TITLE, "P");
        e.set_protected(KP_PASSWORD, "p@ssw0rd");
    });
    let entry = db.entry(db.root().entry_ids().next().unwrap()).unwrap();

    let item = entry_to_parsed_item(&entry, "");
    assert_eq!(item.fields.get("password").unwrap(), "p@ssw0rd");
}

// -- Batch scan -----------------------------------------------------------

/// Batch scan: try parsing every .kdbx file in tests/data/ with password "a".
#[test]
fn _dump_all_kdbx_files() {
    let data_dir = std::path::Path::new("tests/data");
    if !data_dir.exists() {
        eprintln!("Skipping: tests/data not found");
        return;
    }

    let pw = SecureStr::new("a".to_string());
    let parser = KeePassParser;

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(data_dir)
        .expect("read_dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            ext == "kdbx" || ext == "kdb"
        })
        .collect();
    files.sort();

    println!("\n=== KeePass .kdbx batch scan (password='a') ===\n");

    let mut ok_count = 0;
    let mut err_count = 0;

    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy();
        match parser.parse(path, Some(&pw), None) {
            Ok(items) => {
                ok_count += 1;
                println!("[OK] {} — {} entries", name, items.len());
                for item in &items {
                    let title = item
                        .fields
                        .get("name")
                        .map(|s| s.as_str())
                        .unwrap_or("<empty>");
                    let user = item
                        .fields
                        .get("username")
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    let tags = if item.tags.is_empty() {
                        String::new()
                    } else {
                        format!(" tags={:?}", item.tags)
                    };
                    if !user.is_empty() {
                        println!("      {} (user: {}){}", title, user, tags);
                    } else {
                        println!("      {}{}", title, tags);
                    }
                }
            }
            Err(e) => {
                err_count += 1;
                println!("[ERR] {} — {}", name, e);
            }
        }
    }

    println!(
        "\n=== Summary: {} OK, {} ERR, {} total ===",
        ok_count,
        err_count,
        files.len()
    );
}

// -- Integration tests with real .kdbx files (password "a") ----------------

fn parse_kdbx(name: &str) -> Vec<ParsedItem> {
    let path = std::path::Path::new("tests/data").join(name);
    if !path.exists() {
        eprintln!("Skipping: {} not found", name);
        return Vec::new();
    }
    let pw = SecureStr::new("a".to_string());
    KeePassParser
        .parse(&path, Some(&pw), None)
        .unwrap_or_else(|e| panic!("failed to parse {}: {}", name, e))
}

#[test]
fn test_format300_kdbx() {
    let items = parse_kdbx("Format300.kdbx");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].fields.get("name").unwrap(), "Sample Entry");
    assert_eq!(items[0].fields.get("username").unwrap(), "User Name");
    assert_eq!(items[0].fields.get("password").unwrap(), "Password");
    assert_eq!(
        items[0].fields.get("url").unwrap(),
        "http://www.somesite.com/"
    );
}

#[test]
fn test_new_database_kdbx() {
    let items = parse_kdbx("NewDatabase.kdbx");
    assert_eq!(items.len(), 2);

    let by_name: HashMap<&str, &ParsedItem> = items
        .iter()
        .map(|i| (i.fields.get("name").unwrap().as_str(), i))
        .collect();

    let sample = by_name.get("Sample Entry").expect("Sample Entry");
    assert_eq!(sample.fields.get("username").unwrap(), "User Name");

    let sub = by_name.get("Subgroup Entry").expect("Subgroup Entry");
    assert_eq!(sub.fields.get("username").unwrap(), "Bank User Name");
    assert_eq!(sub.tags, vec!["Homebanking", "Subgroup"]);
}

#[test]
fn test_new_database2_kdbx() {
    let items = parse_kdbx("NewDatabase2.kdbx");
    assert_eq!(items.len(), 2);

    let by_name: HashMap<&str, &ParsedItem> = items
        .iter()
        .map(|i| (i.fields.get("name").unwrap().as_str(), i))
        .collect();

    let unicode = by_name.get("Unicode").expect("Unicode entry");
    assert_eq!(unicode.fields.get("username").unwrap(), "¯\\_(ツ)_/¯");
    assert_eq!(unicode.tags, vec!["General"]);
}

#[test]
fn test_new_database_browser_kdbx() {
    let items = parse_kdbx("NewDatabaseBrowser.kdbx");
    assert_eq!(items.len(), 2);

    let by_name: HashMap<&str, &ParsedItem> = items
        .iter()
        .map(|i| (i.fields.get("name").unwrap().as_str(), i))
        .collect();

    let sub = by_name.get("Subgroup Entry").expect("Subgroup Entry");
    assert_eq!(sub.tags, vec!["Homebanking", "Subgroup"]);
}

#[test]
fn test_new_database_multi_kdbx() {
    let items = parse_kdbx("NewDatabaseMulti.kdbx");
    assert_eq!(items.len(), 4);

    let names: Vec<&str> = items
        .iter()
        .map(|i| i.fields.get("name").unwrap().as_str())
        .collect();
    assert!(names.contains(&"Single Entry"));
    assert!(names.contains(&"Multi Entry 1"));
    assert!(names.contains(&"Multi Entry 2"));
    assert!(names.contains(&"Subgroup Entry"));
}

#[test]
fn test_merge_database_kdbx() {
    let items = parse_kdbx("MergeDatabase.kdbx");
    assert_eq!(items.len(), 3);

    let by_name: HashMap<&str, &ParsedItem> = items
        .iter()
        .map(|i| (i.fields.get("name").unwrap().as_str(), i))
        .collect();

    assert!(by_name.contains_key("Sample Entry"));
    assert!(by_name.contains_key("pc"));
    assert!(by_name.contains_key("b"));
    assert_eq!(by_name["pc"].tags, vec!["General"]);
    assert_eq!(by_name["b"].tags, vec!["TestExtraGroup"]);
}

#[test]
fn test_sync_database_kdbx() {
    let items = parse_kdbx("SyncDatabase.kdbx");
    assert_eq!(items.len(), 4);

    let by_name: HashMap<&str, &ParsedItem> = items
        .iter()
        .map(|i| (i.fields.get("name").unwrap().as_str(), i))
        .collect();

    assert!(by_name.contains_key("Sample Entry"));
    assert!(by_name.contains_key("pc"));
    assert!(by_name.contains_key("Subgroup Entry"));
    assert!(by_name.contains_key("b"));
}

// -- KDBX 4.0 integration test (password "t") ------------------------------

#[test]
fn test_format400_kdbx() {
    let path = std::path::Path::new("tests/data/Format400.kdbx");
    if !path.exists() {
        return;
    }
    let pw = SecureStr::new("t".to_string());
    let items = KeePassParser
        .parse(path, Some(&pw), None)
        .expect("Format400.kdbx should parse with password 't'");
    assert_eq!(items.len(), 1, "Format400.kdbx should contain 1 entry");
    assert_eq!(items[0].fields.get("name").unwrap(), "Format400");
    assert_eq!(items[0].fields.get("username").unwrap(), "Format400");
    assert_eq!(items[0].fields.get("password").unwrap(), "Format400");
    assert!(items[0].fields.get("url").unwrap().is_empty());
    assert!(items[0]
        .fields
        .get("notes")
        .unwrap()
        .contains("Custom: Format400"));
}

// -- Expected failure tests ------------------------------------------------

#[test]
fn test_format200_unsupported_version() {
    let path = std::path::Path::new("tests/data/Format200.kdbx");
    if !path.exists() {
        return;
    }
    let pw = SecureStr::new("a".to_string());
    let result = KeePassParser.parse(path, Some(&pw), None);
    assert!(result.is_err(), "KDBX 2.0 should not be supported");
}

#[test]
fn test_wrong_password_returns_error() {
    let path = std::path::Path::new("tests/data/NewDatabase.kdbx");
    if !path.exists() {
        return;
    }
    let wrong = SecureStr::new("wrongpassword".to_string());
    let result = KeePassParser.parse(path, Some(&wrong), None);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(
            err,
            ImportExportError::InvalidPassword
                | ImportExportError::DecryptionFailed(_)
                | ImportExportError::ParseError { .. }
                | ImportExportError::FileReadError { .. }
        ),
        "expected password/decrypt error, got: {:?}",
        err
    );
}

#[test]
fn test_no_password_returns_password_required() {
    let path = std::path::Path::new("tests/data/NewDatabase.kdbx");
    if !path.exists() {
        return;
    }
    let result = KeePassParser.parse(path, None, None);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ImportExportError::PasswordRequired
    ));
}

#[test]
fn test_sync_database_different_password_fails() {
    let path = std::path::Path::new("tests/data/SyncDatabaseDifferentPassword.kdbx");
    if !path.exists() {
        return;
    }
    let pw = SecureStr::new("a".to_string());
    let result = KeePassParser.parse(path, Some(&pw), None);
    assert!(result.is_err(), "different password should fail");
}
