//! Duplicate detection logic for import pipeline.
//!
//! Matches [`ParsedItem`] values against existing vault records using a
//! composite key of `name` + `credential_type` + core field. The core field
//! varies by credential type:
//!
//! | CredentialType | Core field |
//! |----------------|------------|
//! | Login          | username   |
//! | Api            | app_id     |
//! | Ssh            | public_key |

use std::collections::HashSet;

use crate::services::import_export::parser::ParsedItem;

// ---------------------------------------------------------------------------
// ExistingRecordKey — composite key for duplicate matching
// ---------------------------------------------------------------------------

/// Summary of an existing vault record for duplicate matching.
///
/// Two records are considered duplicates when they share the same `name`,
/// `credential_type`, and `core_field` (case-insensitive).
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ExistingRecordKey {
    pub name: String,
    pub credential_type: String,
    pub core_field: String,
}

// ---------------------------------------------------------------------------
// Key extraction helper
// ---------------------------------------------------------------------------

/// Determines the core-field value for a parsed item based on its credential type.
///
/// - `"api"` -> `app_id`
/// - `"ssh"` -> `public_key`
/// - everything else (including `"login"`) -> `username`
fn core_field_for_type(
    cred_type: &str,
    fields: &std::collections::HashMap<String, String>,
) -> String {
    match cred_type {
        "api" => fields.get("app_id"),
        "ssh" => fields.get("public_key"),
        _ => fields.get("username"),
    }
    .map(|s| s.to_lowercase())
    .unwrap_or_default()
}

