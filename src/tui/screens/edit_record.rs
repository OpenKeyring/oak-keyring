//! Edit record screen (U7).

use crossterm::event::KeyCode;
use uuid::Uuid;

use crate::tui::state::form_state::{ExpiryOption, FormState};
use crate::tui::state::generator_state::EmbeddedGeneratorState;
use crate::types::credential::CredentialType;

/// Edit record screen state.
pub struct EditRecordScreen {
    pub form: FormState,
    pub generator: EmbeddedGeneratorState,
    pub all_tags: Vec<String>,
}

impl EditRecordScreen {
    pub fn new(record_id: Uuid, credential_type: CredentialType) -> Self {
        Self {
            form: FormState::new_edit(record_id, credential_type),
            generator: EmbeddedGeneratorState::new(),
            all_tags: Vec::new(),
        }
    }

    /// Handle key events. Same logic as CreateRecordScreen but credential type is locked.
    pub fn handle_key(&mut self, key_event: crossterm::event::KeyEvent) -> EditRecordAction {
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

        // If dropdown is expanded, handle dropdown keys (no credential type dropdown in Edit)
        if self.form.expiry_dropdown.expanded {
            return self.handle_expiry_dropdown(key);
        }

        // Normal form navigation (credential type dropdown is disabled)
        match key {
            KeyCode::Tab => {
                self.form.focus_next();
                EditRecordAction::None
            }
            KeyCode::BackTab => {
                self.form.focus_prev();
                EditRecordAction::None
            }
            KeyCode::Esc => {
                if self.form.has_changes {
                    self.form.show_unsaved_dialog = true;
                    EditRecordAction::None
                } else {
                    EditRecordAction::Cancel
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
            KeyCode::Char(c) => self.handle_char_input(c),
            KeyCode::Backspace => self.handle_backspace(),
            _ => EditRecordAction::None,
        }
    }

    fn handle_enter(&mut self) -> EditRecordAction {
        let ct = self.form.credential_type;

        // Toggle expiry dropdown on Enter
        let expiry_idx = match ct {
            CredentialType::Login | CredentialType::Api => 5,
            CredentialType::Ssh => 6,
        };
        if self.form.focused_field == expiry_idx {
            self.form.expiry_dropdown.expanded = true;
            return EditRecordAction::None;
        }

        // Tag input enter
        let tags_idx = match ct {
            CredentialType::Login | CredentialType::Api => 6,
            CredentialType::Ssh => 7,
        };
        if self.form.focused_field == tags_idx && !self.form.fields.tag_input.is_empty() {
            let tag = std::mem::take(&mut self.form.fields.tag_input);
            if !self.form.fields.tags.contains(&tag) {
                self.form.fields.tags.push(tag);
                self.form.has_changes = true;
            }
            return EditRecordAction::None;
        }

        EditRecordAction::None
    }

    fn handle_char_input(&mut self, c: char) -> EditRecordAction {
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
        EditRecordAction::None
    }

    fn handle_backspace(&mut self) -> EditRecordAction {
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
        EditRecordAction::None
    }

    fn attempt_save(&mut self) -> EditRecordAction {
        let errors = crate::tui::screens::form::validation::validate(
            &self.form.fields,
            self.form.credential_type,
        );
        if !errors.is_empty() {
            self.form.validation_errors = errors;
            self.form.focused_field = self.form.validation_errors[0].field_index;
            return EditRecordAction::None;
        }

        // Check weak password (Login only)
        if self.form.credential_type == CredentialType::Login && self.form.is_password_weak() {
            self.form.show_weak_password_dialog = true;
            return EditRecordAction::None;
        }

        EditRecordAction::Save
    }

    fn handle_weak_password_dialog(&mut self, key: KeyCode) -> EditRecordAction {
        match key {
            KeyCode::Esc | KeyCode::Char('n') => {
                self.form.show_weak_password_dialog = false;
                EditRecordAction::None
            }
            KeyCode::Enter | KeyCode::Char('y') => {
                self.form.show_weak_password_dialog = false;
                EditRecordAction::Save
            }
            _ => EditRecordAction::None,
        }
    }

    fn handle_unsaved_dialog(&mut self, key: KeyCode) -> EditRecordAction {
        match key {
            KeyCode::Esc | KeyCode::Char('n') => {
                self.form.show_unsaved_dialog = false;
                EditRecordAction::None
            }
            KeyCode::Enter | KeyCode::Char('y') => EditRecordAction::Cancel,
            _ => EditRecordAction::None,
        }
    }

    fn handle_generator_key(&mut self, key: KeyCode) -> EditRecordAction {
        match key {
            KeyCode::Esc => {
                self.generator.collapse();
                EditRecordAction::None
            }
            KeyCode::Enter => {
                if self.generator.generator.focus
                    == crate::tui::state::generator_state::GeneratorFocus::ActionButton
                {
                    let pw = self.generator.use_password();
                    self.form.fields.password = Some(pw);
                    self.form.fields.update_strength();
                    self.form.has_changes = true;
                    return EditRecordAction::None;
                }
                self.generator.generator.regenerate();
                EditRecordAction::None
            }
            _ => EditRecordAction::None,
        }
    }

    fn handle_expiry_dropdown(&mut self, key: KeyCode) -> EditRecordAction {
        let options = ExpiryOption::all_options();
        match key {
            KeyCode::Up => {
                if self.form.expiry_dropdown.selected_index > 0 {
                    self.form.expiry_dropdown.selected_index -= 1;
                }
                EditRecordAction::None
            }
            KeyCode::Down => {
                if self.form.expiry_dropdown.selected_index < options.len() - 1 {
                    self.form.expiry_dropdown.selected_index += 1;
                }
                EditRecordAction::None
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.form.fields.expires_at = options[self.form.expiry_dropdown.selected_index].1;
                self.form.expiry_dropdown.expanded = false;
                self.form.has_changes = true;
                EditRecordAction::None
            }
            KeyCode::Esc => {
                self.form.expiry_dropdown.expanded = false;
                EditRecordAction::None
            }
            _ => EditRecordAction::None,
        }
    }

    /// Load existing record data into the form.
    #[allow(clippy::too_many_arguments)]
    pub fn load_record_data(
        &mut self,
        name: String,
        url: String,
        username: Option<String>,
        password: Option<String>,
        app_id: Option<String>,
        secret_key: Option<String>,
        public_key: Option<String>,
        private_key: Option<String>,
        passphrase: Option<String>,
        tags: Vec<String>,
        notes: String,
    ) {
        self.form.fields.name = name;
        self.form.fields.url = url;
        self.form.fields.username = username;
        self.form.fields.password = password;
        self.form.fields.app_id = app_id;
        self.form.fields.secret_key = secret_key;
        self.form.fields.public_key = public_key;
        self.form.fields.private_key = private_key;
        self.form.fields.passphrase = passphrase;
        self.form.fields.update_strength();
        self.form.fields.tags = tags;
        self.form.fields.notes = notes;
        self.form.has_changes = false;
    }
}

/// Actions from the edit record screen.
pub enum EditRecordAction {
    None,
    Save,
    Cancel,
}
