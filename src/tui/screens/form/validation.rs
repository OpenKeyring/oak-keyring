//! Form validation for U7 Create/Edit.

use crate::t;
use crate::tui::state::form_state::{ExpiryOption, FormFields, ValidationError};
use crate::types::credential::CredentialType;
use crate::types::record_limits::{
    char_count, MAX_API_APP_ID_CHARS, MAX_API_SECRET_CHARS, MAX_LOGIN_PASSWORD_CHARS,
    MAX_LOGIN_USERNAME_CHARS, MAX_NOTES_CHARS, MAX_RECORD_NAME_CHARS, MAX_SECURE_NOTE_CHARS,
    MAX_SSH_PASSPHRASE_CHARS, MAX_SSH_PRIVATE_KEY_CHARS, MAX_SSH_PUBLIC_KEY_CHARS,
    MAX_TAGS_PER_RECORD, MAX_TAG_CHARS, MAX_URL_CHARS,
};

/// Validate all required fields. Returns list of errors.
pub fn validate(fields: &FormFields, credential_type: CredentialType) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Name is always required (field index 1)
    if fields.name.trim().is_empty() {
        errors.push(ValidationError {
            field_index: 1,
            message: t!("tui.form.validation_required").to_string(),
        });
    }
    push_len_error(&mut errors, 1, &fields.name, MAX_RECORD_NAME_CHARS);

    match credential_type {
        CredentialType::Login => {
            // Username required (index 3)
            if fields.username.as_ref().is_none_or(|s| s.trim().is_empty()) {
                errors.push(ValidationError {
                    field_index: 3,
                    message: t!("tui.form.validation_required").to_string(),
                });
            }
            if let Some(username) = fields.username.as_ref() {
                push_len_error(&mut errors, 3, username, MAX_LOGIN_USERNAME_CHARS);
            }
            // Password required (index 4)
            if fields
                .password
                .as_ref()
                .is_none_or(|s| s.expose(|v| v.trim().is_empty()))
            {
                errors.push(ValidationError {
                    field_index: 4,
                    message: t!("tui.form.validation_required").to_string(),
                });
            }
            if let Some(password) = fields.password.as_ref() {
                password.expose(|s| push_len_error(&mut errors, 4, s, MAX_LOGIN_PASSWORD_CHARS));
            }
        }
        CredentialType::Api => {
            // AppID required (index 3)
            if fields.app_id.as_ref().is_none_or(|s| s.trim().is_empty()) {
                errors.push(ValidationError {
                    field_index: 3,
                    message: t!("tui.form.validation_required").to_string(),
                });
            }
            if let Some(app_id) = fields.app_id.as_ref() {
                push_len_error(&mut errors, 3, app_id, MAX_API_APP_ID_CHARS);
            }
            // SecretKey required (index 4)
            if fields
                .secret_key
                .as_ref()
                .is_none_or(|s| s.expose(|v| v.trim().is_empty()))
            {
                errors.push(ValidationError {
                    field_index: 4,
                    message: t!("tui.form.validation_required").to_string(),
                });
            }
            if let Some(secret_key) = fields.secret_key.as_ref() {
                secret_key.expose(|s| push_len_error(&mut errors, 4, s, MAX_API_SECRET_CHARS));
            }
        }
        CredentialType::Ssh => {
            // Public key required (index 3)
            if fields
                .public_key
                .as_ref()
                .is_none_or(|s| s.trim().is_empty())
            {
                errors.push(ValidationError {
                    field_index: 3,
                    message: t!("tui.form.validation_required").to_string(),
                });
            }
            if let Some(public_key) = fields.public_key.as_ref() {
                push_len_error(&mut errors, 3, public_key, MAX_SSH_PUBLIC_KEY_CHARS);
            }
            if let Some(private_key) = fields.private_key.as_ref() {
                private_key
                    .expose(|s| push_len_error(&mut errors, 4, s, MAX_SSH_PRIVATE_KEY_CHARS));
            }
            if let Some(passphrase) = fields.passphrase.as_ref() {
                passphrase.expose(|s| push_len_error(&mut errors, 5, s, MAX_SSH_PASSPHRASE_CHARS));
            }
        }
        CredentialType::SecureNote => {
            // No additional required fields - only name (field 1) is required
        }
    }

    let url_index = 2;
    if credential_type != CredentialType::SecureNote {
        push_len_error(&mut errors, url_index, &fields.url, MAX_URL_CHARS);
    }

    let notes_index = match credential_type {
        CredentialType::Login | CredentialType::Api => 7,
        CredentialType::Ssh => 8,
        CredentialType::SecureNote => 2,
    };
    let notes_limit = match credential_type {
        CredentialType::SecureNote => MAX_SECURE_NOTE_CHARS,
        _ => MAX_NOTES_CHARS,
    };
    push_len_error(&mut errors, notes_index, &fields.notes_text(), notes_limit);

    let tags_index = match credential_type {
        CredentialType::Login | CredentialType::Api => 6,
        CredentialType::Ssh => 7,
        CredentialType::SecureNote => 4,
    };
    if fields.tags.len() > MAX_TAGS_PER_RECORD {
        errors.push(ValidationError {
            field_index: tags_index,
            message: t!(
                "tui.form.validation_too_many_tags",
                max = MAX_TAGS_PER_RECORD,
                actual = fields.tags.len()
            )
            .to_string(),
        });
    }
    for tag in &fields.tags {
        push_len_error(&mut errors, tags_index, tag, MAX_TAG_CHARS);
    }
    if !fields.tag_input.is_empty() {
        push_len_error(&mut errors, tags_index, &fields.tag_input, MAX_TAG_CHARS);
    }

    // Custom date validation
    if fields.expires_at == ExpiryOption::Custom {
        let expiry_index = match credential_type {
            CredentialType::Login | CredentialType::Api => 5,
            CredentialType::Ssh => 6,
            CredentialType::SecureNote => 3,
        };
        if let Some(ref date) = fields.custom_date {
            if let Err(msg) = validate_date(date) {
                errors.push(ValidationError {
                    field_index: expiry_index,
                    message: msg,
                });
            }
        } else {
            errors.push(ValidationError {
                field_index: expiry_index,
                message: t!("tui.form.validation_date_format").to_string(),
            });
        }
    }

    errors
}

