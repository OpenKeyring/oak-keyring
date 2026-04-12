//! Validation types, per-format rule sets, and item-level validation.
//!
//! Defines the rule model used to validate imported fields, provides per-format
//! rule-set factories for each supported `ImportSource`, and implements
//! `validate_item()` which evaluates a set of fields against a rule list.

use std::collections::HashMap;

use crate::commands::types::ImportSource;
use crate::services::import_export::mapping::TargetField;

// ---------------------------------------------------------------------------
// Validation rule types
// ---------------------------------------------------------------------------

/// Classifies the kind of constraint a validation rule enforces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationRuleType {
    /// The field must be present and non-empty.
    Required,
    /// Minimum character length for the field value.
    MinLength(usize),
    /// Maximum character length for the field value.
    MaxLength(usize),
    /// Field value must match a regex-like pattern (stored as string).
    Pattern(String),
    /// Structural/format validation (e.g. datetime, email).
    Format,
    /// Field value must be one of the listed allowed values.
    Enum(Vec<String>),
}

/// Constraint to apply when evaluating a field value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationConstraint {
    /// Value must equal the given string exactly.
    Equals(String),
    /// Value must not be empty.
    NotEmpty,
    /// Value must be parseable as a number.
    Numeric,
    /// Value must resemble an email address.
    Email,
    /// Value must resemble a URL.
    Url,
    /// Value must be parseable as a datetime.
    DateTime,
}

/// A single validation rule: ties a rule type to a target field and constraint.
pub struct ValidationRule {
    /// What kind of check this rule performs.
    pub rule_type: ValidationRuleType,
    /// Which field the rule applies to. `None` means a global / record-level rule.
    pub field: Option<TargetField>,
    /// Additional constraint detail for evaluation.
    pub constraint: ValidationConstraint,
    /// Human-readable message emitted when the rule fails.
    pub error_message: String,
}

// ---------------------------------------------------------------------------
// Per-field validation outcome
// ---------------------------------------------------------------------------

/// Outcome of evaluating a single rule against a single field.
///
/// Named `FieldValidation` to avoid collision with the domain-level
/// `ValidationResult` in `types.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValidation {
    /// Rule passed successfully.
    Pass,
    /// Field is present but may need manual review.
    NeedsReview { field: String, message: String },
    /// Field failed the rule.
    Fail { field: String, message: String },
}

// ---------------------------------------------------------------------------
// Per-format rule factories
// ---------------------------------------------------------------------------

/// Validation rules for KeePass (.kdbx) exports.
pub fn keepass_rules() -> Vec<ValidationRule> {
    vec![
        ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Name),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "Title is required".into(),
        },
        ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Username),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "UserName is required".into(),
        },
        ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Password),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "Password is required".into(),
        },
        ValidationRule {
            rule_type: ValidationRuleType::MaxLength(255),
            field: Some(TargetField::Name),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "Title must be at most 255 characters".into(),
        },
        ValidationRule {
            rule_type: ValidationRuleType::MaxLength(255),
            field: Some(TargetField::Username),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "UserName must be at most 255 characters".into(),
        },
        ValidationRule {
            rule_type: ValidationRuleType::MaxLength(255),
            field: Some(TargetField::Url),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "URL must be at most 255 characters".into(),
        },
    ]
}

/// Validation rules for 1Password .1pux exports.
pub fn onepassword_1pux_rules() -> Vec<ValidationRule> {
    vec![
        ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Name),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "title is required".into(),
        },
        ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Username),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "login.username is required".into(),
        },
        ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Password),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "login.password is required".into(),
        },
        ValidationRule {
            rule_type: ValidationRuleType::MaxLength(255),
            field: Some(TargetField::Name),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "title must be at most 255 characters".into(),
        },
    ]
}

/// Validation rules for 1Password .opvault exports.
pub fn onepassword_opvault_rules() -> Vec<ValidationRule> {
    vec![
        ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Name),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "overview.title is required".into(),
        },
        ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Username),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "login.username is required".into(),
        },
        ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Password),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "login.password is required".into(),
        },
    ]
}

