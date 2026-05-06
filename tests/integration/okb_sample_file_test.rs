//! Integration tests for OKB sample file parsing.
//!
//! These tests verify the OKB parser's ability to handle various file formats:
//! - Valid OKB files with different credential types
//! - Edge cases (special characters, long text, Unicode)
//! - Error cases (corrupted headers, wrong versions, truncated files, wrong passwords)

use oak_keyring::services::import_export::parser::{FormatParser, ParsedItem};
use oak_keyring::services::import_export::parsers::okb::OkbParser;
use oak_keyring::types::SecureStr;

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Parse an OKB file with the correct test password.
fn parse_okb(name: &str) -> Vec<ParsedItem> {
    let path = std::path::Path::new("tests/data").join(name);
    if !path.exists() {
        eprintln!("Skipping: {} not found", name);
        return Vec::new();
    }
    let pw = SecureStr::new("test-password".to_string());
    let parser = OkbParser;
    parser
        .parse(&path, Some(&pw), None)
        .unwrap_or_else(|e| panic!("failed to parse {}: {}", name, e))
}

// ---------------------------------------------------------------------------
// Test cases
// ---------------------------------------------------------------------------

#[test]
fn test_okb_basic_parse() {
    let items = parse_okb("okb_basic.okb");
    if items.is_empty() {
        return; // File not found
    }

    assert_eq!(items.len(), 3, "should parse 3 login records");

    // Record 0: GitHub
    let github = &items[0];
    assert_eq!(
        github.fields.get("credential_type"),
        Some(&"login".to_string())
    );
    assert_eq!(github.fields.get("name"), Some(&"GitHub".to_string()));
    assert_eq!(
        github.fields.get("username"),
        Some(&"developer".to_string())
    );
    assert_eq!(
        github.fields.get("password"),
        Some(&"gh_secret_123".to_string())
    );
    assert!(github.fields.get("url").unwrap().contains("github.com"));
    assert_eq!(github.tags, vec!["dev"]);
    assert_eq!(github.fields.get("is_favorite"), Some(&"true".to_string()));

    // Record 1: Gmail
    let gmail = &items[1];
    assert_eq!(
        gmail.fields.get("credential_type"),
        Some(&"login".to_string())
    );
    assert_eq!(gmail.fields.get("name"), Some(&"Gmail".to_string()));
    assert_eq!(
        gmail.fields.get("username"),
        Some(&"user@gmail.com".to_string())
    );
    assert_eq!(
        gmail.fields.get("password"),
        Some(&"gm_pass_456".to_string())
    );
    assert!(gmail.tags.contains(&"email".to_string()));
    assert!(gmail.tags.contains(&"work".to_string()));

    // Record 2: AWS Console
    let aws = &items[2];
    assert_eq!(
        aws.fields.get("credential_type"),
        Some(&"login".to_string())
    );
    assert_eq!(aws.fields.get("name"), Some(&"AWS Console".to_string()));
    assert_eq!(
        aws.fields.get("username"),
        Some(&"admin@company".to_string())
    );
    assert_eq!(
        aws.fields.get("password"),
        Some(&"aws_root_789".to_string())
    );
    assert_eq!(aws.tags, vec!["cloud", "infra"]);
}

#[test]
fn test_okb_mixed_types_parse() {
    let items = parse_okb("okb_mixed_types.okb");
    if items.is_empty() {
        return; // File not found
    }

    assert_eq!(items.len(), 3, "should parse 3 records of different types");

    // Find GitLab (login)
    let gitlab = items
        .iter()
        .find(|item| item.fields.get("name") == Some(&"GitLab".to_string()))
        .expect("GitLab record should exist");
    assert_eq!(
        gitlab.fields.get("credential_type"),
        Some(&"login".to_string())
    );
    assert_eq!(
        gitlab.fields.get("username"),
        Some(&"dev@gitlab".to_string())
    );
    assert_eq!(gitlab.fields.get("password"), Some(&"gl_pass".to_string()));

    // Find AWS API Key (api)
    let aws_api = items
        .iter()
        .find(|item| item.fields.get("name") == Some(&"AWS API Key".to_string()))
        .expect("AWS API Key record should exist");
    assert_eq!(
        aws_api.fields.get("credential_type"),
        Some(&"api".to_string())
    );
    assert_eq!(
        aws_api.fields.get("app_id"),
        Some(&"AKIAIOSFODNN7".to_string())
    );
    assert_eq!(
        aws_api.fields.get("secret_key"),
        Some(&"wJalrXUtnFEMI/K7MDENG".to_string())
    );

    // Find GitHub SSH (ssh)
    let github_ssh = items
        .iter()
        .find(|item| item.fields.get("name") == Some(&"GitHub SSH".to_string()))
        .expect("GitHub SSH record should exist");
    assert_eq!(
        github_ssh.fields.get("credential_type"),
        Some(&"ssh".to_string())
    );
    assert!(github_ssh.fields.contains_key("public_key"));
    assert!(github_ssh.fields.contains_key("private_key"));
    assert!(github_ssh.fields.contains_key("passphrase"));
}

