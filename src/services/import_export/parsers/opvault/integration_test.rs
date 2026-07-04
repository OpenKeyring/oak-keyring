//! Integration test demonstrating the full OpVault parser with test fixtures.
//!
//! This test creates a valid .opvault directory using the test fixture helpers,
//! then parses it using the real OpVault parser to verify end-to-end functionality.

use crate::services::import_export::parser::ParsedItem;
use crate::services::import_export::parsers::opvault::crypto_test::OpVaultFixture;
use crate::services::import_export::parsers::opvault::parser::parse_opvault;
use crate::types::SecureStr;
use serde_json::json;

#[test]
fn test_opvault_full_roundtrip() {
    // Create test fixture with known password
    let fixture = OpVaultFixture::new();

    // Create a temporary .opvault directory
    let temp_dir = tempfile::TempDir::new().unwrap();
    fixture.create_opvault_dir(temp_dir.path()).unwrap();

    // Verify the directory structure
    let default_dir = temp_dir.path().join("default");
    assert!(default_dir.exists());
    assert!(default_dir.join("profile.js").exists());
    assert!(default_dir.join("band_0.js").exists());

    // Parse the .opvault using the real parser
    let password = SecureStr::new(fixture.password.clone());
    let items = parse_opvault(temp_dir.path(), Some(&password)).expect("Failed to parse opvault");

    // Verify we got exactly one item
    assert_eq!(items.len(), 1, "Expected exactly one item");

    let item = &items[0];

    // Verify the item metadata
    assert_eq!(item.source_id, "11111111111111111111111111111111");
    assert_eq!(item.tags.len(), 0);

    // Verify the item fields
    assert_eq!(item.fields.get("name").unwrap(), "Test Login");
    assert_eq!(item.fields.get("username").unwrap(), "test@example.com");
    assert_eq!(item.fields.get("password").unwrap(), "secret123");
    assert_eq!(item.fields.get("url").unwrap(), "https://example.com");

    println!("✓ Full OpVault roundtrip test passed!");
    println!(
        "  Successfully parsed item: {} ({})",
        item.fields.get("name").unwrap(),
        item.source_id
    );
}

#[test]
fn test_opvault_multiple_items() {
    let fixture = OpVaultFixture::new();

    // Create a temporary .opvault directory
    let temp_dir = tempfile::TempDir::new().unwrap();
    let default_dir = temp_dir.path().join("default");
    std::fs::create_dir_all(&default_dir).unwrap();

    // Create profile.js
    let profile_js = fixture.create_profile_js("00000000000000000000000000000000");
    std::fs::write(default_dir.join("profile.js"), profile_js).unwrap();

    // Create band_0.js with multiple items
    let items = vec![
        (
            "11111111111111111111111111111111",
            "Gmail",
            "user1@gmail.com",
            "password1",
            Some("https://gmail.com"),
        ),
        (
            "22222222222222222222222222222222",
            "GitHub",
            "user2@github.com",
            "password2",
            Some("https://github.com"),
        ),
        (
            "33333333333333333333333333333333",
            "Local Server",
            "admin",
            "admin123",
            None, // No URL
        ),
    ];

    // Build a HashMap of UUID -> BandItem for the band file
    let mut band_map = serde_json::Map::new();
    for (uuid, title, username, password, url) in &items {
        let (_, overview, key, details) =
            fixture.create_login_item(uuid, title, username, password, *url);
        let item_json = json!({
            "uuid": uuid,
            "category": "001",
            "trashed": false,
            "o": overview,
            "k": key,
            "d": details,
            "folder": "",
            "findex": "",
            "created": 0,
            "updated": 0
        });
        band_map.insert(uuid.to_string(), item_json);
    }

    let band_content = format!("ld({})", serde_json::to_string(&band_map).unwrap());
    std::fs::write(default_dir.join("band_0.js"), band_content).unwrap();

    // Parse the .opvault
    let password = SecureStr::new(fixture.password.clone());
    let parsed_items =
        parse_opvault(temp_dir.path(), Some(&password)).expect("Failed to parse opvault");

    // Verify all items were parsed
    assert_eq!(parsed_items.len(), 3);

    // Build a lookup by source_id (HashMap iteration order is non-deterministic)
    let parsed_map: std::collections::HashMap<&str, &ParsedItem> = parsed_items
        .iter()
        .map(|item| (item.source_id.as_str(), item))
        .collect();

    // Verify each expected item by UUID
    for (uuid, title, username, password, url) in &items {
        let item = parsed_map.get(uuid).unwrap_or_else(|| {
            panic!("expected item with UUID {uuid}");
        });
        assert_eq!(item.fields.get("name").unwrap(), title);
        assert_eq!(item.fields.get("username").unwrap(), username);
        assert_eq!(item.fields.get("password").unwrap(), password);

        if let Some(url) = url {
            assert_eq!(*item.fields.get("url").unwrap(), *url);
        } else {
            assert!(!item.fields.contains_key("url"));
        }
    }

    println!("✓ Multiple items test passed!");
    println!("  Successfully parsed {} items", parsed_items.len());
}

