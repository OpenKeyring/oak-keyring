//! Form state for U7 Create/Edit screens.

use chrono::Utc;
use uuid::Uuid;

use crate::crypto::strength::{evaluate_strength, PasswordStrength};
use crate::types::credential::CredentialType;

/// Form mode: create new record or edit existing.
#[derive(Debug, Clone)]
pub enum FormMode {
    Create,
    Edit { record_id: Uuid },
}

/// Expiry options for the dropdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpiryOption {
    Never,
    Days30,
    Days90,
    Year1,
    Custom,
}

impl ExpiryOption {
    pub fn all_options() -> &'static [(&'static str, ExpiryOption)] {
        &[
            ("永不过期", ExpiryOption::Never),
            ("30 天", ExpiryOption::Days30),
            ("90 天", ExpiryOption::Days90),
            ("1 年", ExpiryOption::Year1),
            ("自定义日期", ExpiryOption::Custom),
        ]
    }

    /// Compute the expiry datetime from the option.
    pub fn to_datetime(self) -> Option<chrono::DateTime<Utc>> {
        match self {
            ExpiryOption::Never => None,
            ExpiryOption::Days30 => Some(Utc::now() + chrono::Duration::days(30)),
            ExpiryOption::Days90 => Some(Utc::now() + chrono::Duration::days(90)),
            ExpiryOption::Year1 => Some(Utc::now() + chrono::Duration::days(365)),
            ExpiryOption::Custom => None, // handled separately
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ExpiryOption::Never => "永不过期",
            ExpiryOption::Days30 => "30 天",
            ExpiryOption::Days90 => "90 天",
            ExpiryOption::Year1 => "1 年",
            ExpiryOption::Custom => "自定义日期",
        }
    }
}

/// Dropdown component state.
#[derive(Debug, Clone, Default)]
pub struct DropdownState {
    pub expanded: bool,
    pub selected_index: usize,
}

/// Validation error for a specific field.
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field_index: usize,
    pub message: String,
}

/// All form fields (union of all credential types).
#[derive(Debug, Clone)]
pub struct FormFields {
    // Common
    pub name: String,
    pub url: String,
    // Login
    pub username: Option<String>,
    pub password: Option<String>,
    pub password_visible: bool,
    pub strength: Option<PasswordStrength>,
    // API
    pub app_id: Option<String>,
    pub secret_key: Option<String>,
    pub secret_visible: bool,
    // SSH
    pub public_key: Option<String>,
    pub private_key: Option<String>,
    pub private_visible: bool,
    pub passphrase: Option<String>,
    pub passphrase_visible: bool,
    // Common tail
    pub expires_at: ExpiryOption,
    pub custom_date: Option<String>,
    pub tags: Vec<String>,
    pub tag_input: String,
    pub notes: String,
}

impl FormFields {
    pub fn new(credential_type: CredentialType) -> Self {
        let mut fields = Self {
            name: String::new(),
            url: String::new(),
            username: None,
            password: None,
            password_visible: false,
            strength: None,
            app_id: None,
            secret_key: None,
            secret_visible: false,
            public_key: None,
            private_key: None,
            private_visible: false,
            passphrase: None,
            passphrase_visible: false,
            expires_at: ExpiryOption::Never,
            custom_date: None,
            tags: Vec::new(),
            tag_input: String::new(),
            notes: String::new(),
        };
        fields.init_for_type(credential_type);
        fields
    }

    fn init_for_type(&mut self, ct: CredentialType) {
        match ct {
            CredentialType::Login => {
                self.username = Some(String::new());
                self.password = Some(String::new());
            }
            CredentialType::Api => {
                self.app_id = Some(String::new());
                self.secret_key = Some(String::new());
            }
            CredentialType::Ssh => {
                self.public_key = Some(String::new());
                self.private_key = Some(String::new());
                self.passphrase = Some(String::new());
            }
        }
    }

    /// Update password strength when password changes.
    pub fn update_strength(&mut self) {
        if let Some(ref pw) = self.password {
            if pw.is_empty() {
                self.strength = None;
            } else {
                self.strength = Some(evaluate_strength(pw));
            }
        }
    }
}

/// Complete form state.
#[derive(Debug, Clone)]
pub struct FormState {
    pub mode: FormMode,
    pub credential_type: CredentialType,
    pub fields: FormFields,
    pub focused_field: usize,
    pub generator_expanded: bool,
    pub has_changes: bool,
    pub validation_errors: Vec<ValidationError>,
    pub expiry_dropdown: DropdownState,
    pub credential_dropdown: DropdownState,
    pub tag_autocomplete: Option<TagAutocompleteState>,
    pub show_weak_password_dialog: bool,
    pub show_unsaved_dialog: bool,
}

/// Tag autocomplete dropdown state.
#[derive(Debug, Clone)]
pub struct TagAutocompleteState {
    pub matches: Vec<String>,
    pub selected_index: usize,
}

impl FormState {
    /// Create a new form for creating a record.
    pub fn new_create() -> Self {
        Self::new(FormMode::Create, CredentialType::Login)
    }

    /// Create a new form for editing a record.
    pub fn new_edit(record_id: Uuid, credential_type: CredentialType) -> Self {
        Self::new(FormMode::Edit { record_id }, credential_type)
    }

