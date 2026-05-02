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
pub(crate) fn evaluate_rule(rule: &ValidationRule, key: &str, value: &str) -> FieldValidation {
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
        ValidationRuleType::Pattern(pattern) => match regex::Regex::new(pattern) {
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
        },
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
                    message: rule.error_message.clone(),
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