fn push_len_error(errors: &mut Vec<ValidationError>, field_index: usize, value: &str, max: usize) {
    let actual = char_count(value);
    if actual > max {
        errors.push(ValidationError {
            field_index,
            message: t!("tui.form.validation_too_long", max = max, actual = actual).to_string(),
        });
    }
}

/// Validate a date string in YYYY-MM-DD format.
pub fn validate_date(date: &str) -> Result<(), String> {
    if date.len() != 10 {
        return Err(t!("tui.form.validation_date_format").to_string());
    }

    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return Err(t!("tui.form.validation_date_format").to_string());
    }

    let year: i32 = parts[0]
        .parse()
        .map_err(|_| t!("tui.form.validation_date_format").to_string())?;
    let month: u32 = parts[1]
        .parse()
        .map_err(|_| t!("tui.form.validation_invalid_month").to_string())?;
    let day: u32 = parts[2]
        .parse()
        .map_err(|_| t!("tui.form.validation_invalid_day").to_string())?;

    if month == 0 || month > 12 {
        return Err(t!("tui.form.validation_invalid_month").to_string());
    }

    if day == 0 || day > 31 {
        return Err(t!("tui.form.validation_invalid_day").to_string());
    }

    // Simple month-day validation
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 =>
        {
            #[allow(clippy::manual_is_multiple_of)]
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => return Err(t!("tui.form.validation_invalid_month").to_string()),
    };

    if day > max_day {
        return Err(t!("tui.form.validation_invalid_day").to_string());
    }

    // Check not in the past
    if let Some(parsed) = chrono::NaiveDate::from_ymd_opt(year, month, day) {
        let today = chrono::Local::now().date_naive();
        if parsed < today {
            return Err(t!("tui.form.validation_past_date").to_string());
        }
    }

    Ok(())
}

/// Validate month during input (partial validation).
pub fn validate_month_partial(month_str: &str) -> Option<String> {
    if month_str.len() == 2 {
        if let Ok(m) = month_str.parse::<u32>() {
            if m > 12 {
                return Some(t!("tui.form.validation_invalid_month").to_string());
            }
        }
    }
    None
}

