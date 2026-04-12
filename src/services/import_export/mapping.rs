//! Format mapping types and per-format default mapping configurations.
//!
//! Defines how fields from external password-manager exports map to vault fields,
//! including transform rules, required-field markers, and type-inference logic.

use crate::commands::types::{CsvColumnMapping, ImportSource};
use crate::types::CredentialType;

// ---------------------------------------------------------------------------
// Core mapping types
// ---------------------------------------------------------------------------

/// Canonical vault field that an imported column or JSON key maps to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TargetField {
    /// Record name / title.
    Name,
    /// Login username.
    Username,
    /// Login password.
    Password,
    /// API application ID.
    AppId,
    /// API secret key.
    SecretKey,
    /// SSH public key.
    PublicKey,
    /// SSH private key.
    PrivateKey,
    /// SSH passphrase.
    Passphrase,
    /// Website URL.
    Url,
    /// Notes / memo.
    Notes,
    /// Tags list.
    Tags,
    /// Expiration date.
    ExpiresAt,
}

/// Optional transformation applied to a source value before it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldTransform {
    /// Strip leading and trailing whitespace.
    Trim,
    /// Convert the value to lowercase.
    Lowercase,
    /// Convert the value to uppercase.
    Uppercase,
    /// Reformat a datetime string (inner string is the target format pattern).
    DateTimeFormat(String),
    /// Decode a base64-encoded value.
    Base64Decode,
}

/// A single field-level mapping rule: how a source field maps to a vault field.
pub struct FieldMapping {
    /// Source field name (e.g. `"Title"`, `"login.username"`).
    pub source_field: String,
    /// Target vault field.
    pub target_field: TargetField,
    /// Whether the field is required for a valid import.
    pub required: bool,
    /// Fallback value when the source field is absent.
    pub default_value: Option<String>,
    /// Optional transformation applied before storage.
    pub transform: Option<FieldTransform>,
}

/// Maps a source-specific type string to a vault `CredentialType`.
pub struct TypeMapping {
    /// Type identifier in the source format (e.g. `"login"`, `"note"`).
    pub source_type: String,
    /// Resolved vault credential type.
    pub target_type: CredentialType,
    /// Additional field overrides that apply only when this type mapping matches.
    pub field_overrides: Vec<FieldMapping>,
}

/// Complete mapping configuration for a single import format.
pub struct FormatMapping {
    /// The source format this mapping applies to.
    pub source_type: ImportSource,
    /// Default field mappings (applied to every record).
    pub field_mappings: Vec<FieldMapping>,
    /// Type-specific overrides.
    pub type_mappings: Vec<TypeMapping>,
}

// ---------------------------------------------------------------------------
// Per-format default mappings
// ---------------------------------------------------------------------------

/// Default field mapping for KeePass (.kdbx) exports.
pub fn keepass_mapping() -> FormatMapping {
    FormatMapping {
        source_type: ImportSource::KeePass,
        field_mappings: vec![
            FieldMapping {
                source_field: "Title".into(),
                target_field: TargetField::Name,
                required: true,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "UserName".into(),
                target_field: TargetField::Username,
                required: true,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "Password".into(),
                target_field: TargetField::Password,
                required: true,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "URL".into(),
                target_field: TargetField::Url,
                required: false,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "Notes".into(),
                target_field: TargetField::Notes,
                required: false,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "Tags".into(),
                target_field: TargetField::Tags,
                required: false,
                default_value: None,
                transform: None,
            },
        ],
        type_mappings: vec![],
    }
}

/// Default field mapping for 1Password .1pux exports.
pub fn onepassword_1pux_mapping() -> FormatMapping {
    FormatMapping {
        source_type: ImportSource::OnePassword1pux,
        field_mappings: vec![
            FieldMapping {
                source_field: "title".into(),
                target_field: TargetField::Name,
                required: true,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "login.username".into(),
                target_field: TargetField::Username,
                required: true,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "login.password".into(),
                target_field: TargetField::Password,
                required: true,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "login.url".into(),
                target_field: TargetField::Url,
                required: false,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "notesPlain".into(),
                target_field: TargetField::Notes,
                required: false,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "tags".into(),
                target_field: TargetField::Tags,
                required: false,
                default_value: None,
                transform: None,
            },
        ],
        type_mappings: vec![],
    }
}

/// Default field mapping for 1Password .opvault exports.
pub fn onepassword_opvault_mapping() -> FormatMapping {
    FormatMapping {
        source_type: ImportSource::OnePasswordOpvault,
        field_mappings: vec![
            FieldMapping {
                source_field: "overview.title".into(),
                target_field: TargetField::Name,
                required: true,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "login.username".into(),
                target_field: TargetField::Username,
                required: true,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "login.password".into(),
                target_field: TargetField::Password,
                required: true,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "login.url".into(),
                target_field: TargetField::Url,
                required: false,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "notesPlain".into(),
                target_field: TargetField::Notes,
                required: false,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "sections".into(),
                target_field: TargetField::Notes,
                required: false,
                default_value: None,
                transform: None,
            },
        ],
        type_mappings: vec![],
    }
}