    fn new(mode: FormMode, credential_type: CredentialType) -> Self {
        Self {
            mode,
            credential_type,
            fields: FormFields::new(credential_type),
            focused_field: 0,
            generator_expanded: false,
            has_changes: false,
            validation_errors: Vec::new(),
            expiry_dropdown: DropdownState::default(),
            credential_dropdown: DropdownState::default(),
            tag_autocomplete: None,
            show_weak_password_dialog: false,
            show_unsaved_dialog: false,
        }
    }

    /// Switch credential type (Create mode only).
    pub fn switch_credential_type(&mut self, ct: CredentialType) {
        if matches!(self.mode, FormMode::Create) {
            // Preserve common fields
            let name = std::mem::take(&mut self.fields.name);
            let url = std::mem::take(&mut self.fields.url);
            let expires_at = self.fields.expires_at;
            let tags = std::mem::take(&mut self.fields.tags);
            let notes = std::mem::take(&mut self.fields.notes);

            self.credential_type = ct;
            self.fields = FormFields::new(ct);
            self.fields.name = name;
            self.fields.url = url;
            self.fields.expires_at = expires_at;
            self.fields.tags = tags;
            self.fields.notes = notes;
            self.focused_field = 0;
            self.has_changes = true;
        }
    }

    /// Get the number of visible fields for current credential type.
    pub fn field_count(&self) -> usize {
        match self.credential_type {
            CredentialType::Login => 8, // type + name + url + username + password + expiry + tags + notes
            CredentialType::Api => 8, // type + name + url + app_id + secret_key + expiry + tags + notes
            CredentialType::Ssh => 9, // type + name + url + public_key + private_key + passphrase + expiry + tags + notes
        }
    }

    /// Move focus to next field.
    pub fn focus_next(&mut self) {
        let count = self.field_count();
        if self.focused_field < count - 1 {
            self.focused_field += 1;
        }
    }

    /// Move focus to previous field.
    pub fn focus_prev(&mut self) {
        if self.focused_field > 0 {
            self.focused_field -= 1;
        }
    }

    /// Whether the credential type dropdown is interactive.
    pub fn is_credential_type_editable(&self) -> bool {
        matches!(self.mode, FormMode::Create)
    }

    /// Check if current password is weak (for save confirmation).
    pub fn is_password_weak(&self) -> bool {
        self.fields.strength.as_ref().is_some_and(|s| {
            matches!(
                s.level,
                crate::crypto::strength::StrengthLevel::VeryWeak
                    | crate::crypto::strength::StrengthLevel::Weak
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_create_defaults_to_login() {
        let state = FormState::new_create();
        assert_eq!(state.credential_type, CredentialType::Login);
        assert!(state.fields.username.is_some());
        assert!(state.fields.password.is_some());
        assert_eq!(state.focused_field, 0);
        assert!(!state.has_changes);
    }

    #[test]
    fn new_edit_disables_type_switch() {
        let state = FormState::new_edit(Uuid::new_v4(), CredentialType::Ssh);
        assert!(!state.is_credential_type_editable());
        assert!(state.fields.public_key.is_some());
    }

    #[test]
    fn switch_credential_type_preserves_common() {
        let mut state = FormState::new_create();
        state.fields.name = "Test".into();
        state.fields.url = "https://example.com".into();
        state.fields.tags = vec!["work".into()];
        state.switch_credential_type(CredentialType::Api);
        assert_eq!(state.fields.name, "Test");
        assert_eq!(state.fields.url, "https://example.com");
        assert_eq!(state.fields.tags, vec!["work"]);
        assert!(state.fields.app_id.is_some());
        assert!(state.fields.username.is_none());
    }

    #[test]
    fn field_count_login() {
        let state = FormState::new_create();
        assert_eq!(state.field_count(), 8);
    }

    #[test]
    fn field_count_ssh() {
        let state = FormState::new_edit(Uuid::new_v4(), CredentialType::Ssh);
        assert_eq!(state.field_count(), 9);
    }

    #[test]
    fn focus_next_wraps_at_end() {
        let mut state = FormState::new_create();
        state.focused_field = 7;
        state.focus_next();
        assert_eq!(state.focused_field, 7); // stays at last
    }

    #[test]
    fn focus_prev_stops_at_zero() {
        let mut state = FormState::new_create();
        state.focus_prev();
        assert_eq!(state.focused_field, 0);
    }

    #[test]
    fn expiry_option_labels() {
        assert_eq!(ExpiryOption::Never.label(), "永不过期");
        assert_eq!(ExpiryOption::Days30.label(), "30 天");
        assert_eq!(ExpiryOption::Custom.label(), "自定义日期");
    }

    #[test]
    fn is_password_weak_true() {
        let mut state = FormState::new_create();
        state.fields.password = Some("a".into());
        state.fields.update_strength();
        assert!(state.is_password_weak());
    }

    #[test]
    fn is_password_weak_false_for_strong() {
        let mut state = FormState::new_create();
        state.fields.password = Some("abcd1234ABCD!@ababcd1234".into());
        state.fields.update_strength();
        assert!(!state.is_password_weak());
    }
}