/// Validate day during input (partial validation with given year and month).
pub fn validate_day_partial(year_str: &str, month_str: &str, day_str: &str) -> Option<String> {
    if day_str.len() == 2 {
        let year: u32 = year_str.parse().ok()?;
        let month: u32 = month_str.parse().ok()?;
        let day: u32 = day_str.parse().ok()?;

        if month == 0 || month > 12 {
            return None; // month already invalid, skip day check
        }

        let max_day = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 =>
            {
                #[allow(clippy::manual_is_multiple_of)]
                if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                    29
                } else {
                    28
                }
            }
            _ => return None,
        };

        if day > max_day {
            return Some(t!("tui.form.validation_invalid_day").to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn login_fields() -> FormFields {
        let mut fields = FormFields::new(CredentialType::Login);
        fields.name = "GitHub".into();
        fields.username = Some("user".into());
        for c in "password123".chars() {
            fields.password.as_mut().unwrap().push_char(c);
        }
        fields
    }

    #[test]
    fn valid_login_no_errors() {
        let fields = login_fields();
        let errors = validate(&fields, CredentialType::Login);
        assert!(errors.is_empty());
    }

    #[test]
    fn empty_name_error() {
        let mut fields = login_fields();
        fields.name = String::new();
        let errors = validate(&fields, CredentialType::Login);
        assert!(errors.iter().any(|e| e.field_index == 1));
    }

    #[test]
    fn name_over_limit_errors() {
        let mut fields = login_fields();
        fields.name = "a".repeat(121);
        let errors = validate(&fields, CredentialType::Login);
        assert!(errors.iter().any(|e| e.field_index == 1));
    }

    #[test]
    fn empty_username_error() {
        let mut fields = login_fields();
        fields.username = Some(String::new());
        let errors = validate(&fields, CredentialType::Login);
        assert!(errors.iter().any(|e| e.field_index == 3));
    }

    #[test]
    fn username_over_limit_errors() {
        let mut fields = login_fields();
        fields.username = Some("a".repeat(321));
        let errors = validate(&fields, CredentialType::Login);
        assert!(errors.iter().any(|e| e.field_index == 3));
    }

    #[test]
    fn too_many_tags_errors() {
        let mut fields = login_fields();
        fields.tags = (0..11).map(|n| format!("tag-{n}")).collect();
        let errors = validate(&fields, CredentialType::Login);
        assert!(errors.iter().any(|e| e.field_index == 6));
    }

    #[test]
    fn tag_name_over_limit_errors() {
        let mut fields = login_fields();
        fields.tags = vec!["a".repeat(51)];
        let errors = validate(&fields, CredentialType::Login);
        assert!(errors.iter().any(|e| e.field_index == 6));
    }

    #[test]
    fn login_notes_over_limit_errors() {
        let mut fields = login_fields();
        fields.set_notes_text(&"a".repeat(16_385));
        let errors = validate(&fields, CredentialType::Login);
        assert!(errors.iter().any(|e| e.field_index == 7));
    }

    #[test]
    fn secure_note_allows_larger_notes() {
        let mut fields = FormFields::new(CredentialType::SecureNote);
        fields.name = "note".into();
        fields.set_notes_text(&"a".repeat(16_385));
        let errors = validate(&fields, CredentialType::SecureNote);
        assert!(errors.is_empty());
    }

    #[test]
    fn secure_note_notes_over_limit_errors() {
        let mut fields = FormFields::new(CredentialType::SecureNote);
        fields.name = "note".into();
        fields.set_notes_text(&"a".repeat(65_537));
        let errors = validate(&fields, CredentialType::SecureNote);
        assert!(errors.iter().any(|e| e.field_index == 2));
    }

    #[test]
    fn empty_password_error() {
        let mut fields = login_fields();
        fields.password = Some(crate::types::sensitive::SensitiveInput::new());
        let errors = validate(&fields, CredentialType::Login);
        assert!(errors.iter().any(|e| e.field_index == 4));
    }

    #[test]
    fn api_requires_appid_and_secret() {
        let mut fields = FormFields::new(CredentialType::Api);
        fields.name = "Test".into();
        fields.app_id = Some(String::new());
        fields.secret_key = Some(crate::types::sensitive::SensitiveInput::new());
        let errors = validate(&fields, CredentialType::Api);
        assert!(errors.iter().any(|e| e.field_index == 3));
        assert!(errors.iter().any(|e| e.field_index == 4));
    }

    #[test]
    fn ssh_requires_public_key() {
        let mut fields = FormFields::new(CredentialType::Ssh);
        fields.name = "Server".into();
        fields.public_key = Some(String::new());
        let errors = validate(&fields, CredentialType::Ssh);
        assert!(errors.iter().any(|e| e.field_index == 3));
    }

    #[test]
    fn validate_date_valid() {
        assert!(validate_date("2027-06-30").is_ok());
    }

    #[test]
    fn validate_date_invalid_format() {
        assert!(validate_date("2027-6-30").is_err());
        assert!(validate_date("not-a-date").is_err());
    }

    #[test]
    fn validate_date_invalid_month() {
        assert!(validate_date("2027-13-01").is_err());
    }

    #[test]
    fn validate_date_feb_30_invalid() {
        assert!(validate_date("2027-02-30").is_err());
    }

    #[test]
    fn validate_date_leap_year_feb_29() {
        assert!(validate_date("2028-02-29").is_ok()); // 2028 is leap year
    }

    #[test]
    fn validate_month_partial_over_12() {
        assert_eq!(
            validate_month_partial("13"),
            Some("✗ Invalid month".to_string())
        );
    }

    #[test]
    fn validate_month_partial_valid() {
        assert_eq!(validate_month_partial("06"), None);
    }

    #[test]
    fn validate_day_partial_feb_30() {
        assert_eq!(
            validate_day_partial("2027", "02", "30"),
            Some("✗ Invalid day".to_string())
        );
    }

    #[test]
    fn validate_day_partial_valid() {
        assert_eq!(validate_day_partial("2027", "06", "15"), None);
    }
}
