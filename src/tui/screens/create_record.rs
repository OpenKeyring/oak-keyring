//! Create record screen (U7).

use crossterm::event::KeyCode;

use crate::tui::screens::form::validation;
use crate::tui::state::form_state::{ExpiryOption, FormState};
use crate::tui::state::generator_state::EmbeddedGeneratorState;
use crate::types::credential::CredentialType;

/// Create record screen state.
pub struct CreateRecordScreen {
    pub form: FormState,
    pub generator: EmbeddedGeneratorState,
    pub all_tags: Vec<String>,
}

impl Default for CreateRecordScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl CreateRecordScreen {
    pub fn new() -> Self {
        Self {
            form: FormState::new_create(),
            generator: EmbeddedGeneratorState::new(),
            all_tags: Vec::new(),
        }
    }

    /// Handle a key event. Returns action to take.
    pub fn handle_key(&mut self, key_event: crossterm::event::KeyEvent) -> CreateRecordAction {
        let key = key_event.code;
        // If dialogs are showing, handle them first
        if self.form.show_weak_password_dialog {
            return self.handle_weak_password_dialog(key);
        }
        if self.form.show_unsaved_dialog {
            return self.handle_unsaved_dialog(key);
        }

        // If generator is expanded, handle generator keys
        if self.generator.expanded {
            return self.handle_generator_key(key);
        }

        // If dropdown is expanded, handle dropdown keys
        if self.form.credential_dropdown.expanded {
            return self.handle_credential_dropdown(key);
        }
        if self.form.expiry_dropdown.expanded {
            return self.handle_expiry_dropdown(key);
        }

        // Normal form navigation
        match key {
            KeyCode::Tab => {
                self.form.focus_next();
                CreateRecordAction::None
            }
            KeyCode::BackTab => {
                self.form.focus_prev();
                CreateRecordAction::None
            }
            KeyCode::Esc => {
                if self.form.has_changes {
                    self.form.show_unsaved_dialog = true;
                    CreateRecordAction::None
                } else {
                    CreateRecordAction::Cancel
                }
            }
            KeyCode::Char('s')
                if key_event
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.attempt_save()
            }
            KeyCode::Enter => self.handle_enter(),
            KeyCode::Char(c) => {
                if self.is_custom_date_focused() {
                    self.handle_date_char(c)
                } else {
                    self.handle_char_input(c)
                }
            }
            KeyCode::Backspace => {
                if self.is_custom_date_focused() {
                    self.handle_date_backspace()
                } else {
                    self.handle_backspace()
                }
            }
            _ => CreateRecordAction::None,
        }
    }

    fn handle_enter(&mut self) -> CreateRecordAction {
        let focused = self.form.focused_field;
        let ct = self.form.credential_type;

        // Check if focused on a dropdown field - toggle it
        if focused == 0 && self.form.is_credential_type_editable() {
            self.form.credential_dropdown.expanded = true;
            return CreateRecordAction::None;
        }
        let expiry_idx = match ct {
            CredentialType::Login | CredentialType::Api => 5,
            CredentialType::Ssh => 6,
        };
        if focused == expiry_idx {
            self.form.expiry_dropdown.expanded = true;
            return CreateRecordAction::None;
        }

        // If on tag input, add tag
        let tags_idx = match ct {
            CredentialType::Login | CredentialType::Api => 6,
            CredentialType::Ssh => 7,
        };
        if focused == tags_idx && !self.form.fields.tag_input.is_empty() {
            let tag = std::mem::take(&mut self.form.fields.tag_input);
            if !self.form.fields.tags.contains(&tag) {
                self.form.fields.tags.push(tag);
                self.form.has_changes = true;
            }
            return CreateRecordAction::None;
        }

        CreateRecordAction::None
    }

    fn handle_char_input(&mut self, c: char) -> CreateRecordAction {
        let focused = self.form.focused_field;
        let ct = self.form.credential_type;

        match focused {
            1 => {
                self.form.fields.name.push(c);
                self.form.has_changes = true;
            }
            2 => {
                self.form.fields.url.push(c);
                self.form.has_changes = true;
            }
            3 => {
                match ct {
                    CredentialType::Login => {
                        self.form
                            .fields
                            .username
                            .get_or_insert_with(String::new)
                            .push(c);
                    }
                    CredentialType::Api => {
                        self.form
                            .fields
                            .app_id
                            .get_or_insert_with(String::new)
                            .push(c);
                    }
                    CredentialType::Ssh => {
                        self.form
                            .fields
                            .public_key
                            .get_or_insert_with(String::new)
                            .push(c);
                    }
                }
                self.form.has_changes = true;
            }
            4 => {
                match ct {
                    CredentialType::Login => {
                        self.form
                            .fields
                            .password
                            .get_or_insert_with(String::new)
                            .push(c);
                        self.form.fields.update_strength();
                    }
                    CredentialType::Api => {
                        self.form
                            .fields
                            .secret_key
                            .get_or_insert_with(String::new)
                            .push(c);
                    }
                    CredentialType::Ssh => {
                        self.form
                            .fields
                            .private_key
                            .get_or_insert_with(String::new)
                            .push(c);
                    }
                }
                self.form.has_changes = true;
            }
            5 if ct == CredentialType::Ssh => {
                self.form
                    .fields
                    .passphrase
                    .get_or_insert_with(String::new)
                    .push(c);
                self.form.has_changes = true;
            }
            _ => {
                // Handle tags and notes fields
                let tags_idx = match ct {
                    CredentialType::Login | CredentialType::Api => 6,
                    CredentialType::Ssh => 7,
                };
                let notes_idx = match ct {
                    CredentialType::Login | CredentialType::Api => 7,
                    CredentialType::Ssh => 8,
                };
                if focused == tags_idx {
                    self.form.fields.tag_input.push(c);
                    self.form.has_changes = true;
                } else if focused == notes_idx {
                    self.form.fields.notes.push(c);
                    self.form.has_changes = true;
                }
            }
        }
        CreateRecordAction::None
    }

    fn handle_backspace(&mut self) -> CreateRecordAction {
        let focused = self.form.focused_field;
        let ct = self.form.credential_type;

        match focused {
            1 => {
                self.form.fields.name.pop();
            }
            2 => {
                self.form.fields.url.pop();
            }
            3 => match ct {
                CredentialType::Login => {
                    self.form.fields.username.as_mut().and_then(|s| s.pop());
                }
                CredentialType::Api => {
                    self.form.fields.app_id.as_mut().and_then(|s| s.pop());
                }
                CredentialType::Ssh => {
                    self.form.fields.public_key.as_mut().and_then(|s| s.pop());
                }
            },
            4 => match ct {
                CredentialType::Login => {
                    self.form.fields.password.as_mut().and_then(|s| s.pop());
                    self.form.fields.update_strength();
                }
                CredentialType::Api => {
                    self.form.fields.secret_key.as_mut().and_then(|s| s.pop());
                }
                CredentialType::Ssh => {
                    self.form.fields.private_key.as_mut().and_then(|s| s.pop());
                }
            },
            5 if ct == CredentialType::Ssh => {
                self.form.fields.passphrase.as_mut().and_then(|s| s.pop());
            }
            _ => {
                let tags_idx = match ct {
                    CredentialType::Login | CredentialType::Api => 6,
                    CredentialType::Ssh => 7,
                };
                if focused == tags_idx {
                    self.form.fields.tag_input.pop();
                }
            }
        }
        self.form.has_changes = true;
        CreateRecordAction::None
    }

    /// Check if the custom date sub-input is currently focused.
    fn is_custom_date_focused(&self) -> bool {
        let expiry_idx = match self.form.credential_type {
            CredentialType::Login | CredentialType::Api => 5,
            CredentialType::Ssh => 6,
        };
        self.form.focused_field == expiry_idx && self.form.fields.expires_at == ExpiryOption::Custom
    }

    /// Handle smart cursor backspace for YYYY-MM-DD date input.
    fn handle_date_backspace(&mut self) -> CreateRecordAction {
        if let Some(ref mut date) = self.form.fields.custom_date {
            if !date.is_empty() {
                date.pop();
                // If we just deleted a character right after a '-', delete the '-' too
                if date.ends_with('-') {
                    date.pop();
                }
            }
        }
        self.form.has_changes = true;
        CreateRecordAction::None
    }

    /// Handle digit input for YYYY-MM-DD with auto-skip separators.
    fn handle_date_char(&mut self, c: char) -> CreateRecordAction {
        if !c.is_ascii_digit() {
            return CreateRecordAction::None;
        }
        let date = self
            .form
            .fields
            .custom_date
            .get_or_insert_with(|| "    -  -  ".to_string());

        // Find next space position to place the digit
        if let Some(pos) = date.chars().position(|ch| ch == ' ') {
            let mut chars: Vec<char> = date.chars().collect();
            chars[pos] = c;
            *date = chars.into_iter().collect();
        }

        self.form.has_changes = true;
        CreateRecordAction::None
    }

    fn attempt_save(&mut self) -> CreateRecordAction {
        let errors = validation::validate(&self.form.fields, self.form.credential_type);
        if !errors.is_empty() {
            self.form.validation_errors = errors;
            self.form.focused_field = self.form.validation_errors[0].field_index;
            return CreateRecordAction::None;
        }

        // Check weak password (Login only)
        if self.form.credential_type == CredentialType::Login && self.form.is_password_weak() {
            self.form.show_weak_password_dialog = true;
            return CreateRecordAction::None;
        }

        CreateRecordAction::Save
    }

    fn handle_weak_password_dialog(&mut self, key: KeyCode) -> CreateRecordAction {
        match key {
            KeyCode::Esc | KeyCode::Char('n') => {
                self.form.show_weak_password_dialog = false;
                CreateRecordAction::None
            }
            KeyCode::Enter | KeyCode::Char('y') => {
                self.form.show_weak_password_dialog = false;
                CreateRecordAction::Save
            }
            _ => CreateRecordAction::None,
        }
    }

    fn handle_unsaved_dialog(&mut self, key: KeyCode) -> CreateRecordAction {
        match key {
            KeyCode::Esc | KeyCode::Char('n') => {
                self.form.show_unsaved_dialog = false;
                CreateRecordAction::None
            }
            KeyCode::Enter | KeyCode::Char('y') => CreateRecordAction::Cancel,
            _ => CreateRecordAction::None,
        }
    }

    fn handle_generator_key(&mut self, key: KeyCode) -> CreateRecordAction {
        match key {
            KeyCode::Esc => {
                self.generator.collapse();
                CreateRecordAction::None
            }
            KeyCode::Enter => {
                if self.generator.generator.focus
                    == crate::tui::state::generator_state::GeneratorFocus::ActionButton
                {
                    let pw = self.generator.use_password();
                    self.form.fields.password = Some(pw);
                    self.form.fields.update_strength();
                    self.form.has_changes = true;
                    return CreateRecordAction::None;
                }
                self.generator.generator.regenerate();
                CreateRecordAction::None
            }
            _ => CreateRecordAction::None,
        }
    }

    fn handle_credential_dropdown(&mut self, key: KeyCode) -> CreateRecordAction {
        match key {
            KeyCode::Up => {
                if self.form.credential_dropdown.selected_index > 0 {
                    self.form.credential_dropdown.selected_index -= 1;
                }
                CreateRecordAction::None
            }
            KeyCode::Down => {
                if self.form.credential_dropdown.selected_index < 2 {
                    self.form.credential_dropdown.selected_index += 1;
                }
                CreateRecordAction::None
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let ct = match self.form.credential_dropdown.selected_index {
                    0 => CredentialType::Login,
                    1 => CredentialType::Api,
                    _ => CredentialType::Ssh,
                };
                self.form.switch_credential_type(ct);
                self.form.credential_dropdown.expanded = false;
                CreateRecordAction::None
            }
            KeyCode::Esc => {
                self.form.credential_dropdown.expanded = false;
                CreateRecordAction::None
            }
            _ => CreateRecordAction::None,
        }
    }

    fn handle_expiry_dropdown(&mut self, key: KeyCode) -> CreateRecordAction {
        let options = ExpiryOption::all_options();
        match key {
            KeyCode::Up => {
                if self.form.expiry_dropdown.selected_index > 0 {
                    self.form.expiry_dropdown.selected_index -= 1;
                }
                CreateRecordAction::None
            }
            KeyCode::Down => {
                if self.form.expiry_dropdown.selected_index < options.len() - 1 {
                    self.form.expiry_dropdown.selected_index += 1;
                }
                CreateRecordAction::None
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.form.fields.expires_at = options[self.form.expiry_dropdown.selected_index].1;
                self.form.expiry_dropdown.expanded = false;
                self.form.has_changes = true;
                CreateRecordAction::None
            }
            KeyCode::Esc => {
                self.form.expiry_dropdown.expanded = false;
                CreateRecordAction::None
            }
            _ => CreateRecordAction::None,
        }
    }
}

/// Actions from the create record screen.
pub enum CreateRecordAction {
    None,
    Save,
    Cancel,
}
