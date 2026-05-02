use super::validation::*;
use crate::commands::types::ImportSource;
use crate::services::import_export::mapping::TargetField;
use crate::services::import_export::parser::ParsedItem;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