#[test]
fn test_opvault_wrong_password() {
    let fixture = OpVaultFixture::new();

    // Create a temporary .opvault directory
    let temp_dir = tempfile::TempDir::new().unwrap();
    fixture.create_opvault_dir(temp_dir.path()).unwrap();

    // Try to parse with wrong password
    let wrong_password = SecureStr::new("wrongpassword".to_string());
    let result = parse_opvault(temp_dir.path(), Some(&wrong_password));

    // Should fail with InvalidPassword error
    assert!(result.is_err());

    if let Err(e) = result {
        let error_msg = e.to_string();
        assert!(
            error_msg.contains("password")
                || error_msg.contains("HMAC")
                || error_msg.contains("decrypt"),
            "Expected password/HMAC/decrypt error, got: {}",
            error_msg
        );
        println!("✓ Wrong password test passed!");
        println!("  Error message: {}", error_msg);
    } else {
        panic!("Expected error when using wrong password");
    }
}

#[test]
fn test_real_opvault_parse() {
    let path = std::path::Path::new("tests/data/openkeyring.opvault");
    if !path.exists() {
        eprintln!("Skipping: test data not found at {:?}", path);
        return;
    }
    let password = SecureStr::new("oak-keyring".to_string());
    let items = parse_opvault(path, Some(&password)).expect("Failed to parse opvault");

    assert_eq!(items.len(), 5, "expected 5 items, got {}", items.len());

    let by_id: std::collections::HashMap<&str, &ParsedItem> =
        items.iter().map(|i| (i.source_id.as_str(), i)).collect();

    // cat=003 Secure Note
    let sn = by_id
        .get("12CC60BD1B8F4AA491F9314B437DDF86")
        .expect("Secure Note item");
    assert_eq!(sn.fields.get("name").unwrap(), "My Secure Note");
    assert!(sn.fields.get("notes").unwrap().contains("test secure note"));
    assert!(!sn.fields.contains_key("username"));
    assert!(!sn.fields.contains_key("password"));

    // cat=005 Password — top-level password + TOTP
    let cp = by_id
        .get("1211EB9D74FE44CAADA3805506E482BB")
        .expect("Strong Password item");
    assert_eq!(cp.fields.get("name").unwrap(), "Strong Password");
    assert_eq!(cp.fields.get("password").unwrap(), "Str0ng!P@ss");
    assert!(cp.fields.contains_key("totp"));

    // cat=001 Login — designation fields + TOTP
    let kp = by_id
        .get("30B6513EE64B4DFE9C47EC2F257CE296")
        .expect("Example Website item");
    assert_eq!(kp.fields.get("name").unwrap(), "Example Website");
    assert_eq!(kp.fields.get("username").unwrap(), "user1");
    assert_eq!(kp.fields.get("password").unwrap(), "password123");
    assert_eq!(kp.fields.get("url").unwrap(), "https://example.com");
    assert!(kp.fields.get("notes").unwrap().contains("Test account"));
    assert!(
        !kp.fields.get("notes").unwrap().contains("TOTP:"),
        "TOTP should be structured, not appended to notes"
    );
    assert!(kp.fields.contains_key("totp"));

    // cat=110 Server — section fields extracted
    let srv = by_id
        .get("43B445C591924C0ABD7770816A1E8514")
        .expect("My Server item");
    assert_eq!(srv.fields.get("name").unwrap(), "My Server");
    assert_eq!(srv.fields.get("username").unwrap(), "admin");
    assert_eq!(srv.fields.get("password").unwrap(), "admin123");
    assert_eq!(srv.fields.get("url").unwrap(), "myserver.local");
    assert!(srv
        .fields
        .get("notes")
        .unwrap()
        .contains("admin username: admin"));

    // cat=001 Login — designation fields + date section
    let exp = by_id
        .get("A6C49CAF606248828E33F0938FCEFF5C")
        .expect("Old Account item");
    assert_eq!(exp.fields.get("name").unwrap(), "Old Account");
    assert_eq!(exp.fields.get("username").unwrap(), "olduser");
    assert_eq!(exp.fields.get("password").unwrap(), "oldpass123");
}

#[test]
fn test_real_opvault_wrong_password() {
    let path = std::path::Path::new("tests/data/openkeyring.opvault");
    if !path.exists() {
        eprintln!("Skipping: test data not found at {:?}", path);
        return;
    }
    let wrong = SecureStr::new("wrong".to_string());
    assert!(parse_opvault(path, Some(&wrong)).is_err());
}

#[test]
fn _dump() {
    let path = std::path::Path::new("tests/data/openkeyring.opvault");
    if !path.exists() {
        eprintln!("Skipping: test data not found at {:?}", path);
        return;
    }
    let items = parse_opvault(path, Some(&SecureStr::new("oak-keyring".into()))).unwrap();
    println!("\n=== openkeyring.opvault 解析结果 ===");
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