/// Validation rules for Bitwarden .json exports.
pub fn bitwarden_rules() -> Vec<ValidationRule> {
    vec![
        ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Name),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "name is required".into(),
        },
        ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Username),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "login.username is required".into(),
        },
        ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Password),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "login.password is required".into(),
        },
        ValidationRule {
            rule_type: ValidationRuleType::MaxLength(255),
            field: Some(TargetField::Name),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "name must be at most 255 characters".into(),
        },
    ]
}

/// Validation rules for CSV imports.
pub fn csv_rules() -> Vec<ValidationRule> {
    vec![
        ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Name),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "name is required".into(),
        },
        ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Username),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "username is required".into(),
        },
        ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Password),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "password is required".into(),
        },
    ]
}

/// Validation rules for OpenKeyring Backup (.okb) exports.
pub fn okb_rules() -> Vec<ValidationRule> {
    vec![
        ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Name),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "name is required".into(),
        },
        ValidationRule {
            rule_type: ValidationRuleType::Format,
            field: Some(TargetField::Name),
            constraint: ValidationConstraint::Equals("credential_type".into()),
            error_message: "credential_type must be one of: login, api, ssh".into(),
        },
    ]
}

/// Returns the validation rule set for the given import source format.
///
/// Provides an exhaustive match so that adding a new `ImportSource` variant
/// will produce a compile error until rules are defined.
pub fn get_rules_for_format(source: ImportSource) -> Vec<ValidationRule> {
    match source {
        ImportSource::KeePass => keepass_rules(),
        ImportSource::OnePassword1pux => onepassword_1pux_rules(),
        ImportSource::OnePasswordOpvault => onepassword_opvault_rules(),
        ImportSource::Bitwarden => bitwarden_rules(),
        ImportSource::Csv => csv_rules(),
        ImportSource::OpenKeyringBackup => okb_rules(),
    }
}

// ---------------------------------------------------------------------------
// Field-name helper
// ---------------------------------------------------------------------------

/// Converts a `TargetField` to a lowercase string key used in field maps.
fn field_key(field: &TargetField) -> String {
    match field {
        TargetField::Name => "name".into(),
        TargetField::Username => "username".into(),
        TargetField::Password => "password".into(),
        TargetField::AppId => "app_id".into(),
        TargetField::SecretKey => "secret_key".into(),
        TargetField::PublicKey => "public_key".into(),
        TargetField::PrivateKey => "private_key".into(),
        TargetField::Passphrase => "passphrase".into(),
        TargetField::Url => "url".into(),
        TargetField::Notes => "notes".into(),
        TargetField::Tags => "tags".into(),
        TargetField::ExpiresAt => "expires_at".into(),
    }
}

// ---------------------------------------------------------------------------
// Core validation function
// ---------------------------------------------------------------------------

/// Evaluates the given field map against the provided rules.
///
/// Returns one `FieldValidation` per rule. Fields are looked up in the
/// `fields` map by their lowercase key (see `field_key`). When a rule has
/// `field: None` it is treated as a global/record-level check and always passes
/// at this level (global rules are intended for higher-level orchestration).
pub fn validate_item(
    fields: &HashMap<String, String>,
    rules: &[ValidationRule],
) -> Vec<FieldValidation> {
    rules
        .iter()
        .map(|rule| {
            // Global rules (no target field) always pass at item level.
            let target_field = match &rule.field {
                None => return FieldValidation::Pass,
                Some(f) => f,
            };

            let key = field_key(target_field);
            let value = fields.get(&key).map(|s| s.as_str()).unwrap_or("");

            evaluate_rule(rule, &key, value)
        })
        .collect()
}

