//! Format mapping types, per-format default configurations, and the field-mapping engine.
//!
//! Defines how fields from external password-manager exports map to vault fields,
//! including transform rules, required-field markers, and type-inference logic.
//! The engine functions ([`infer_credential_type`], [`apply_field_mapping`],
//! [`map_parsed_item`]) convert parsed source data into [`MappedRecord`] values.

use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::commands::types::{CsvColumnMapping, ImportSource};
use crate::services::import_export::parser::ParsedItem;
use crate::services::import_export::types::MappedRecord;
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
    /// Credential type (login, api, ssh).
    CredentialType,
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
            FieldMapping {
                source_field: "credential_type".into(),
                target_field: TargetField::CredentialType,
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
// Mapping engine
// ---------------------------------------------------------------------------

/// Infer the vault [`CredentialType`] from the fields present in a parsed item.
///
/// Detection rules (checked in order, first match wins):
/// - `credential_type` field present and parsable → use that type (priority for OKB exports)
/// - `username` **and** `password` present → [`CredentialType::Login`]
/// - `app_id` **and** `secret_key` present → [`CredentialType::Api`]
/// - `public_key` **and** `private_key` present → [`CredentialType::Ssh`]
/// - Otherwise → [`CredentialType::Login`] (default)
pub fn infer_credential_type(fields: &HashMap<String, String>) -> CredentialType {
    // Priority 1: Check for explicit credential_type field (from OKB exports)
    if let Some(type_str) = fields.get("credential_type") {
        if let Ok(cred_type) = CredentialType::from_db_str(type_str) {
            return cred_type;
        }
    }

    // Priority 2: Fallback to heuristic field-based detection
    let has = |key: &str| fields.contains_key(key);

    if has("username") && has("password") {
        CredentialType::Login
    } else if has("app_id") && has("secret_key") {
        CredentialType::Api
    } else if has("public_key") && has("private_key") {
        CredentialType::Ssh
    } else {
        CredentialType::Login
    }
}

/// Apply a single [`FieldTransform`] to a value.
fn apply_transform(value: &str, transform: &FieldTransform) -> String {
    match transform {
        FieldTransform::Trim => value.trim().to_string(),
        FieldTransform::Lowercase => value.to_lowercase(),
        FieldTransform::Uppercase => value.to_uppercase(),
        FieldTransform::DateTimeFormat(fmt) => {
            let parsed = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S"));
            match parsed {
                Ok(dt) => dt.format(fmt).to_string(),
                Err(_) => {
                    // Try Unix timestamp
                    if let Ok(ts) = value.parse::<i64>() {
                        if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
                            return dt.format(fmt).to_string();
                        }
                    }
                    value.to_string() // Pass through on parse failure
                }
            }
        }
        FieldTransform::Base64Decode => {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(value)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .unwrap_or_else(|| value.to_string())
        }
    }
}

/// Map source fields through a [`FormatMapping`], producing a `HashMap<TargetField, String>`.
///
/// For each [`FieldMapping`] in the format mapping:
/// 1. Look up `source_field` in the input fields.
/// 2. If found, apply the optional [`FieldTransform`].
/// 3. If not found and not required, use `default_value` if provided.
/// 4. If not found and required, skip (the caller validates required fields separately).
pub fn apply_field_mapping(
    fields: &HashMap<String, String>,
    mapping: &FormatMapping,
) -> HashMap<TargetField, String> {
    let mut result = HashMap::new();

    for fm in &mapping.field_mappings {
        if let Some(value) = fields.get(&fm.source_field) {
            let transformed = match &fm.transform {
                Some(t) => apply_transform(value, t),
                None => value.clone(),
            };
            result.insert(fm.target_field.clone(), transformed);
        } else if let Some(default) = &fm.default_value {
            result.insert(fm.target_field.clone(), default.clone());
        }
        // else: field not found and no default — skip (validation handles required)
    }

    result
}

/// Convert a [`ParsedItem`] into a [`MappedRecord`] using the default mapping
/// for the given [`ImportSource`].
///
/// This is the main entry point that ties together type inference, field mapping,
/// and unmapped-field collection.
pub fn map_parsed_item(item: &ParsedItem, source: ImportSource) -> MappedRecord {
    // 1. Get format mapping
    let mapping = get_default_mapping(source);

    // 2. Infer credential type
    let credential_type = infer_credential_type(&item.fields);

    // 3. Apply field mapping
    let mapped = apply_field_mapping(&item.fields, &mapping);

    // 4. Collect unsupported fields into notes
    let mapped_source_fields: HashSet<String> = mapping
        .field_mappings
        .iter()
        .map(|fm| fm.source_field.clone())
        .collect();

    let mut notes = mapped.get(&TargetField::Notes).cloned().unwrap_or_default();
    for (key, value) in &item.fields {
        if !mapped_source_fields.contains(key) && !value.is_empty() {
            if !notes.is_empty() {
                notes.push('\n');
            }
            notes.push_str(&format!("Field: {} = {}", key, value));
        }
    }

    MappedRecord {
        id: Uuid::new_v4(),
        credential_type,
        fields: item.fields.clone(),
        tags: item.tags.clone(),
        is_favorite: false,
        expires_at: None,
        source_item_id: item.source_id.clone(),
        notes: if notes.is_empty() { None } else { Some(notes) },
        is_duplicate: false,
    }
}
