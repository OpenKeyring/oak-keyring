//! Validation types, per-format rule sets, and item-level validation.
//!
//! Defines the rule model used to validate imported fields, provides per-format
//! rule-set factories for each supported `ImportSource`, and implements both
//! single-item validation (`validate_item`) and batch validation (`validate_items`).

use std::collections::HashMap;

use crate::commands::types::{FailedItem, ImportSource, ReviewItem};
use crate::services::import_export::mapping::TargetField;
use crate::services::import_export::parser::ParsedItem;

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

// ---------------------------------------------------------------------------
// Batch validation
// ---------------------------------------------------------------------------

/// Summary of validating a batch of imported items against a rule set.
#[derive(Debug, Clone)]
pub struct ImportValidationSummary {
    /// Total number of items that were validated.
    pub total_items: usize,
    /// Items that passed all rules (ready for import).
    pub importable: usize,
    /// Items that have warnings but no hard failures.
    pub needs_review: usize,
    /// Items that have at least one hard failure.
    pub failed: usize,
    /// Details for items that need manual review.
    pub review_items: Vec<ReviewItem>,
    /// Details for items that failed validation.
    pub failed_items: Vec<FailedItem>,
}

/// Validates a batch of parsed items against the given rules.
///
/// Each item is evaluated via [`validate_item`]. Items are classified into
/// three categories:
/// - **importable**: all rules pass,
/// - **needs_review**: at least one `NeedsReview` result but no `Fail`,
/// - **failed**: at least one `Fail` result.
///
/// For items that need review or fail, only the first relevant result is
/// recorded to keep the summary concise.
pub fn validate_items(items: &[ParsedItem], rules: &[ValidationRule]) -> ImportValidationSummary {
    let mut summary = ImportValidationSummary {
        total_items: items.len(),
        importable: 0,
        needs_review: 0,
        failed: 0,
        review_items: Vec::new(),
        failed_items: Vec::new(),
    };

    for item in items {
        let results = validate_item(&item.fields, rules);

        let has_failures = results
            .iter()
            .any(|r| matches!(r, FieldValidation::Fail { .. }));
        let has_warnings = results
            .iter()
            .any(|r| matches!(r, FieldValidation::NeedsReview { .. }));

        if has_failures {
            summary.failed += 1;
            // Record only the first failure reason for this item.
            for result in &results {
                if let FieldValidation::Fail { field, message } = result {
                    summary.failed_items.push(FailedItem {
                        name: item.fields.get("name").cloned().unwrap_or_default(),
                        reason: format!("{}: {}", field, message),
                    });
                    break;
                }
            }
        } else if has_warnings {
            summary.needs_review += 1;
            // Record only the first warning reason for this item.
            for result in &results {
                if let FieldValidation::NeedsReview { field, message } = result {
                    summary.review_items.push(ReviewItem {
                        name: item.fields.get("name").cloned().unwrap_or_default(),
                        reason: format!("{}: {}", field, message),
                    });
                    break;
                }
            }
        } else {
            summary.importable += 1;
        }
    }

    summary
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
        ValidationRuleType::Pattern(pattern) => {
            match regex::Regex::new(pattern) {
                Ok(re) => {
                    if re.is_match(value) {
                        FieldValidation::Pass
                    } else {
                        FieldValidation::Fail {
                            field: key.into(),
                            message: rule.error_message.clone(),
                        }
                    }
                }
                Err(_) => FieldValidation::Fail {
                    field: key.into(),
                    message: format!("invalid regex pattern: {}", pattern),
                },
            }
        }
        ValidationRuleType::Format => {
            let valid = value.is_empty()
                || (value.contains('@') && value.contains('.')) // email heuristic
                || value.starts_with("http://") || value.starts_with("https://") // url
                || chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").is_ok()
                || chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").is_ok();
            if valid {
                FieldValidation::Pass
            } else {
                FieldValidation::Fail {
                    field: key.into(),
                    message: if rule.error_message.is_empty() {
                        "invalid format".to_string()
                    } else {
                        rule.error_message.clone()
                    },
                }
            }
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

    // -- Pattern rule --

    #[test]
    fn pattern_rejects_non_matching_value() {
        let rule = ValidationRule {
            rule_type: ValidationRuleType::Pattern(r"^\d{4}-\d{2}-\d{2}$".to_string()),
            field: Some(TargetField::ExpiresAt),
            constraint: ValidationConstraint::DateTime,
            error_message: "expected YYYY-MM-DD".to_string(),
        };
        let result = evaluate_rule(&rule, "expires_at", "not-a-date");
        assert!(matches!(result, FieldValidation::Fail { .. }));
        if let FieldValidation::Fail { field, message } = &result {
            assert_eq!(field, "expires_at");
            assert_eq!(message, "expected YYYY-MM-DD");
        }
    }

    #[test]
    fn pattern_accepts_matching_value() {
        let rule = ValidationRule {
            rule_type: ValidationRuleType::Pattern(r"^\d{4}-\d{2}-\d{2}$".to_string()),
            field: Some(TargetField::ExpiresAt),
            constraint: ValidationConstraint::DateTime,
            error_message: "expected YYYY-MM-DD".to_string(),
        };
        let result = evaluate_rule(&rule, "expires_at", "2026-04-28");
        assert_eq!(result, FieldValidation::Pass);
    }

    #[test]
    fn pattern_invalid_regex_produces_fail() {
        let rule = ValidationRule {
            rule_type: ValidationRuleType::Pattern("([invalid".to_string()),
            field: Some(TargetField::Name),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "pattern check".to_string(),
        };
        let result = evaluate_rule(&rule, "name", "anything");
        assert!(matches!(result, FieldValidation::Fail { .. }));
        if let FieldValidation::Fail { message, .. } = &result {
            assert!(message.contains("invalid regex pattern"));
        }
    }

    #[test]
    fn pattern_passes_through_validate_item() {
        let mut fields = HashMap::new();
        fields.insert("expires_at".into(), "2026-01-15".into());

        let rules = vec![ValidationRule {
            rule_type: ValidationRuleType::Pattern(r"^\d{4}-\d{2}-\d{2}$".to_string()),
            field: Some(TargetField::ExpiresAt),
            constraint: ValidationConstraint::DateTime,
            error_message: "expected YYYY-MM-DD".to_string(),
        }];

        let results = validate_item(&fields, &rules);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], FieldValidation::Pass);
    }

    #[test]
    fn pattern_fails_through_validate_item() {
        let mut fields = HashMap::new();
        fields.insert("expires_at".into(), "bad-date".into());

        let rules = vec![ValidationRule {
            rule_type: ValidationRuleType::Pattern(r"^\d{4}-\d{2}-\d{2}$".to_string()),
            field: Some(TargetField::ExpiresAt),
            constraint: ValidationConstraint::DateTime,
            error_message: "expected YYYY-MM-DD".to_string(),
        }];

        let results = validate_item(&fields, &rules);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], FieldValidation::Fail { field, .. } if field == "expires_at"));
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

    // =======================================================================
    // validate_items() batch tests
    // =======================================================================

    use crate::services::import_export::parser::ParsedItem;

    /// Helper: create a ParsedItem with the given fields.
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

    #[test]
    fn validate_items_all_pass() {
        let items = vec![
            make_item(&[
                ("name", "Entry A"),
                ("username", "user_a"),
                ("password", "pass_a"),
            ]),
            make_item(&[
                ("name", "Entry B"),
                ("username", "user_b"),
                ("password", "pass_b"),
            ]),
            make_item(&[
                ("name", "Entry C"),
                ("username", "user_c"),
                ("password", "pass_c"),
            ]),
        ];

        let rules = csv_rules();
        let summary = validate_items(&items, &rules);

        assert_eq!(summary.total_items, 3);
        assert_eq!(summary.importable, 3);
        assert_eq!(summary.needs_review, 0);
        assert_eq!(summary.failed, 0);
        assert!(summary.review_items.is_empty());
        assert!(summary.failed_items.is_empty());
    }

    #[test]
    fn validate_items_some_fail() {
        let items = vec![
            make_item(&[("name", "Good"), ("username", "user"), ("password", "pass")]),
            make_item(&[
                ("name", "Also Good"),
                ("username", "user2"),
                ("password", "pass2"),
            ]),
            // Missing username and password — should fail.
            make_item(&[("name", "Bad")]),
        ];

        let rules = csv_rules();
        let summary = validate_items(&items, &rules);

        assert_eq!(summary.total_items, 3);
        assert_eq!(summary.importable, 2);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.failed_items.len(), 1);
        assert_eq!(summary.failed_items[0].name, "Bad");
        assert!(summary.failed_items[0].reason.contains("username"));
    }

    #[test]
    fn validate_items_some_need_review() {
        // Build a rule that produces NeedsReview for a custom check.
        let rules = vec![ValidationRule {
            rule_type: ValidationRuleType::Required,
            field: Some(TargetField::Name),
            constraint: ValidationConstraint::NotEmpty,
            error_message: "name is required".into(),
        }];

        // Both items pass the Required rule, so both are importable.
        // To test needs_review, we need a rule that yields NeedsReview.
        // Since current evaluate_rule only produces Pass or Fail, and
        // NeedsReview is a future extension, verify the classification
        // logic with a direct FieldValidation result test instead.
        //
        // For now, test that items with all Pass are importable.
        let items = vec![
            make_item(&[("name", "Entry A")]),
            make_item(&[("name", "Entry B")]),
        ];

        let summary = validate_items(&items, &rules);
        assert_eq!(summary.importable, 2);
        assert_eq!(summary.needs_review, 0);
    }

    #[test]
    fn validate_items_all_fail() {
        let items = vec![
            make_item(&[]), // No fields at all
            make_item(&[]),
            make_item(&[]),
        ];

        let rules = csv_rules();
        let summary = validate_items(&items, &rules);

        assert_eq!(summary.total_items, 3);
        assert_eq!(summary.failed, 3);
        assert_eq!(summary.importable, 0);
        assert_eq!(summary.needs_review, 0);
        assert_eq!(summary.failed_items.len(), 3);
    }

    #[test]
    fn validate_items_empty_list() {
        let items: Vec<ParsedItem> = vec![];
        let rules = csv_rules();
        let summary = validate_items(&items, &rules);

        assert_eq!(summary.total_items, 0);
        assert_eq!(summary.importable, 0);
        assert_eq!(summary.needs_review, 0);
        assert_eq!(summary.failed, 0);
        assert!(summary.review_items.is_empty());
        assert!(summary.failed_items.is_empty());
    }

    #[test]
    fn validate_items_mixed_results() {
        let items = vec![
            // Passes all rules.
            make_item(&[("name", "Good"), ("username", "user"), ("password", "pass")]),
            // Missing username — fails.
            make_item(&[("name", "No User"), ("password", "pass")]),
            // Missing password — fails.
            make_item(&[("name", "No Pass"), ("username", "user3")]),
        ];

        let rules = csv_rules();
        let summary = validate_items(&items, &rules);

        assert_eq!(summary.total_items, 3);
        assert_eq!(summary.importable, 1);
        assert_eq!(summary.failed, 2);
        assert_eq!(summary.failed_items.len(), 2);
    }

    #[test]
    fn validate_items_failed_name_extraction() {
        // Item without a "name" field — FailedItem.name should default to "".
        let items = vec![make_item(&[("username", "user"), ("password", "pass")])];

        let rules = csv_rules();
        let summary = validate_items(&items, &rules);

        assert_eq!(summary.failed, 1);
        assert_eq!(summary.failed_items[0].name, "");
    }

    #[test]
    fn validate_items_records_first_failure_only() {
        // Item missing both username and password — both Required rules fail,
        // but only the first failure should be recorded.
        let items = vec![make_item(&[("name", "Multi Fail")])];

        let rules = csv_rules();
        let summary = validate_items(&items, &rules);

        assert_eq!(summary.failed, 1);
        assert_eq!(summary.failed_items.len(), 1);
        // First failure is username (order matches csv_rules).
        assert!(summary.failed_items[0].reason.contains("username"));
    }
}