/// Evaluates a single rule against a field value.
fn evaluate_rule(rule: &ValidationRule, key: &str, value: &str) -> FieldValidation {
    match &rule.rule_type {
        ValidationRuleType::Required => {
            if value.is_empty() {
                FieldValidation::Fail {
                    field: key.into(),
                    message: rule.error_message.clone(),
                }
            } else {
                FieldValidation::Pass
            }
        }
        ValidationRuleType::MinLength(min) => {
            if value.len() < *min {
                FieldValidation::Fail {
                    field: key.into(),
                    message: rule.error_message.clone(),
                }
            } else {
                FieldValidation::Pass
            }
        }
        ValidationRuleType::MaxLength(max) => {
            if value.len() > *max {
                FieldValidation::Fail {
                    field: key.into(),
                    message: rule.error_message.clone(),
                }
            } else {
                FieldValidation::Pass
            }
        }
        ValidationRuleType::Pattern(_) => {
            // Pattern matching is deferred to a later task; treat as pass.
            FieldValidation::Pass
        }
        ValidationRuleType::Format => {
            // Format-level validation is deferred; treat as pass for now.
            FieldValidation::Pass
        }
        ValidationRuleType::Enum(allowed) => {
            if value.is_empty() || allowed.iter().any(|a| a == value) {
                FieldValidation::Pass
            } else {
                FieldValidation::Fail {
                    field: key.into(),
                    message: rule.error_message.clone(),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    // -- Required rule --

    #[test]
    fn required_field_present_passes() {
        let mut fields = HashMap::new();
        fields.insert("name".into(), "My Entry".into());

        let rules = vec![ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Name),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "name is required".into(),
        }];

        let results = validate_item(&fields, &rules);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], FieldValidation::Pass);
    }

    #[test]
    fn required_field_missing_fails() {
        let fields = HashMap::new();

        let rules = vec![ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Name),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "name is required".into(),
        }];

        let results = validate_item(&fields, &rules);
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], FieldValidation::Fail { field, message }
            if field == "name" && message == "name is required")
        );
    }

    #[test]
    fn required_field_empty_string_fails() {
        let mut fields = HashMap::new();
        fields.insert("name".into(), "".into());

        let rules = vec![ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Name),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "name is required".into(),
        }];

        let results = validate_item(&fields, &rules);
        assert!(matches!(&results[0], FieldValidation::Fail { .. }));
    }

    // -- MinLength rule --

    #[test]
    fn min_length_short_value_fails() {
        let mut fields = HashMap::new();
        fields.insert("password".into(), "ab".into());

        let rules = vec![ValidationRule {
            rule_type: ValidationRuleType::MinLength(8),
            field: Some(TargetField::Password),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "password too short".into(),
        }];

        let results = validate_item(&fields, &rules);
        assert!(matches!(&results[0], FieldValidation::Fail { .. }));
    }

    #[test]
    fn min_length_long_enough_passes() {
        let mut fields = HashMap::new();
        fields.insert("password".into(), "longpassword".into());

        let rules = vec![ValidationRule {
            rule_type: ValidationRuleType::MinLength(8),
            field: Some(TargetField::Password),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "password too short".into(),
        }];

        let results = validate_item(&fields, &rules);
        assert_eq!(results[0], FieldValidation::Pass);
    }

    // -- MaxLength rule --

    #[test]
    fn max_length_too_long_fails() {
        let mut fields = HashMap::new();
        fields.insert("name".into(), "a".repeat(300));

        let rules = vec![ValidationRule {
            rule_type: ValidationRuleType::MaxLength(255),
            field: Some(TargetField::Name),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "name too long".into(),
        }];

        let results = validate_item(&fields, &rules);
        assert!(matches!(&results[0], FieldValidation::Fail { .. }));
    }

    #[test]
    fn max_length_within_limit_passes() {
        let mut fields = HashMap::new();
        fields.insert("name".into(), "a".repeat(100));

        let rules = vec![ValidationRule {
            rule_type: ValidationRuleType::MaxLength(255),
            field: Some(TargetField::Name),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "name too long".into(),
        }];

        let results = validate_item(&fields, &rules);
        assert_eq!(results[0], FieldValidation::Pass);
    }

    // -- Per-format rule counts and required fields --

    #[test]
    fn keepass_rules_has_correct_count_and_required_fields() {
        let rules = keepass_rules();
        assert_eq!(rules.len(), 6);

        let required: Vec<&ValidationRule> = rules
            .iter()
            .filter(|r| r.rule_type == ValidationRuleType::Required)
            .collect();
        assert_eq!(required.len(), 3);

        let required_fields: Vec<&TargetField> =
            required.iter().filter_map(|r| r.field.as_ref()).collect();
        assert!(required_fields.contains(&&TargetField::Name));
        assert!(required_fields.contains(&&TargetField::Username));
        assert!(required_fields.contains(&&TargetField::Password));
    }

    #[test]
    fn onepassword_1pux_rules_has_correct_count() {
        let rules = onepassword_1pux_rules();
        assert_eq!(rules.len(), 4);
    }

    #[test]
    fn onepassword_opvault_rules_has_correct_count() {
        let rules = onepassword_opvault_rules();
        assert_eq!(rules.len(), 3);
    }

    #[test]
    fn bitwarden_rules_has_correct_count() {
        let rules = bitwarden_rules();
        assert_eq!(rules.len(), 4);
    }

    #[test]
    fn csv_rules_has_correct_count() {
        let rules = csv_rules();
        assert_eq!(rules.len(), 3);
    }

    #[test]
    fn okb_rules_has_correct_count() {
        let rules = okb_rules();
        assert_eq!(rules.len(), 2);
    }

    // -- Empty fields map: all required rules fail --

    #[test]
    fn empty_fields_all_required_rules_fail() {
        let fields = HashMap::new();
        let rules = keepass_rules();

        let results = validate_item(&fields, &rules);

        let failures: Vec<&FieldValidation> = results
            .iter()
            .filter(|r| matches!(r, FieldValidation::Fail { .. }))
            .collect();

        // All 3 Required rules should fail; MaxLength rules pass on empty.
        assert_eq!(failures.len(), 3);
    }

    // -- get_rules_for_format exhaustiveness for all 6 variants --

    #[test]
    fn get_rules_for_format_returns_non_empty_for_all_variants() {
        let sources = [
            ImportSource::KeePass,
            ImportSource::OnePassword1pux,
            ImportSource::OnePasswordOpvault,
            ImportSource::Bitwarden,
            ImportSource::Csv,
            ImportSource::OpenKeyringBackup,
        ];

        for source in sources {
            let rules = get_rules_for_format(source.clone());
            assert!(
                !rules.is_empty(),
                "rules for {source:?} should not be empty"
            );
        }
    }

    #[test]
    fn get_rules_for_format_keepass_returns_6_rules() {
        assert_eq!(get_rules_for_format(ImportSource::KeePass).len(), 6);
    }

    #[test]
    fn get_rules_for_format_1pux_returns_4_rules() {
        assert_eq!(get_rules_for_format(ImportSource::OnePassword1pux).len(), 4);
    }

    #[test]
    fn get_rules_for_format_opvault_returns_3_rules() {
        assert_eq!(
            get_rules_for_format(ImportSource::OnePasswordOpvault).len(),
            3
        );
    }

    #[test]
    fn get_rules_for_format_bitwarden_returns_4_rules() {
        assert_eq!(get_rules_for_format(ImportSource::Bitwarden).len(), 4);
    }

    #[test]
    fn get_rules_for_format_csv_returns_3_rules() {
        assert_eq!(get_rules_for_format(ImportSource::Csv).len(), 3);
    }

    #[test]
    fn get_rules_for_format_okb_returns_2_rules() {
        assert_eq!(
            get_rules_for_format(ImportSource::OpenKeyringBackup).len(),
            2
        );
    }

    // -- Global rule (field = None) always passes --

    #[test]
    fn global_rule_always_passes() {
        let fields = HashMap::new();
        let rules = vec![ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: None,
            constraint: ValidationConstraint::NotEmpty,
            error_message: "global check".into(),
        }];

        let results = validate_item(&fields, &rules);
        assert_eq!(results[0], FieldValidation::Pass);
    }

    // -- Multiple rules, mixed results --

    #[test]
    fn mixed_rules_produce_mixed_results() {
        let mut fields = HashMap::new();
        fields.insert("name".into(), "Valid Name".into());
        // password is missing on purpose

        let rules = vec![
            ValidationRule {
                rule_type: ValidationRuleType::Required,
                field: Some(TargetField::Name),
                constraint: ValidationConstraint::NotEmpty,
                error_message: "name is required".into(),
            },
            ValidationRule {
                rule_type: ValidationRuleType::Required,
                field: Some(TargetField::Password),
                constraint: ValidationConstraint::NotEmpty,
                error_message: "password is required".into(),
            },
        ];

        let results = validate_item(&fields, &rules);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], FieldValidation::Pass);
        assert!(matches!(&results[1], FieldValidation::Fail { field, .. }
            if field == "password"));
    }
}
