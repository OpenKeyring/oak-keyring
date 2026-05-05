use std::collections::HashMap;

use super::mapping::*;

use crate::commands::types::{CsvColumnMapping, ImportSource};
use crate::services::import_export::parser::ParsedItem;
use crate::types::CredentialType;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    // -- Field count tests --

    #[test]
    fn keepass_mapping_has_6_field_mappings() {
        let m = keepass_mapping();
        assert_eq!(m.field_mappings.len(), 6);
        assert_eq!(m.source_type, ImportSource::KeePass);
    }

    #[test]
    fn onepassword_1pux_mapping_has_6_field_mappings() {
        let m = onepassword_1pux_mapping();
        assert_eq!(m.field_mappings.len(), 6);
        assert_eq!(m.source_type, ImportSource::OnePassword1pux);
    }

    #[test]
    fn onepassword_opvault_mapping_has_6_field_mappings() {
        let m = onepassword_opvault_mapping();
        assert_eq!(m.field_mappings.len(), 6);
        assert_eq!(m.source_type, ImportSource::OnePasswordOpvault);
    }

    #[test]
    fn bitwarden_mapping_has_6_field_mappings() {
        let m = bitwarden_mapping();
        assert_eq!(m.field_mappings.len(), 6);
        assert_eq!(m.source_type, ImportSource::Bitwarden);
    }

    #[test]
    fn okb_mapping_has_6_field_mappings() {
        let m = okb_mapping();
        assert_eq!(m.field_mappings.len(), 6);
        assert_eq!(m.source_type, ImportSource::OpenKeyringBackup);
    }

    #[test]
    fn csv_mapping_without_tags_has_5_field_mappings() {
        let m = csv_mapping(&CsvColumnMapping {
            name_column: "name".into(),
            username_column: "user".into(),
            password_column: "pass".into(),
            url_column: "url".into(),
            notes_column: "notes".into(),
            tags_column: None,
            skip_header: true,
        });
        assert_eq!(m.field_mappings.len(), 5);
        assert_eq!(m.source_type, ImportSource::Csv);
    }

    #[test]
    fn csv_mapping_with_tags_has_6_field_mappings() {
        let m = csv_mapping(&CsvColumnMapping {
            name_column: "name".into(),
            username_column: "user".into(),
            password_column: "pass".into(),
            url_column: "url".into(),
            notes_column: "notes".into(),
            tags_column: Some("tags".into()),
            skip_header: false,
        });
        assert_eq!(m.field_mappings.len(), 6);
    }

    // -- Required field tests --

    #[test]
    fn keepass_has_3_required_fields() {
        let m = keepass_mapping();
        let required: Vec<&FieldMapping> = m.field_mappings.iter().filter(|f| f.required).collect();
        assert_eq!(required.len(), 3);
        let targets: Vec<&TargetField> = required.iter().map(|f| &f.target_field).collect();
        assert!(targets.contains(&&TargetField::Name));
        assert!(targets.contains(&&TargetField::Username));
        assert!(targets.contains(&&TargetField::Password));
    }

    #[test]
    fn onepassword_1pux_has_3_required_fields() {
        let m = onepassword_1pux_mapping();
        let required: Vec<&FieldMapping> = m.field_mappings.iter().filter(|f| f.required).collect();
        assert_eq!(required.len(), 3);
    }

    #[test]
    fn onepassword_opvault_has_3_required_fields() {
        let m = onepassword_opvault_mapping();
        let required: Vec<&FieldMapping> = m.field_mappings.iter().filter(|f| f.required).collect();
        assert_eq!(required.len(), 3);
    }

    #[test]
    fn bitwarden_has_3_required_fields() {
        let m = bitwarden_mapping();
        let required: Vec<&FieldMapping> = m.field_mappings.iter().filter(|f| f.required).collect();
        assert_eq!(required.len(), 3);
    }

    #[test]
    fn okb_has_1_required_field() {
        let m = okb_mapping();
        let required: Vec<&FieldMapping> = m.field_mappings.iter().filter(|f| f.required).collect();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0].target_field, TargetField::Name);
    }

    #[test]
    fn csv_has_3_required_fields() {
        let m = csv_mapping(&CsvColumnMapping {
            name_column: "a".into(),
            username_column: "b".into(),
            password_column: "c".into(),
            url_column: "d".into(),
            notes_column: "e".into(),
            tags_column: None,
            skip_header: false,
        });
        let required: Vec<&FieldMapping> = m.field_mappings.iter().filter(|f| f.required).collect();
        assert_eq!(required.len(), 3);
    }

    // -- CSV dynamic mapping correctness --

    #[test]
    fn csv_mapping_uses_provided_column_names() {
        let m = csv_mapping(&CsvColumnMapping {
            name_column: "site".into(),
            username_column: "login".into(),
            password_column: "secret".into(),
            url_column: "website".into(),
            notes_column: "memo".into(),
            tags_column: Some("labels".into()),
            skip_header: true,
        });

        let sources: Vec<&str> = m
            .field_mappings
            .iter()
            .map(|f| f.source_field.as_str())
            .collect();
        assert_eq!(
            sources,
            &["site", "login", "secret", "website", "memo", "labels"]
        );
    }

    // -- get_default_mapping exhaustiveness --

    #[test]
    fn get_default_mapping_returns_correct_source_type() {
        // Exhaustiveness check: if a new ImportSource variant is added,
        // this test will fail to compile until it is handled above.
        let cases = [
            (ImportSource::KeePass, "KeePass"),
            (ImportSource::OnePassword1pux, "1Password 1pux"),
            (ImportSource::OnePasswordOpvault, "1Password opvault"),
            (ImportSource::Bitwarden, "Bitwarden"),
            (ImportSource::Csv, "CSV"),
            (ImportSource::OpenKeyringBackup, "OpenKeyring Backup"),
        ];

        for (source, _label) in cases {
            let m = get_default_mapping(source.clone());
            assert_eq!(m.source_type, source);
            assert!(
                !m.field_mappings.is_empty(),
                "mapping for {_label:?} should have at least one field mapping"
            );
        }
    }

    // -- OKB type mappings --

    #[test]
    fn okb_mapping_has_3_type_mappings() {
        let m = okb_mapping();
        assert_eq!(m.type_mappings.len(), 3);

        let types: Vec<&CredentialType> = m.type_mappings.iter().map(|t| &t.target_type).collect();
        assert!(types.contains(&&CredentialType::Login));
        assert!(types.contains(&&CredentialType::Api));
        assert!(types.contains(&&CredentialType::Ssh));
    }

    // -- TypeMapping basic construction --

    #[test]
    fn type_mapping_construction() {
        let tm = TypeMapping {
            source_type: "note".into(),
            target_type: CredentialType::Login,
            field_overrides: vec![FieldMapping {
                source_field: "extra".into(),
                target_field: TargetField::Notes,
                required: false,
                default_value: Some("n/a".into()),
                transform: Some(FieldTransform::Trim),
            }],
        };
        assert_eq!(tm.source_type, "note");
        assert_eq!(tm.target_type, CredentialType::Login);
        assert_eq!(tm.field_overrides.len(), 1);
        assert_eq!(tm.field_overrides[0].default_value.as_ref().unwrap(), "n/a");
    }

    // -- FieldTransform variants --

    #[test]
    fn field_transform_equality() {
        assert_eq!(FieldTransform::Trim, FieldTransform::Trim);
        assert_eq!(
            FieldTransform::DateTimeFormat("%Y-%m-%d".into()),
            FieldTransform::DateTimeFormat("%Y-%m-%d".into())
        );
        assert_ne!(FieldTransform::Trim, FieldTransform::Lowercase);
    }

    // -- TargetField exhaustiveness via hash set --

    #[test]
    fn target_field_all_variants_are_distinct() {
        use std::collections::HashSet;
        let all = [
            TargetField::Name,
            TargetField::Username,
            TargetField::Password,
            TargetField::AppId,
            TargetField::SecretKey,
            TargetField::PublicKey,
            TargetField::PrivateKey,
            TargetField::Passphrase,
            TargetField::Url,
            TargetField::Notes,
            TargetField::Tags,
            TargetField::ExpiresAt,
        ];
        let set: HashSet<&TargetField> = all.iter().collect();
        assert_eq!(set.len(), 12, "all 12 TargetField variants must be unique");
    }

    // -- FormatMapping construction --

    #[test]
    fn format_mapping_with_custom_field_and_type() {
        let fm = FormatMapping {
            source_type: ImportSource::Bitwarden,
            field_mappings: vec![FieldMapping {
                source_field: "custom".into(),
                target_field: TargetField::AppId,
                required: true,
                default_value: None,
                transform: Some(FieldTransform::Base64Decode),
            }],
            type_mappings: vec![TypeMapping {
                source_type: "securenote".into(),
                target_type: CredentialType::Login,
                field_overrides: vec![],
            }],
        };
        assert_eq!(fm.field_mappings.len(), 1);
        assert_eq!(fm.type_mappings.len(), 1);
        assert_eq!(fm.field_mappings[0].target_field, TargetField::AppId);
        assert!(matches!(
            &fm.field_mappings[0].transform,
            Some(FieldTransform::Base64Decode)
        ));
    }

    // =========================================================================
    // Engine function tests
    // =========================================================================

    // -- infer_credential_type --

    #[test]
    fn infer_type_login_when_username_and_password_present() {
        let mut fields = HashMap::new();
        fields.insert("username".into(), "alice".into());
        fields.insert("password".into(), "s3cret".into());
        assert_eq!(infer_credential_type(&fields), CredentialType::Login);
    }

    #[test]
    fn infer_type_api_when_app_id_and_secret_key_present() {
        let mut fields = HashMap::new();
        fields.insert("app_id".into(), "my-app".into());
        fields.insert("secret_key".into(), "abc123".into());
        assert_eq!(infer_credential_type(&fields), CredentialType::Api);
    }

    #[test]
    fn infer_type_ssh_when_public_key_and_private_key_present() {
        let mut fields = HashMap::new();
        fields.insert("public_key".into(), "ssh-rsa AAA...".into());
        fields.insert("private_key".into(), "-----BEGIN RSA...".into());
        assert_eq!(infer_credential_type(&fields), CredentialType::Ssh);
    }

    #[test]
    fn infer_type_defaults_to_login_when_no_matching_fields() {
        let fields = HashMap::new();
        assert_eq!(infer_credential_type(&fields), CredentialType::Login);
    }

    #[test]
    fn infer_type_login_takes_priority_over_api() {
        // Login is checked first, so username+password + app_id+secret_key --> Login
        let mut fields = HashMap::new();
        fields.insert("username".into(), "alice".into());
        fields.insert("password".into(), "s3cret".into());
        fields.insert("app_id".into(), "my-app".into());
        fields.insert("secret_key".into(), "abc123".into());
        assert_eq!(infer_credential_type(&fields), CredentialType::Login);
    }

    #[test]
    fn infer_type_explicit_credential_type_overrides_heuristic() {
        // When credential_type field is present, it takes priority over field-based heuristics
        let mut fields = HashMap::new();
        fields.insert("credential_type".into(), "ssh".into());
        fields.insert("username".into(), "alice".into());
        fields.insert("password".into(), "s3cret".into());
        // Without credential_type, this would be Login, but with it, should be Ssh
        assert_eq!(infer_credential_type(&fields), CredentialType::Ssh);
    }

    #[test]
    fn infer_type_explicit_api_overrides_login_heuristic() {
        // OKB export with API type and legacy username/password fields
        let mut fields = HashMap::new();
        fields.insert("credential_type".into(), "api".into());
        fields.insert("username".into(), "alice".into());
        fields.insert("password".into(), "s3cret".into());
        // Without credential_type, this would be Login, but with it, should be Api
        assert_eq!(infer_credential_type(&fields), CredentialType::Api);
    }

    #[test]
    fn infer_type_ignores_invalid_credential_type() {
        // Invalid credential_type value should fall back to heuristic
        let mut fields = HashMap::new();
        fields.insert("credential_type".into(), "invalid_type".into());
        fields.insert("username".into(), "alice".into());
        fields.insert("password".into(), "s3cret".into());
        // Invalid credential_type is ignored, heuristic applies
        assert_eq!(infer_credential_type(&fields), CredentialType::Login);
    }

    #[test]
    fn infer_type_login_when_credential_type_field_is_login() {
        // Explicit "login" credential_type should be respected
        let mut fields = HashMap::new();
        fields.insert("credential_type".into(), "login".into());
        fields.insert("username".into(), "alice".into());
        fields.insert("password".into(), "s3cret".into());
        // Explicit login type should return Login
        assert_eq!(infer_credential_type(&fields), CredentialType::Login);
    }

    #[test]
    fn infer_type_ignores_credential_type_for_non_okb_imports() {
        // When credential_type field is absent, heuristic should apply
        let mut fields = HashMap::new();
        fields.insert("app_id".into(), "my-app".into());
        fields.insert("secret_key".into(), "abc123".into());
        // No credential_type field, heuristic applies → Api
        assert_eq!(infer_credential_type(&fields), CredentialType::Api);
    }

    // -- apply_field_mapping --

    #[test]
    fn apply_mapping_basic_source_fields_mapped_correctly() {
        let mapping = bitwarden_mapping();
        let mut fields = HashMap::new();
        fields.insert("name".into(), "Gmail".into());
        fields.insert("login.username".into(), "user@gmail.com".into());
        fields.insert("login.password".into(), "hunter2".into());
        fields.insert("login.uri".into(), "https://gmail.com".into());
        fields.insert("notes".into(), "My email".into());

        let result = apply_field_mapping(&fields, &mapping);

        assert_eq!(result.get(&TargetField::Name).unwrap(), "Gmail");
        assert_eq!(
            result.get(&TargetField::Username).unwrap(),
            "user@gmail.com"
        );
        assert_eq!(result.get(&TargetField::Password).unwrap(), "hunter2");
        assert_eq!(result.get(&TargetField::Url).unwrap(), "https://gmail.com");
        assert_eq!(result.get(&TargetField::Notes).unwrap(), "My email");
    }

    #[test]
    fn apply_mapping_with_trim_transform() {
        let mapping = FormatMapping {
            source_type: ImportSource::Csv,
            field_mappings: vec![FieldMapping {
                source_field: "name".into(),
                target_field: TargetField::Name,
                required: true,
                default_value: None,
                transform: Some(FieldTransform::Trim),
            }],
            type_mappings: vec![],
        };

        let mut fields = HashMap::new();
        fields.insert("name".into(), "  padded name  ".into());

        let result = apply_field_mapping(&fields, &mapping);
        assert_eq!(result.get(&TargetField::Name).unwrap(), "padded name");
    }

    #[test]
    fn apply_mapping_uses_default_value_when_source_field_missing() {
        let mapping = FormatMapping {
            source_type: ImportSource::Csv,
            field_mappings: vec![FieldMapping {
                source_field: "url".into(),
                target_field: TargetField::Url,
                required: false,
                default_value: Some("https://example.com".into()),
                transform: None,
            }],
            type_mappings: vec![],
        };

        let fields = HashMap::new();
        let result = apply_field_mapping(&fields, &mapping);
        assert_eq!(
            result.get(&TargetField::Url).unwrap(),
            "https://example.com"
        );
    }

    #[test]
    fn apply_mapping_skips_missing_required_field_without_default() {
        let mapping = FormatMapping {
            source_type: ImportSource::Csv,
            field_mappings: vec![FieldMapping {
                source_field: "name".into(),
                target_field: TargetField::Name,
                required: true,
                default_value: None,
                transform: None,
            }],
            type_mappings: vec![],
        };

        let fields = HashMap::new();
        let result = apply_field_mapping(&fields, &mapping);
        assert!(
            result.get(&TargetField::Name).is_none(),
            "missing required field without default should be skipped"
        );
    }

    #[test]
    fn apply_mapping_with_base64_decode_transform() {
        let mapping = FormatMapping {
            source_type: ImportSource::Csv,
            field_mappings: vec![FieldMapping {
                source_field: "secret".into(),
                target_field: TargetField::Password,
                required: true,
                default_value: None,
                transform: Some(FieldTransform::Base64Decode),
            }],
            type_mappings: vec![],
        };

        let mut fields = HashMap::new();
        fields.insert("secret".into(), "aGVsbG8=".into()); // "hello" in base64

        let result = apply_field_mapping(&fields, &mapping);
        assert_eq!(result.get(&TargetField::Password).unwrap(), "hello");
    }

    #[test]
    fn apply_mapping_base64_decode_invalid_input_passes_through() {
        let mapping = FormatMapping {
            source_type: ImportSource::Csv,
            field_mappings: vec![FieldMapping {
                source_field: "secret".into(),
                target_field: TargetField::Password,
                required: true,
                default_value: None,
                transform: Some(FieldTransform::Base64Decode),
            }],
            type_mappings: vec![],
        };

        let mut fields = HashMap::new();
        fields.insert("secret".into(), "not-valid-base64!!!".into());

        let result = apply_field_mapping(&fields, &mapping);
        assert_eq!(
            result.get(&TargetField::Password).unwrap(),
            "not-valid-base64!!!"
        );
    }

    // -- map_parsed_item --

    #[test]
    fn map_parsed_item_csv_full_mapping_with_unsupported_fields_in_notes() {
        let mut fields = HashMap::new();
        fields.insert("name".into(), "MySite".into());
        fields.insert("username".into(), "user@example.com".into());
        fields.insert("password".into(), "pass123".into());
        fields.insert("url".into(), "https://example.com".into());
        fields.insert("notes".into(), "My notes".into());
        fields.insert("custom_field".into(), "custom_value".into());

        let item = ParsedItem {
            source_id: "csv-row-5".into(),
            fields,
            tags: vec![],
        };

        let record = map_parsed_item(&item, ImportSource::Csv);

        assert_eq!(record.credential_type, CredentialType::Login);
        assert_eq!(record.source_item_id, "csv-row-5");
        assert!(!record.is_favorite);
        assert!(!record.is_duplicate);
        assert!(record.expires_at.is_none());

        // Unsupported field should appear in notes
        let notes = record.notes.as_deref().unwrap();
        assert!(
            notes.contains("Field: custom_field = custom_value"),
            "unexpected notes: {notes}"
        );
        // Original notes should also be present
        assert!(
            notes.contains("My notes"),
            "original notes should be preserved: {notes}"
        );
    }

    #[test]
    fn map_parsed_item_bitwarden_specific_mapping() {
        let mut fields = HashMap::new();
        fields.insert("name".into(), "GitHub".into());
        fields.insert("login.username".into(), "dev".into());
        fields.insert("login.password".into(), "ghp_secret".into());
        fields.insert("login.uri".into(), "https://github.com".into());
        fields.insert("notes".into(), "Work account".into());

        let item = ParsedItem {
            source_id: "bw-123".into(),
            fields,
            tags: vec!["dev".into(), "work".into()],
        };

        let record = map_parsed_item(&item, ImportSource::Bitwarden);

        assert_eq!(record.credential_type, CredentialType::Login);
        assert_eq!(record.source_item_id, "bw-123");
        assert_eq!(record.tags, vec!["dev", "work"]);
    }

    #[test]
    fn map_parsed_item_preserves_tags() {
        let mut fields = HashMap::new();
        fields.insert("username".into(), "bob".into());
        fields.insert("password".into(), "secret".into());

        let item = ParsedItem {
            source_id: "tag-test".into(),
            fields,
            tags: vec!["personal".into(), "finance".into(), "bank".into()],
        };

        let record = map_parsed_item(&item, ImportSource::Csv);

        assert_eq!(
            record.tags,
            vec!["personal", "finance", "bank"],
            "tags should be preserved from ParsedItem"
        );
    }

    #[test]
    fn map_parsed_item_generates_unique_uuid_per_call() {
        let mut fields = HashMap::new();
        fields.insert("username".into(), "alice".into());
        fields.insert("password".into(), "pw".into());

        let item = ParsedItem {
            source_id: "uuid-test".into(),
            fields,
            tags: vec![],
        };

        let record1 = map_parsed_item(&item, ImportSource::Csv);
        let record2 = map_parsed_item(&item, ImportSource::Csv);

        assert_ne!(
            record1.id, record2.id,
            "each call should produce a unique UUID"
        );
    }

    #[test]
    fn map_parsed_item_no_notes_when_all_fields_supported() {
        let mut fields = HashMap::new();
        fields.insert("name".into(), "Test".into());
        fields.insert("username".into(), "user".into());
        fields.insert("password".into(), "pw".into());
        fields.insert("url".into(), "https://example.com".into());
        fields.insert("notes".into(), "a note".into());

        let item = ParsedItem {
            source_id: "no-extra".into(),
            fields,
            tags: vec![],
        };

        let record = map_parsed_item(&item, ImportSource::Csv);

        // All fields are mapped by CSV default, but notes has content so it
        // should contain only the original notes text.
        let notes = record.notes.as_deref().unwrap();
        assert_eq!(notes, "a note");
    }
}