/// Builds an [`ExistingRecordKey`] from a [`ParsedItem`].
fn item_to_key(item: &ParsedItem) -> ExistingRecordKey {
    let name = item
        .fields
        .get("name")
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    let cred_type = item
        .fields
        .get("credential_type")
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    let core_field = core_field_for_type(&cred_type, &item.fields);

    ExistingRecordKey {
        name,
        credential_type: cred_type,
        core_field,
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Detects duplicates between parsed items and existing vault records.
///
/// Returns a `Vec<bool>` with the same length as `items`. `true` at index *i*
/// means `items[i]` has a matching record in `existing_keys`.
pub fn detect_duplicates(
    items: &[ParsedItem],
    existing_keys: &HashSet<ExistingRecordKey>,
) -> Vec<bool> {
    items
        .iter()
        .map(|item| existing_keys.contains(&item_to_key(item)))
        .collect()
}

/// Extracts duplicate-detection keys from a slice of [`ParsedItem`] values.
///
/// Useful for building the `existing_keys` set from previously imported items
/// or for comparing two import batches against each other.
pub fn extract_keys(items: &[ParsedItem]) -> Vec<ExistingRecordKey> {
    items.iter().map(item_to_key).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    // -- Helpers --

    /// Build a `ParsedItem` from a slice of `(key, value)` pairs.
    fn make_item(fields: &[(&str, &str)]) -> ParsedItem {
        ParsedItem {
            source_id: "test".into(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            tags: vec![],
        }
    }

    /// Convenience: build an `ExistingRecordKey`.
    fn make_key(name: &str, cred_type: &str, core_field: &str) -> ExistingRecordKey {
        ExistingRecordKey {
            name: name.to_lowercase(),
            credential_type: cred_type.to_lowercase(),
            core_field: core_field.to_lowercase(),
        }
    }

    // -- Test 1: Exact duplicate --

    #[test]
    fn exact_duplicate_is_detected() {
        let existing: HashSet<ExistingRecordKey> = {
            let mut set = HashSet::new();
            set.insert(make_key("Gmail", "login", "user@gmail.com"));
            set
        };

        let items = vec![make_item(&[
            ("name", "Gmail"),
            ("credential_type", "login"),
            ("username", "user@gmail.com"),
        ])];

        let result = detect_duplicates(&items, &existing);
        assert_eq!(result, vec![true]);
    }

    // -- Test 2: No duplicate (different name) --

    #[test]
    fn different_name_is_not_duplicate() {
        let existing: HashSet<ExistingRecordKey> = {
            let mut set = HashSet::new();
            set.insert(make_key("Gmail", "login", "user@gmail.com"));
            set
        };

        let items = vec![make_item(&[
            ("name", "Outlook"),
            ("credential_type", "login"),
            ("username", "user@gmail.com"),
        ])];

        let result = detect_duplicates(&items, &existing);
        assert_eq!(result, vec![false]);
    }

    // -- Test 3: Different credential type -> NOT duplicate --

    #[test]
    fn different_credential_type_is_not_duplicate() {
        let existing: HashSet<ExistingRecordKey> = {
            let mut set = HashSet::new();
            set.insert(make_key("Gmail", "login", "user@gmail.com"));
            set
        };

        let items = vec![make_item(&[
            ("name", "Gmail"),
            ("credential_type", "api"),
            ("app_id", "user@gmail.com"),
        ])];

        let result = detect_duplicates(&items, &existing);
        assert_eq!(result, vec![false]);
    }

    // -- Test 4: Different core field -> NOT duplicate --

    #[test]
    fn different_core_field_is_not_duplicate() {
        let existing: HashSet<ExistingRecordKey> = {
            let mut set = HashSet::new();
            set.insert(make_key("Gmail", "login", "user@gmail.com"));
            set
        };

        let items = vec![make_item(&[
            ("name", "Gmail"),
            ("credential_type", "login"),
            ("username", "other@gmail.com"),
        ])];

        let result = detect_duplicates(&items, &existing);
        assert_eq!(result, vec![false]);
    }

    // -- Test 5: Case insensitive matching --

    #[test]
    fn case_insensitive_matching() {
        let existing: HashSet<ExistingRecordKey> = {
            let mut set = HashSet::new();
            set.insert(make_key("Example", "Login", "User@Example.COM"));
            set
        };

        let items = vec![make_item(&[
            ("name", "example"),
            ("credential_type", "login"),
            ("username", "user@example.com"),
        ])];

        let result = detect_duplicates(&items, &existing);
        assert_eq!(result, vec![true]);
    }

    // -- Test 6: Login matching (name + username) --

    #[test]
    fn login_matching_by_name_and_username() {
        let existing: HashSet<ExistingRecordKey> = {
            let mut set = HashSet::new();
            set.insert(make_key("GitHub", "login", "devuser"));
            set.insert(make_key("GitLab", "login", "devuser"));
            set
        };

        let items = vec![make_item(&[
            ("name", "GitHub"),
            ("credential_type", "login"),
            ("username", "devuser"),
        ])];

        let result = detect_duplicates(&items, &existing);
        assert_eq!(result, vec![true]);
    }

    // -- Test 7: Api matching (name + app_id) --

    #[test]
    fn api_matching_by_name_and_app_id() {
        let existing: HashSet<ExistingRecordKey> = {
            let mut set = HashSet::new();
            set.insert(make_key("AWS CLI", "api", "AKIAIOSFODNN7EXAMPLE"));
            set
        };

        let items = vec![make_item(&[
            ("name", "AWS CLI"),
            ("credential_type", "api"),
            ("app_id", "AKIAIOSFODNN7EXAMPLE"),
        ])];

        let result = detect_duplicates(&items, &existing);
        assert_eq!(result, vec![true]);
    }

    // -- Test 8: Ssh matching (name + public_key) --

    #[test]
    fn ssh_matching_by_name_and_public_key() {
        let existing: HashSet<ExistingRecordKey> = {
            let mut set = HashSet::new();
            set.insert(make_key("Work SSH", "ssh", "ssh-rsa AAAAB3...key1"));
            set
        };

        let items = vec![make_item(&[
            ("name", "Work SSH"),
            ("credential_type", "ssh"),
            ("public_key", "ssh-rsa AAAAB3...key1"),
        ])];

        let result = detect_duplicates(&items, &existing);
        assert_eq!(result, vec![true]);
    }

    // -- Test 9: Empty existing -> no duplicates --

    #[test]
    fn empty_existing_records_no_duplicates() {
        let existing: HashSet<ExistingRecordKey> = HashSet::new();

        let items = vec![make_item(&[
            ("name", "Gmail"),
            ("credential_type", "login"),
            ("username", "user@gmail.com"),
        ])];

        let result = detect_duplicates(&items, &existing);
        assert_eq!(result, vec![false]);
    }

    // -- Test 10: extract_keys produces correct keys --

    #[test]
    fn extract_keys_produces_correct_keys() {
        let items = vec![
            make_item(&[
                ("name", "Gmail"),
                ("credential_type", "login"),
                ("username", "user@gmail.com"),
            ]),
            make_item(&[
                ("name", "AWS CLI"),
                ("credential_type", "api"),
                ("app_id", "AKIA123"),
            ]),
            make_item(&[
                ("name", "Work SSH"),
                ("credential_type", "ssh"),
                ("public_key", "ssh-rsa KEY"),
            ]),
        ];

        let keys = extract_keys(&items);

        assert_eq!(keys.len(), 3);
        assert_eq!(
            keys[0],
            ExistingRecordKey {
                name: "gmail".into(),
                credential_type: "login".into(),
                core_field: "user@gmail.com".into(),
            }
        );
        assert_eq!(
            keys[1],
            ExistingRecordKey {
                name: "aws cli".into(),
                credential_type: "api".into(),
                core_field: "akia123".into(),
            }
        );
        assert_eq!(
            keys[2],
            ExistingRecordKey {
                name: "work ssh".into(),
                credential_type: "ssh".into(),
                core_field: "ssh-rsa key".into(),
            }
        );
    }

    // -- Additional edge-case tests --

    #[test]
    fn missing_fields_default_to_empty_string() {
        let existing: HashSet<ExistingRecordKey> = HashSet::new();

        // Item with no credential_type or username fields.
        let items = vec![make_item(&[("name", "Partial")])];

        let keys = extract_keys(&items);
        assert_eq!(keys[0].credential_type, "");
        assert_eq!(keys[0].core_field, "");

        let result = detect_duplicates(&items, &existing);
        assert_eq!(result, vec![false]);
    }

    #[test]
    fn multiple_items_mixed_duplicates() {
        let existing: HashSet<ExistingRecordKey> = {
            let mut set = HashSet::new();
            set.insert(make_key("Gmail", "login", "user@gmail.com"));
            set.insert(make_key("AWS CLI", "api", "AKIA123"));
            set
        };

        let items = vec![
            // Duplicate: matches existing Gmail login
            make_item(&[
                ("name", "Gmail"),
                ("credential_type", "login"),
                ("username", "user@gmail.com"),
            ]),
            // Not duplicate: new entry
            make_item(&[
                ("name", "Outlook"),
                ("credential_type", "login"),
                ("username", "user@outlook.com"),
            ]),
            // Duplicate: matches existing AWS CLI api
            make_item(&[
                ("name", "AWS CLI"),
                ("credential_type", "api"),
                ("app_id", "AKIA123"),
            ]),
            // Not duplicate: same name but different username
            make_item(&[
                ("name", "Gmail"),
                ("credential_type", "login"),
                ("username", "other@gmail.com"),
            ]),
        ];

        let result = detect_duplicates(&items, &existing);
        assert_eq!(result, vec![true, false, true, false]);
    }

    #[test]
    fn extract_keys_into_hashset_for_cross_batch_detection() {
        let batch1 = vec![make_item(&[
            ("name", "Gmail"),
            ("credential_type", "login"),
            ("username", "user@gmail.com"),
        ])];

        let existing: HashSet<ExistingRecordKey> = extract_keys(&batch1).into_iter().collect();

        let batch2 = vec![make_item(&[
            ("name", "Gmail"),
            ("credential_type", "login"),
            ("username", "user@gmail.com"),
        ])];

        let result = detect_duplicates(&batch2, &existing);
        assert_eq!(result, vec![true]);
    }

    #[test]
    fn empty_items_produces_empty_results() {
        let existing: HashSet<ExistingRecordKey> = HashSet::new();
        let items: Vec<ParsedItem> = vec![];

        let result = detect_duplicates(&items, &existing);
        assert!(result.is_empty());

        let keys = extract_keys(&items);
        assert!(keys.is_empty());
    }
}