/// Default field mapping for Bitwarden .json exports.
pub fn bitwarden_mapping() -> FormatMapping {
    FormatMapping {
        source_type: ImportSource::Bitwarden,
        field_mappings: vec![
            FieldMapping {
                source_field: "name".into(),
                target_field: TargetField::Name,
                required: true,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "login.username".into(),
                target_field: TargetField::Username,
                required: true,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "login.password".into(),
                target_field: TargetField::Password,
                required: true,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "login.uri".into(),
                target_field: TargetField::Url,
                required: false,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "notes".into(),
                target_field: TargetField::Notes,
                required: false,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "fields".into(),
                target_field: TargetField::Notes,
                required: false,
                default_value: None,
                transform: None,
            },
        ],
        type_mappings: vec![],
    }
}

/// Dynamic field mapping for CSV imports based on user-provided column names.
pub fn csv_mapping(column_mapping: &CsvColumnMapping) -> FormatMapping {
    let mut mappings = vec![
        FieldMapping {
            source_field: column_mapping.name_column.clone(),
            target_field: TargetField::Name,
            required: true,
            default_value: None,
            transform: None,
        },
        FieldMapping {
            source_field: column_mapping.username_column.clone(),
            target_field: TargetField::Username,
            required: true,
            default_value: None,
            transform: None,
        },
        FieldMapping {
            source_field: column_mapping.password_column.clone(),
            target_field: TargetField::Password,
            required: true,
            default_value: None,
            transform: None,
        },
        FieldMapping {
            source_field: column_mapping.url_column.clone(),
            target_field: TargetField::Url,
            required: false,
            default_value: None,
            transform: None,
        },
        FieldMapping {
            source_field: column_mapping.notes_column.clone(),
            target_field: TargetField::Notes,
            required: false,
            default_value: None,
            transform: None,
        },
    ];

    if let Some(tags_col) = &column_mapping.tags_column {
        mappings.push(FieldMapping {
            source_field: tags_col.clone(),
            target_field: TargetField::Tags,
            required: false,
            default_value: None,
            transform: None,
        });
    }

    FormatMapping {
        source_type: ImportSource::Csv,
        field_mappings: mappings,
        type_mappings: vec![],
    }
}

/// Default field mapping for OpenKeyring Backup (.okb) exports.
pub fn okb_mapping() -> FormatMapping {
    FormatMapping {
        source_type: ImportSource::OpenKeyringBackup,
        field_mappings: vec![
            FieldMapping {
                source_field: "name".into(),
                target_field: TargetField::Name,
                required: true,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "username".into(),
                target_field: TargetField::Username,
                required: false,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "password".into(),
                target_field: TargetField::Password,
                required: false,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "credential_type".into(),
                target_field: TargetField::Name,
                required: false,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "url".into(),
                target_field: TargetField::Url,
                required: false,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "notes".into(),
                target_field: TargetField::Notes,
                required: false,
                default_value: None,
                transform: None,
            },
            FieldMapping {
                source_field: "tags".into(),
                target_field: TargetField::Tags,
                required: false,
                default_value: None,
                transform: None,
            },
        ],
        type_mappings: vec![
            TypeMapping {
                source_type: "login".into(),
                target_type: CredentialType::Login,
                field_overrides: vec![],
            },
            TypeMapping {
                source_type: "api".into(),
                target_type: CredentialType::Api,
                field_overrides: vec![],
            },
            TypeMapping {
                source_type: "ssh".into(),
                target_type: CredentialType::Ssh,
                field_overrides: vec![],
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// Dispatch helper
// ---------------------------------------------------------------------------

/// Returns the default `FormatMapping` for the given `ImportSource`.
///
/// For the `Csv` variant, call [`csv_mapping`] directly because it requires
/// a [`CsvColumnMapping`] parameter.
pub fn get_default_mapping(source: ImportSource) -> FormatMapping {
    match source {
        ImportSource::KeePass => keepass_mapping(),
        ImportSource::OnePassword1pux => onepassword_1pux_mapping(),
        ImportSource::OnePasswordOpvault => onepassword_opvault_mapping(),
        ImportSource::Bitwarden => bitwarden_mapping(),
        ImportSource::Csv => {
            // CSV requires explicit column mapping; provide a sensible default.
            csv_mapping(&CsvColumnMapping {
                name_column: "name".into(),
                username_column: "username".into(),
                password_column: "password".into(),
                url_column: "url".into(),
                notes_column: "notes".into(),
                tags_column: None,
                skip_header: true,
            })
        }
        ImportSource::OpenKeyringBackup => okb_mapping(),
    }
}

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
    fn okb_mapping_has_7_field_mappings() {
        let m = okb_mapping();
        assert_eq!(m.field_mappings.len(), 7);
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
}
