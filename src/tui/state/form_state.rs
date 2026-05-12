//! Form state for U7 Create/Edit screens.

use chrono::Utc;
use uuid::Uuid;

use crate::crypto::strength::{evaluate_strength, PasswordStrength};
use crate::t;
use crate::types::credential::{CredentialType, EncryptedPayload};
use crate::types::sensitive::{SecureStr, SensitiveInput};

/// Form mode: create new record or edit existing.
#[derive(Debug, Clone)]
pub enum FormMode {
    Create,
    Edit { record_id: Uuid },
}

/// Sub-focus within a password/sensitive field.
/// Tracks which inline button (or the input itself) is focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordFieldFocus {
    Input,
    Show,
    Copy,
    Generate,
    Paste,
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
    pub fn all_options() -> Vec<(String, ExpiryOption)> {
        vec![
            (t!("tui.form.expiry_never").to_string(), ExpiryOption::Never),
            (t!("tui.form.expiry_30d").to_string(), ExpiryOption::Days30),
            (t!("tui.form.expiry_90d").to_string(), ExpiryOption::Days90),
            (t!("tui.form.expiry_1y").to_string(), ExpiryOption::Year1),
            (
                t!("tui.form.expiry_custom").to_string(),
                ExpiryOption::Custom,
            ),
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

    pub fn label(self) -> String {
        match self {
            ExpiryOption::Never => t!("tui.form.expiry_never").to_string(),
            ExpiryOption::Days30 => t!("tui.form.expiry_30d").to_string(),
            ExpiryOption::Days90 => t!("tui.form.expiry_90d").to_string(),
            ExpiryOption::Year1 => t!("tui.form.expiry_1y").to_string(),
            ExpiryOption::Custom => t!("tui.form.expiry_custom").to_string(),
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
#[derive(Debug)]
pub struct FormFields {
    // Common
    pub name: String,
    pub url: String,
    // Login
    pub username: Option<String>,
    pub password: Option<SensitiveInput>,
    pub password_visible: bool,
    pub strength: Option<PasswordStrength>,
    // API
    pub app_id: Option<String>,
    pub secret_key: Option<SensitiveInput>,
    pub secret_visible: bool,
    // SSH
    pub public_key: Option<String>,
    pub private_key: Option<SensitiveInput>,
    pub private_visible: bool,
    pub passphrase: Option<SensitiveInput>,
    pub passphrase_visible: bool,
    // Common tail
    pub expires_at: ExpiryOption,
    pub custom_date: Option<String>,
    pub tags: Vec<String>,
    pub tag_input: String,
    pub notes: String,
}

impl Clone for FormFields {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            url: self.url.clone(),
            username: self.username.clone(),
            password: self.password.as_ref().map(|_| SensitiveInput::new()),
            password_visible: self.password_visible,
            strength: self.strength.clone(),
            app_id: self.app_id.clone(),
            secret_key: self.secret_key.as_ref().map(|_| SensitiveInput::new()),
            secret_visible: self.secret_visible,
            public_key: self.public_key.clone(),
            private_key: self.private_key.as_ref().map(|_| SensitiveInput::new()),
            private_visible: self.private_visible,
            passphrase: self.passphrase.as_ref().map(|_| SensitiveInput::new()),
            passphrase_visible: self.passphrase_visible,
            expires_at: self.expires_at,
            custom_date: self.custom_date.clone(),
            tags: self.tags.clone(),
            tag_input: self.tag_input.clone(),
            notes: self.notes.clone(),
        }
    }
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
                self.password = Some(SensitiveInput::new());
                self.password_visible = false;
            }
            CredentialType::Api => {
                self.app_id = Some(String::new());
                self.secret_key = Some(SensitiveInput::new());
                self.secret_visible = false;
            }
            CredentialType::Ssh => {
                self.public_key = Some(String::new());
                self.private_key = Some(SensitiveInput::new());
                self.private_visible = false;
                self.passphrase = Some(SensitiveInput::new());
                self.passphrase_visible = false;
            }
        }
    }

    /// Update password strength when password changes.
    pub fn update_strength(&mut self) {
        if let Some(ref pw) = self.password {
            if pw.is_empty() {
                self.strength = None;
            } else {
                pw.expose(|s| self.strength = Some(evaluate_strength(s)));
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
    /// Focus index within the weak password dialog: 0 = "Go Back", 1 = "Save Anyway".
    pub weak_dialog_focus: usize,
    pub show_unsaved_dialog: bool,
    /// Sub-focus within the currently focused password/sensitive field.
    /// Only meaningful when `focused_field` points to a field with inline buttons.
    pub password_sub_focus: PasswordFieldFocus,
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
            weak_dialog_focus: 0,
            show_unsaved_dialog: false,
            password_sub_focus: PasswordFieldFocus::Input,
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

    /// Get the ordered list of inline buttons for the currently focused field.
    /// Returns `None` if the focused field does not have inline buttons.
    pub fn inline_buttons(&self) -> Option<Vec<PasswordFieldFocus>> {
        let focused = self.focused_field;
        let ct = self.credential_type;
        match ct {
            CredentialType::Login if focused == 4 => Some(vec![
                PasswordFieldFocus::Generate,
                PasswordFieldFocus::Show,
                PasswordFieldFocus::Copy,
            ]),
            CredentialType::Api if focused == 4 => {
                Some(vec![PasswordFieldFocus::Show, PasswordFieldFocus::Copy])
            }
            CredentialType::Ssh => match focused {
                3 => Some(vec![PasswordFieldFocus::Paste, PasswordFieldFocus::Copy]),
                4 => Some(vec![
                    PasswordFieldFocus::Show,
                    PasswordFieldFocus::Paste,
                    PasswordFieldFocus::Copy,
                ]),
                5 => Some(vec![
                    PasswordFieldFocus::Show,
                    PasswordFieldFocus::Paste,
                    PasswordFieldFocus::Copy,
                ]),
                _ => None,
            },
            _ => None,
        }
    }

    /// Advance sub-focus to the next inline button.
    /// Returns `true` if sub-focus moved (meaning the key was consumed).
    pub fn sub_focus_next(&mut self) -> bool {
        if let Some(buttons) = self.inline_buttons() {
            if self.password_sub_focus == PasswordFieldFocus::Input {
                // Move from input to first button
                self.password_sub_focus = buttons[0];
                return true;
            }
            if let Some(idx) = buttons.iter().position(|b| *b == self.password_sub_focus) {
                if idx + 1 < buttons.len() {
                    self.password_sub_focus = buttons[idx + 1];
                    return true;
                }
            }
            // Already at last button -- do not consume
        }
        false
    }

    /// Move sub-focus to the previous inline button.
    /// Returns `true` if sub-focus moved (meaning the key was consumed).
    pub fn sub_focus_prev(&mut self) -> bool {
        if let Some(buttons) = self.inline_buttons() {
            if let Some(idx) = buttons.iter().position(|b| *b == self.password_sub_focus) {
                if idx > 0 {
                    self.password_sub_focus = buttons[idx - 1];
                    return true;
                }
                // At first button -- move back to Input
                self.password_sub_focus = PasswordFieldFocus::Input;
                return true;
            }
            // Not on any button (e.g. Input) -- do not consume
        }
        false
    }

    /// Get the visibility flag for the currently focused sensitive field.
    pub fn current_field_visible(&self) -> bool {
        let focused = self.focused_field;
        let ct = self.credential_type;
        match ct {
            CredentialType::Login if focused == 4 => self.fields.password_visible,
            CredentialType::Api if focused == 4 => self.fields.secret_visible,
            CredentialType::Ssh => match focused {
                4 => self.fields.private_visible,
                5 => self.fields.passphrase_visible,
                _ => false,
            },
            _ => false,
        }
    }

    /// Toggle visibility for the currently focused sensitive field.
    pub fn toggle_current_visibility(&mut self) {
        let focused = self.focused_field;
        let ct = self.credential_type;
        match ct {
            CredentialType::Login if focused == 4 => {
                self.fields.password_visible = !self.fields.password_visible;
            }
            CredentialType::Api if focused == 4 => {
                self.fields.secret_visible = !self.fields.secret_visible;
            }
            CredentialType::Ssh => match focused {
                4 => self.fields.private_visible = !self.fields.private_visible,
                5 => self.fields.passphrase_visible = !self.fields.passphrase_visible,
                _ => {}
            },
            _ => {}
        }
    }

    /// Get the secret value of the currently focused sensitive field as a string.
    /// Returns `None` if the focused field is not a sensitive field.
    pub fn current_secret_value(&self) -> Option<String> {
        let focused = self.focused_field;
        let ct = self.credential_type;
        match ct {
            CredentialType::Login if focused == 4 => {
                self.fields.password.as_ref().map(|p| p.expose(|s| s.to_string()))
            }
            CredentialType::Api if focused == 4 => {
                self.fields.secret_key.as_ref().map(|k| k.expose(|s| s.to_string()))
            }
            CredentialType::Ssh => match focused {
                3 => self.fields.public_key.as_ref().cloned(),
                4 => self
                    .fields
                    .private_key
                    .as_ref()
                    .map(|k| k.expose(|s| s.to_string())),
                5 => self
                    .fields
                    .passphrase
                    .as_ref()
                    .map(|p| p.expose(|s| s.to_string())),
                _ => None,
            },
            _ => None,
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
            self.password_sub_focus = PasswordFieldFocus::Input;
        }
    }

    /// Move focus to previous field.
    pub fn focus_prev(&mut self) {
        if self.focused_field > 0 {
            self.focused_field -= 1;
            self.password_sub_focus = PasswordFieldFocus::Input;
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

    /// Compute the selected expiry datetime.
    pub fn expiry_datetime(&self) -> Option<chrono::DateTime<Utc>> {
        if self.fields.expires_at == ExpiryOption::Custom {
            let date = self.fields.custom_date.as_ref()?;
            let parsed = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
            let naive = parsed.and_hms_opt(0, 0, 0)?;
            Some(chrono::DateTime::<Utc>::from_naive_utc_and_offset(
                naive, Utc,
            ))
        } else {
            self.fields.expires_at.to_datetime()
        }
    }

    /// Build `EncryptedPayload` from form fields.
    ///
    /// Sensitive fields (`password`, `secret_key`, `private_key`, `passphrase`)
    /// are **moved** out of `FormFields` into `SecureStr`, leaving `None` behind.
    /// This ensures the plaintext `String` does not persist in form state.
    pub fn build_payload(&mut self) -> EncryptedPayload {
        match self.credential_type {
            CredentialType::Login => EncryptedPayload::Login {
                name: std::mem::take(&mut self.fields.name),
                username: self.fields.username.take().unwrap_or_default(),
                password: self
                    .fields
                    .password
                    .as_mut()
                    .map(SensitiveInput::take_secure)
                    .unwrap_or_else(|| SecureStr::new(String::new())),
                url: Some(std::mem::take(&mut self.fields.url)),
                notes: Some(std::mem::take(&mut self.fields.notes)),
            },
            CredentialType::Api => EncryptedPayload::Api {
                name: std::mem::take(&mut self.fields.name),
                app_id: self.fields.app_id.take().unwrap_or_default(),
                secret_key: self
                    .fields
                    .secret_key
                    .as_mut()
                    .map(SensitiveInput::take_secure)
                    .unwrap_or_else(|| SecureStr::new(String::new())),
                url: Some(std::mem::take(&mut self.fields.url)),
                notes: Some(std::mem::take(&mut self.fields.notes)),
            },
            CredentialType::Ssh => EncryptedPayload::Ssh {
                name: std::mem::take(&mut self.fields.name),
                public_key: self.fields.public_key.take().unwrap_or_default(),
                private_key: self
                    .fields
                    .private_key
                    .as_mut()
                    .map(SensitiveInput::take_secure),
                passphrase: self
                    .fields
                    .passphrase
                    .as_mut()
                    .map(SensitiveInput::take_secure),
                notes: Some(std::mem::take(&mut self.fields.notes)),
            },
        }
    }

    /// Clear sensitive fields. Called on screen unmount to ensure
    /// plaintext secrets don't persist in memory after the form is dismissed.
    pub fn clear_sensitive_fields(&mut self) {
        self.fields.password = None;
        self.fields.secret_key = None;
        self.fields.private_key = None;
        self.fields.passphrase = None;
        self.fields.strength = None;
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
        // Test that labels are non-empty and contain expected substrings
        let never_label = ExpiryOption::Never.label();
        let days30_label = ExpiryOption::Days30.label();
        let custom_label = ExpiryOption::Custom.label();

        assert!(!never_label.is_empty());
        assert!(!days30_label.is_empty());
        assert!(!custom_label.is_empty());

        // Verify labels contain expected keywords (works for both EN and ZH)
        assert!(never_label.contains("Never") || never_label.contains("不过期"));
        assert!(days30_label.contains("30") || days30_label.contains("天"));
        assert!(custom_label.contains("Custom") || custom_label.contains("自定义"));
    }

    #[test]
    fn is_password_weak_true() {
        let mut state = FormState::new_create();
        state.fields.password.as_mut().unwrap().push_char('a');
        state.fields.update_strength();
        assert!(state.is_password_weak());
    }

    #[test]
    fn is_password_weak_false_for_strong() {
        let mut state = FormState::new_create();
        for c in "abcd1234ABCD!@ababcd1234".chars() {
            state.fields.password.as_mut().unwrap().push_char(c);
        }
        state.fields.update_strength();
        assert!(!state.is_password_weak());
    }

    #[test]
    fn build_login_payload_moves_sensitive_password() {
        let mut state = FormState::new_create();
        state.fields.name = "Example".into();
        state.fields.url = "https://example.com".into();
        state.fields.username = Some("alice".into());
        for c in "secret".chars() {
            state.fields.password.as_mut().unwrap().push_char(c);
        }
        state.fields.notes = "notes".into();

        let payload = state.build_payload();

        match payload {
            EncryptedPayload::Login {
                name,
                username,
                password,
                url,
                notes,
            } => {
                assert_eq!(name, "Example");
                assert_eq!(username, "alice");
                assert_eq!(password.expose(), "secret");
                assert_eq!(url.as_deref(), Some("https://example.com"));
                assert_eq!(notes.as_deref(), Some("notes"));
            }
            _ => panic!("expected login payload"),
        }
        assert!(state.fields.password.is_some()); // SensitiveInput remains, but is empty
        assert!(state.fields.password.as_ref().unwrap().is_empty());
    }

    #[test]
    fn clear_sensitive_fields_removes_plaintext_secrets() {
        let mut state = FormState::new_edit(Uuid::new_v4(), CredentialType::Ssh);
        for c in "password".chars() {
            state.fields.password.as_mut().unwrap().push_char(c);
        }
        for c in "secret".chars() {
            state.fields.secret_key.as_mut().unwrap().push_char(c);
        }
        for c in "private".chars() {
            state.fields.private_key.as_mut().unwrap().push_char(c);
        }
        for c in "passphrase".chars() {
            state.fields.passphrase.as_mut().unwrap().push_char(c);
        }

        state.clear_sensitive_fields();

        assert!(state.fields.password.is_none());
        assert!(state.fields.secret_key.is_none());
        assert!(state.fields.private_key.is_none());
        assert!(state.fields.passphrase.is_none());
    }

    #[test]
    fn inline_buttons_login_password() {
        let state = FormState::new_create();
        // Login password is field 4
        assert!(state.inline_buttons().is_none()); // field 0 has no buttons
        let mut s = state;
        s.focused_field = 4;
        let buttons = s.inline_buttons().unwrap();
        assert_eq!(
            buttons,
            vec![
                PasswordFieldFocus::Generate,
                PasswordFieldFocus::Show,
                PasswordFieldFocus::Copy,
            ]
        );
    }

    #[test]
    fn sub_focus_next_cycles_through_buttons() {
        let mut state = FormState::new_create();
        state.focused_field = 4; // Login password
        assert_eq!(state.password_sub_focus, PasswordFieldFocus::Input);
        // Right from Input should go to Generate (first button)
        assert!(state.sub_focus_next());
        assert_eq!(state.password_sub_focus, PasswordFieldFocus::Generate);
        assert!(state.sub_focus_next());
        assert_eq!(state.password_sub_focus, PasswordFieldFocus::Show);
        assert!(state.sub_focus_next());
        assert_eq!(state.password_sub_focus, PasswordFieldFocus::Copy);
        // At last button, should not move
        assert!(!state.sub_focus_next());
        assert_eq!(state.password_sub_focus, PasswordFieldFocus::Copy);
    }

    #[test]
    fn sub_focus_prev_from_first_button_moves_to_input() {
        let mut state = FormState::new_create();
        state.focused_field = 4;
        state.password_sub_focus = PasswordFieldFocus::Generate;
        assert!(state.sub_focus_prev());
        assert_eq!(state.password_sub_focus, PasswordFieldFocus::Input);
    }

    #[test]
    fn sub_focus_prev_from_input_does_not_move() {
        let mut state = FormState::new_create();
        state.focused_field = 4;
        state.password_sub_focus = PasswordFieldFocus::Input;
        assert!(!state.sub_focus_prev());
    }

    #[test]
    fn toggle_visibility_login_password() {
        let mut state = FormState::new_create();
        state.focused_field = 4;
        assert!(!state.current_field_visible());
        state.toggle_current_visibility();
        assert!(state.current_field_visible());
        state.toggle_current_visibility();
        assert!(!state.current_field_visible());
    }

    #[test]
    fn current_secret_value_returns_password() {
        let mut state = FormState::new_create();
        for c in "hunter2".chars() {
            state.fields.password.as_mut().unwrap().push_char(c);
        }
        state.focused_field = 4;
        assert_eq!(state.current_secret_value(), Some("hunter2".to_string()));
    }

    #[test]
    fn focus_next_resets_sub_focus() {
        let mut state = FormState::new_create();
        state.focused_field = 4;
        state.password_sub_focus = PasswordFieldFocus::Show;
        state.focus_next(); // moves to field 5
        assert_eq!(state.password_sub_focus, PasswordFieldFocus::Input);
    }
}