#[test]
fn test_okb_edge_cases_parse() {
    let items = parse_okb("okb_edge_cases.okb");
    if items.is_empty() {
        return; // File not found
    }

    assert_eq!(items.len(), 4, "should parse 4 edge case records");

    // Record with special characters in name
    let special_name = items
        .iter()
        .find(|item| {
            item.fields
                .get("name")
                .map(|n: &String| n.contains(',') && n.contains('"'))
                .unwrap_or(false)
        })
        .expect("Record with comma and quote in name should exist");
    assert!(special_name
        .fields
        .get("password")
        .unwrap()
        .contains::<&str>(&"<>&\"'"));
    assert!(special_name.fields.get("notes").unwrap().contains("line1"));

    // Record with long notes
    let long_notes = items
        .iter()
        .find(|item| item.fields.get("name") == Some(&"Long Notes".to_string()))
        .expect("Long Notes record should exist");
    assert!(
        long_notes.fields.get("notes").unwrap().len() > 2000,
        "notes field should be very long"
    );

    // Record with Chinese characters
    let chinese = items
        .iter()
        .find(|item| item.fields.get("name") == Some(&"测试账户".to_string()))
        .expect("Chinese record should exist");
    assert!(chinese.fields.get("username").unwrap().contains("用户"));
    assert!(chinese.tags.iter().any(|t: &String| t.contains("标签")));

    // Minimal entry (only required fields)
    let minimal = items
        .iter()
        .find(|item| item.fields.get("name") == Some(&"Minimal Entry".to_string()))
        .expect("Minimal Entry record should exist");
    assert_eq!(
        minimal.fields.get("credential_type"),
        Some(&"login".to_string())
    );
    // These fields should be absent or empty in a minimal record
    assert!(
        minimal
            .fields
            .get("username")
            .map(|u: &String| u.is_empty())
            .unwrap_or(true),
        "username should be absent or empty in minimal record"
    );
    assert!(
        minimal
            .fields
            .get("password")
            .map(|p: &String| p.is_empty())
            .unwrap_or(true),
        "password should be absent or empty in minimal record"
    );
    assert!(
        minimal
            .fields
            .get("url")
            .map(|u: &String| u.is_empty())
            .unwrap_or(true),
        "url should be absent or empty in minimal record"
    );
}

#[test]
fn test_okb_corrupted_header_rejected() {
    let path = std::path::Path::new("tests/data").join("okb_corrupted_header.bin");
    if !path.exists() {
        eprintln!("Skipping: okb_corrupted_header.bin not found");
        return;
    }

    let pw = SecureStr::new("test-password".to_string());
    let parser = OkbParser;
    let result = parser.parse(&path, Some(&pw), None);

    assert!(result.is_err(), "corrupted header file should be rejected");

    let err = result.unwrap_err();
    let err_msg = err.to_string().to_lowercase();

    // Error should be either InvalidFormat or DecryptionFailed
    assert!(
        err_msg.contains("invalid format")
            || err_msg.contains("decryption")
            || err_msg.contains("version"),
        "error should mention format, decryption, or version issue, got: {}",
        err
    );
}

#[test]
fn test_okb_wrong_version_rejected() {
    let path = std::path::Path::new("tests/data").join("okb_wrong_version.bin");
    if !path.exists() {
        eprintln!("Skipping: okb_wrong_version.bin not found");
        return;
    }

    let pw = SecureStr::new("test-password".to_string());
    let parser = OkbParser;
    let result = parser.parse(&path, Some(&pw), None);

    assert!(result.is_err(), "wrong version file should be rejected");

    let err = result.unwrap_err();
    let err_msg = err.to_string().to_lowercase();

    assert!(
        err_msg.contains("invalid format")
            || err_msg.contains("version")
            || err_msg.contains("unsupported"),
        "error should mention format or version issue, got: {}",
        err
    );
}

#[test]
fn test_okb_truncated_rejected() {
    let path = std::path::Path::new("tests/data").join("okb_truncated.bin");
    if !path.exists() {
        eprintln!("Skipping: okb_truncated.bin not found");
        return;
    }

    let pw = SecureStr::new("test-password".to_string());
    let parser = OkbParser;
    let result = parser.parse(&path, Some(&pw), None);

    assert!(result.is_err(), "truncated file should be rejected");

    let err = result.unwrap_err();
    let err_msg = err.to_string().to_lowercase();

    assert!(
        err_msg.contains("too short")
            || err_msg.contains("unexpected eof")
            || err_msg.contains("invalid format"),
        "error should mention file too short or format issue, got: {}",
        err
    );
}

#[test]
fn test_okb_wrong_password_rejected() {
    let path = std::path::Path::new("tests/data").join("okb_basic.okb");
    if !path.exists() {
        eprintln!("Skipping: okb_basic.okb not found");
        return;
    }

    let wrong_pw = SecureStr::new("wrong-password".to_string());
    let parser = OkbParser;
    let result = parser.parse(&path, Some(&wrong_pw), None);

    assert!(result.is_err(), "wrong password should be rejected");

    let err = result.unwrap_err();
    let err_msg = err.to_string().to_lowercase();

    assert!(
        err_msg.contains("decryption") || err_msg.contains("password") || err_msg.contains("auth"),
        "error should mention decryption or password issue, got: {}",
        err
    );
}
