//! Edit record screen (U7).

use crossterm::event::{KeyCode, KeyModifiers};
use tui_textarea::CursorMove;
use uuid::Uuid;

use crate::commands::result::CommandResult;
use crate::commands::{Command, Message};
use crate::tui::components::textarea;
use crate::tui::screens::form::validation;
use crate::tui::state::form_state::{ExpiryOption, FormFooterButton, FormState};
use crate::tui::state::generator_state::{EmbeddedGeneratorState, GenerationStyle, GeneratorFocus};
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
use crate::types::credential::CredentialType;
use crate::types::sensitive::SensitiveInput;
use crate::types::SecureStr;

/// Edit record screen state.
pub struct EditRecordScreen {
    pub form: FormState,
    pub generator: EmbeddedGeneratorState,
    pub all_tags: Vec<String>,
    pub record_id: Option<Uuid>,
    pub record_version: Option<u64>,
}

impl Default for EditRecordScreen {
    fn default() -> Self {
        Self {
            form: FormState::new_edit(Uuid::nil(), CredentialType::Login),
            generator: EmbeddedGeneratorState::new(),
            all_tags: Vec::new(),
            record_id: None,
            record_version: None,
        }
    }
}

impl EditRecordScreen {
    pub fn new(record_id: Uuid, credential_type: CredentialType) -> Self {
        Self {
            form: FormState::new_edit(record_id, credential_type),
            generator: EmbeddedGeneratorState::new(),
            all_tags: Vec::new(),
            record_id: Some(record_id),
            record_version: None,
        }
    }

    /// Handle key events. Same logic as CreateRecordScreen but credential type is locked.
    fn handle_key(
        &mut self,
        key_event: crossterm::event::KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
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
                ScreenResult::Continue
            }
            KeyCode::Down => {
                if self.form.textarea_captures_vertical() {
                    if textarea::cursor_has_line_below(&self.form.fields.notes) {
                        self.form.fields.notes.move_cursor(CursorMove::Down);
                    } else {
                        self.form.focus_next();
                    }
                    ScreenResult::Continue
                } else {
                    self.form.focus_next();
                    ScreenResult::Continue
                }
            }
            KeyCode::BackTab => {
                self.form.focus_prev();
                ScreenResult::Continue
            }
            KeyCode::Up => {
                if self.form.textarea_captures_vertical() {
                    if textarea::cursor_has_line_above(&self.form.fields.notes) {
                        self.form.fields.notes.move_cursor(CursorMove::Up);
                    } else {
                        self.form.focus_prev();
                    }
                    ScreenResult::Continue
                } else {
                    self.form.focus_prev();
                    ScreenResult::Continue
                }
            }
            KeyCode::Right => {
                if self.is_tags_focused() && self.form.fields.focus_next_tag() {
                    return ScreenResult::Continue;
                }
                if self.open_focused_dropdown() {
                    return ScreenResult::Continue;
                }

                // If textarea is focused, handle cursor movement
                if self.form.textarea_captures_vertical() {
                    let (row, col) = self.form.fields.notes.cursor();
                    // Check if we're at the rightmost position of the current line
                    let lines = self.form.fields.notes.lines();
                    if let Some(current_line) = lines.get(row) {
                        if col < current_line.len() {
                            self.form.fields.notes.move_cursor(CursorMove::Forward);
                            return ScreenResult::Continue;
                        }
                    }
                    // At rightmost position, fall through to focus_next
                }

                if self.form.sub_focus_next() {
                    ScreenResult::Continue
                } else {
                    self.form.focus_next();
                    ScreenResult::Continue
                }
            }
            KeyCode::Left => {
                if self.is_tags_focused() && self.form.fields.focus_prev_tag() {
                    return ScreenResult::Continue;
                }

                // If textarea is focused, handle cursor movement
                if self.form.textarea_captures_vertical() {
                    let (_row, col) = self.form.fields.notes.cursor();
                    if col > 0 {
                        self.form.fields.notes.move_cursor(CursorMove::Back);
                        return ScreenResult::Continue;
                    }
                    // At leftmost position, fall through to focus_prev
                }

                if self.form.sub_focus_prev() {
                    ScreenResult::Continue
                } else {
                    self.form.focus_prev();
                    ScreenResult::Continue
                }
            }
            KeyCode::Esc => self.cancel_form(),
            KeyCode::Char('g') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.activate_generate_shortcut(ctx)
            }
            KeyCode::Char('v') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.activate_visibility_shortcut()
            }
            KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.activate_copy_shortcut()
            }
            KeyCode::Char('s') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.attempt_save()
            }
            KeyCode::Enter => self.handle_enter(ctx),
            KeyCode::Char(' ') if self.form.footer_focus.is_some() => self.handle_enter(ctx),
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
            KeyCode::Delete => self.handle_delete(),
            _ => ScreenResult::Continue,
        }
    }

    fn handle_enter(&mut self, ctx: &ScreenContext) -> ScreenResult {
        let ct = self.form.credential_type;

        if let Some(button) = self.form.footer_focus {
            return self.activate_footer_button(button);
        }

        // If notes textarea is focused, insert newline
        let notes_idx = match ct {
            CredentialType::Login | CredentialType::Api => 7,
            CredentialType::Ssh => 8,
            CredentialType::SecureNote => 2,
        };
        if self.form.focused_field == notes_idx {
            if !self.form.can_insert_char_into_current_field('\n') {
                self.form.set_current_limit_error();
                return ScreenResult::Continue;
            }
            self.form.fields.notes.insert_newline();
            self.form.clear_current_limit_error();
            self.form.has_changes = true;
            return ScreenResult::Continue;
        }

        // Check inline button actions first
        match self.form.password_sub_focus {
            crate::tui::state::form_state::PasswordFieldFocus::Show => {
                self.form.toggle_current_visibility();
                return ScreenResult::Continue;
            }
            crate::tui::state::form_state::PasswordFieldFocus::Copy => {
                if let Some(value) = self.form.current_secret_value() {
                    if !value.expose().is_empty() {
                        return ScreenResult::Command(Box::new(Command::CopyRawToClipboard {
                            value,
                        }));
                    }
                }
                return ScreenResult::Continue;
            }
            crate::tui::state::form_state::PasswordFieldFocus::Generate => {
                // Only Login password field has Generate button
                if ct == CredentialType::Login && self.form.focused_field == 4 {
                    self.generator.expand_from_config(&ctx.config.password);
                    return ScreenResult::Continue;
                }
            }
            crate::tui::state::form_state::PasswordFieldFocus::Paste => {
                return ScreenResult::Continue;
            }
            crate::tui::state::form_state::PasswordFieldFocus::Input => {}
        }

        // Toggle expiry dropdown on Enter
        let expiry_idx = match ct {
            CredentialType::Login | CredentialType::Api => 5,
            CredentialType::Ssh => 6,
            CredentialType::SecureNote => 3,
        };
        if self.form.focused_field == expiry_idx {
            self.form.expiry_dropdown.expanded = true;
            return ScreenResult::Continue;
        }

        // Tag input enter
        let tags_idx = match ct {
            CredentialType::Login | CredentialType::Api => 6,
            CredentialType::Ssh => 7,
            CredentialType::SecureNote => 4,
        };
        if self.form.focused_field == tags_idx && !self.form.fields.tag_input.is_empty() {
            if self.form.fields.commit_tag_input() {
                self.form.clear_current_limit_error();
                self.form.has_changes = true;
            }
            return ScreenResult::Continue;
        }

        ScreenResult::Continue
    }

    fn handle_char_input(&mut self, c: char) -> ScreenResult {
        if self.form.footer_focus.is_some() {
            return ScreenResult::Continue;
        }

        // If sub-focus is on a button, don't accept text input
        if self.form.password_sub_focus != crate::tui::state::form_state::PasswordFieldFocus::Input
        {
            return ScreenResult::Continue;
        }

        if !self.form.can_insert_char_into_current_field(c) {
            self.form.set_current_limit_error();
            return ScreenResult::Continue;
        }

        let focused = self.form.focused_field;
        let ct = self.form.credential_type;

        match focused {
            1 => {
                self.form.fields.name.push(c);
                self.form.has_changes = true;
            }
            2 => {
                match ct {
                    CredentialType::Login | CredentialType::Api | CredentialType::Ssh => {
                        self.form.fields.url.push(c);
                        self.form.has_changes = true;
                    }
                    CredentialType::SecureNote => {
                        // Field 2 for SecureNote is notes textarea
                        self.form.fields.notes.insert_char(c);
                        self.form.has_changes = true;
                    }
                }
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
                    CredentialType::SecureNote => {
                        // Field 3 for SecureNote is expiry - no text input
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
                            .get_or_insert_with(SensitiveInput::new)
                            .push_char(c);
                        self.form.fields.update_strength();
                    }
                    CredentialType::Api => {
                        self.form
                            .fields
                            .secret_key
                            .get_or_insert_with(SensitiveInput::new)
                            .push_char(c);
                    }
                    CredentialType::Ssh => {
                        self.form
                            .fields
                            .private_key
                            .get_or_insert_with(SensitiveInput::new)
                            .push_char(c);
                    }
                    CredentialType::SecureNote => {
                        // Field 4 for SecureNote is tags - no text input
                    }
                }
                self.form.has_changes = true;
            }
            5 if ct == CredentialType::Ssh => {
                self.form
                    .fields
                    .passphrase
                    .get_or_insert_with(SensitiveInput::new)
                    .push_char(c);
                self.form.has_changes = true;
            }
            _ => {
                // Handle tags and notes fields
                let tags_idx = match ct {
                    CredentialType::Login | CredentialType::Api => 6,
                    CredentialType::Ssh => 7,
                    CredentialType::SecureNote => 4,
                };
                let notes_idx = match ct {
                    CredentialType::Login | CredentialType::Api => 7,
                    CredentialType::Ssh => 8,
                    CredentialType::SecureNote => 2,
                };
                if focused == tags_idx {
                    if matches!(c, ',' | '，') {
                        if self.form.fields.commit_tag_input() {
                            self.form.has_changes = true;
                        }
                    } else {
                        self.form.fields.tag_focus = None;
                        self.form.fields.tag_input.push(c);
                        self.form.has_changes = true;
                    }
                } else if focused == notes_idx {
                    self.form.fields.notes.insert_char(c);
                    self.form.has_changes = true;
                }
            }
        }
        self.form.clear_current_limit_error();
        ScreenResult::Continue
    }

    fn handle_backspace(&mut self) -> ScreenResult {
        if self.form.footer_focus.is_some() {
            return ScreenResult::Continue;
        }

        // If sub-focus is on a button, don't delete text
        if self.form.password_sub_focus != crate::tui::state::form_state::PasswordFieldFocus::Input
        {
            return ScreenResult::Continue;
        }

        let focused = self.form.focused_field;
        let ct = self.form.credential_type;

        match focused {
            1 => {
                self.form.fields.name.pop();
            }
            2 => match ct {
                CredentialType::Login | CredentialType::Api | CredentialType::Ssh => {
                    self.form.fields.url.pop();
                }
                CredentialType::SecureNote => {
                    // Field 2 for SecureNote is notes textarea
                    self.form.fields.notes.delete_char();
                }
            },
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
                CredentialType::SecureNote => {
                    // Field 3 for SecureNote is expiry - no text input
                }
            },
            4 => match ct {
                CredentialType::Login => {
                    if let Some(s) = self.form.fields.password.as_mut() {
                        s.pop_char()
                    };
                    self.form.fields.update_strength();
                }
                CredentialType::Api => {
                    if let Some(s) = self.form.fields.secret_key.as_mut() {
                        s.pop_char()
                    };
                }
                CredentialType::Ssh => {
                    if let Some(s) = self.form.fields.private_key.as_mut() {
                        s.pop_char()
                    };
                }
                CredentialType::SecureNote => {
                    // Field 4 for SecureNote is tags - no text input
                }
            },
            5 if ct == CredentialType::Ssh => {
                if let Some(s) = self.form.fields.passphrase.as_mut() {
                    s.pop_char()
                };
            }
            _ => {
                let tags_idx = match ct {
                    CredentialType::Login | CredentialType::Api => 6,
                    CredentialType::Ssh => 7,
                    CredentialType::SecureNote => 4,
                };
                let notes_idx = match ct {
                    CredentialType::Login | CredentialType::Api => 7,
                    CredentialType::Ssh => 8,
                    CredentialType::SecureNote => 2,
                };
                if focused == tags_idx {
                    if self.form.fields.tag_input.is_empty() {
                        if self.form.fields.remove_focused_tag() {
                            self.form.has_changes = true;
                            return ScreenResult::Continue;
                        }
                    } else {
                        self.form.fields.tag_input.pop();
                    }
                } else if focused == notes_idx {
                    self.form.fields.notes.delete_char();
                }
            }
        }
        self.form.has_changes = true;
        self.form.clear_current_limit_error();
        ScreenResult::Continue
    }

    fn handle_delete(&mut self) -> ScreenResult {
        if self.is_tags_focused() && self.form.fields.remove_focused_tag() {
            self.form.has_changes = true;
        } else if self.form.textarea_captures_vertical() {
            self.form.fields.notes.delete_next_char();
            self.form.has_changes = true;
        }
        ScreenResult::Continue
    }

    /// Check if the custom date sub-input is currently focused.
    fn is_custom_date_focused(&self) -> bool {
        let expiry_idx = match self.form.credential_type {
            CredentialType::Login | CredentialType::Api => 5,
            CredentialType::Ssh => 6,
            CredentialType::SecureNote => 3,
        };
        self.form.focused_field == expiry_idx && self.form.fields.expires_at == ExpiryOption::Custom
    }

    fn open_focused_dropdown(&mut self) -> bool {
        if self.form.footer_focus.is_some() {
            return false;
        }
        let expiry_idx = match self.form.credential_type {
            CredentialType::Login | CredentialType::Api => 5,
            CredentialType::Ssh => 6,
            CredentialType::SecureNote => 3,
        };
        if self.form.focused_field == expiry_idx {
            self.form.expiry_dropdown.expanded = true;
            return true;
        }
        false
    }

    fn is_tags_focused(&self) -> bool {
        let tags_idx = match self.form.credential_type {
            CredentialType::Login | CredentialType::Api => 6,
            CredentialType::Ssh => 7,
            CredentialType::SecureNote => 4,
        };
        self.form.footer_focus.is_none() && self.form.focused_field == tags_idx
    }

    /// Handle smart cursor backspace for YYYY-MM-DD date input.
    fn handle_date_backspace(&mut self) -> ScreenResult {
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
        ScreenResult::Continue
    }

    /// Handle digit input for YYYY-MM-DD with auto-skip separators.
    fn handle_date_char(&mut self, c: char) -> ScreenResult {
        if !c.is_ascii_digit() {
            return ScreenResult::Continue;
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
        ScreenResult::Continue
    }

    fn attempt_save(&mut self) -> ScreenResult {
        let errors = validation::validate(&self.form.fields, self.form.credential_type);
        if !errors.is_empty() {
            self.form.validation_errors = errors;
            self.form.focused_field = self.form.validation_errors[0].field_index;
            return ScreenResult::Continue;
        }

        // Check weak password (Login only)
        if self.form.credential_type == CredentialType::Login && self.form.is_password_weak() {
            self.form.show_weak_password_dialog = true;
            return ScreenResult::Continue;
        }

        self.update_record_command()
    }

    fn cancel_form(&mut self) -> ScreenResult {
        if self.form.has_changes {
            self.form.show_unsaved_dialog = true;
            self.form.unsaved_dialog_focus = 0;
            ScreenResult::Continue
        } else {
            ScreenResult::NavigateTo(crate::commands::types::Screen::Main)
        }
    }

    fn activate_footer_button(&mut self, button: FormFooterButton) -> ScreenResult {
        match button {
            FormFooterButton::Save => self.attempt_save(),
            FormFooterButton::Cancel => self.cancel_form(),
        }
    }

    fn activate_generate_shortcut(&mut self, ctx: &ScreenContext) -> ScreenResult {
        if self.form.credential_type == CredentialType::Login {
            self.form.focus_field(4);
            self.generator.expand_from_config(&ctx.config.password);
        }
        ScreenResult::Continue
    }

    fn activate_visibility_shortcut(&mut self) -> ScreenResult {
        self.form.focus_field(self.shortcut_secret_field_index());
        self.form.toggle_current_visibility();
        ScreenResult::Continue
    }

    fn activate_copy_shortcut(&mut self) -> ScreenResult {
        self.form.focus_field(self.shortcut_secret_field_index());
        if let Some(value) = self.form.current_secret_value() {
            if !value.expose().is_empty() {
                return ScreenResult::Command(Box::new(Command::CopyRawToClipboard { value }));
            }
        }
        ScreenResult::Continue
    }

    fn shortcut_secret_field_index(&self) -> usize {
        match self.form.credential_type {
            CredentialType::Login | CredentialType::Api | CredentialType::Ssh => 4,
            CredentialType::SecureNote => {
                // SecureNote has no secret field, return notes field as fallback
                2
            }
        }
    }

    fn update_record_command(&mut self) -> ScreenResult {
        ScreenResult::Command(Box::new(Command::UpdateRecord {
            id: self.record_id.unwrap_or_else(Uuid::nil),
            payload: self.form.build_payload(),
            tags: std::mem::take(&mut self.form.fields.tags),
            is_favorite: false,
            expires_at: self.form.expiry_datetime(),
            expected_version: self.record_version.unwrap_or(0),
        }))
    }

    fn handle_weak_password_dialog(&mut self, key: KeyCode) -> ScreenResult {
        match key {
            KeyCode::Esc => {
                self.form.show_weak_password_dialog = false;
                self.form.weak_dialog_focus = 0;
                ScreenResult::Continue
            }
            KeyCode::Left | KeyCode::Tab => {
                self.form.weak_dialog_focus = 0;
                ScreenResult::Continue
            }
            KeyCode::Right => {
                self.form.weak_dialog_focus = 1;
                ScreenResult::Continue
            }
            KeyCode::Enter => {
                let focus = self.form.weak_dialog_focus;
                self.form.show_weak_password_dialog = false;
                self.form.weak_dialog_focus = 0;
                if focus == 0 {
                    // "Cancel" returns to editing.
                    ScreenResult::Continue
                } else {
                    // "Save" continues despite the warning.
                    self.update_record_command()
                }
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_unsaved_dialog(&mut self, key: KeyCode) -> ScreenResult {
        match key {
            KeyCode::Esc | KeyCode::Char('n') => {
                self.form.show_unsaved_dialog = false;
                self.form.unsaved_dialog_focus = 0;
                ScreenResult::Continue
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Left => {
                self.form.unsaved_dialog_focus = 1 - self.form.unsaved_dialog_focus;
                ScreenResult::Continue
            }
            KeyCode::Enter => {
                if self.form.unsaved_dialog_focus == 0 {
                    self.form.show_unsaved_dialog = false;
                    ScreenResult::Continue
                } else {
                    ScreenResult::NavigateTo(crate::commands::types::Screen::Main)
                }
            }
            KeyCode::Char('y') => {
                self.form.unsaved_dialog_focus = 1;
                ScreenResult::NavigateTo(crate::commands::types::Screen::Main)
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_generator_key(&mut self, key: KeyCode) -> ScreenResult {
        match key {
            KeyCode::Esc => {
                self.generator.collapse();
                ScreenResult::Continue
            }
            KeyCode::Down => {
                self.generator.generator.focus_section_down();
                ScreenResult::Continue
            }
            KeyCode::Up => {
                self.generator.generator.focus_section_up();
                ScreenResult::Continue
            }
            KeyCode::Tab => {
                self.generator_focus_next();
                ScreenResult::Continue
            }
            KeyCode::BackTab => {
                self.generator_focus_prev();
                ScreenResult::Continue
            }
            KeyCode::Char('r') => {
                self.generator.generator.regenerate();
                ScreenResult::Continue
            }
            KeyCode::Char('y') => {
                if self.generator.generator.focus == GeneratorFocus::SeparatorInput {
                    self.generator.generator.memorable_config.separator = "y".to_string();
                    self.generator.generator.regenerate();
                } else {
                    let pw = self.generator.use_password();
                    self.form.fields.password = Some(SensitiveInput::from(pw));
                    self.form.fields.update_strength();
                    self.form.has_changes = true;
                }
                ScreenResult::Continue
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                if self.generator.generator.focus == GeneratorFocus::SeparatorInput {
                    self.generator.generator.memorable_config.separator = "+".to_string();
                    self.generator.generator.regenerate();
                } else {
                    self.generator.generator.increment_length();
                }
                ScreenResult::Continue
            }
            KeyCode::Char('-') => {
                if self.generator.generator.focus == GeneratorFocus::SeparatorInput {
                    self.generator.generator.memorable_config.separator = "-".to_string();
                    self.generator.generator.regenerate();
                } else {
                    self.generator.generator.decrement_length();
                }
                ScreenResult::Continue
            }
            KeyCode::Enter | KeyCode::Char(' ') => self.activate_generator_focus(),
            _ => self.handle_generator_focus_key(key),
        }
    }

    fn handle_generator_focus_key(&mut self, key: KeyCode) -> ScreenResult {
        match self.generator.generator.focus {
            GeneratorFocus::LengthSlider => match key {
                KeyCode::Left => {
                    self.generator.generator.decrement_length();
                    ScreenResult::Continue
                }
                KeyCode::Right => {
                    self.generator.generator.increment_length();
                    ScreenResult::Continue
                }
                _ => ScreenResult::Continue,
            },
            GeneratorFocus::Toggle(idx) => match key {
                KeyCode::Left => {
                    self.generator.generator.focus_prev_toggle();
                    ScreenResult::Continue
                }
                KeyCode::Right => {
                    self.generator.generator.focus_next_toggle();
                    ScreenResult::Continue
                }
                _ => {
                    let _ = idx;
                    ScreenResult::Continue
                }
            },
            GeneratorFocus::SeparatorInput => match key {
                KeyCode::Left => {
                    self.generator.generator.focus_prev_toggle();
                    ScreenResult::Continue
                }
                KeyCode::Right => {
                    self.generator.generator.focus_next_toggle();
                    ScreenResult::Continue
                }
                KeyCode::Char(c) => {
                    self.generator.generator.memorable_config.separator = c.to_string();
                    self.generator.generator.regenerate();
                    ScreenResult::Continue
                }
                KeyCode::Backspace => {
                    self.generator.generator.memorable_config.separator = "-".to_string();
                    self.generator.generator.regenerate();
                    ScreenResult::Continue
                }
                _ => ScreenResult::Continue,
            },
            GeneratorFocus::RegenerateButton => match key {
                KeyCode::Right => {
                    self.generator.generator.focus = GeneratorFocus::ActionButton;
                    ScreenResult::Continue
                }
                _ => ScreenResult::Continue,
            },
            GeneratorFocus::ActionButton => match key {
                KeyCode::Left => {
                    self.generator.generator.focus = GeneratorFocus::RegenerateButton;
                    ScreenResult::Continue
                }
                _ => ScreenResult::Continue,
            },
            _ => ScreenResult::Continue,
        }
    }

    fn activate_generator_focus(&mut self) -> ScreenResult {
        match self.generator.generator.focus {
            GeneratorFocus::ActionButton => {
                let pw = self.generator.use_password();
                self.form.fields.password = Some(SensitiveInput::from(pw));
                self.form.fields.update_strength();
                self.form.has_changes = true;
                ScreenResult::Continue
            }
            GeneratorFocus::RegenerateButton => {
                self.generator.generator.regenerate();
                ScreenResult::Continue
            }
            GeneratorFocus::Toggle(idx) => {
                self.toggle_generator_option(idx);
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn toggle_generator_option(&mut self, idx: usize) {
        if self.generator.generator.style == GenerationStyle::Random {
            self.generator.generator.toggle_char_type(idx);
        } else if self.generator.generator.style == GenerationStyle::Memorable && idx == 0 {
            self.generator.generator.memorable_config.capitalize =
                !self.generator.generator.memorable_config.capitalize;
            self.generator.generator.regenerate();
        }
    }

    fn embedded_generator_focus_order(&self) -> Vec<GeneratorFocus> {
        self.generator
            .generator
            .focus_order()
            .into_iter()
            .filter(|focus| *focus != GeneratorFocus::StyleSelector)
            .collect()
    }

    fn generator_focus_next(&mut self) {
        let order = self.embedded_generator_focus_order();
        if let Some(idx) = order
            .iter()
            .position(|focus| *focus == self.generator.generator.focus)
        {
            self.generator.generator.focus = order[(idx + 1) % order.len()];
        }
    }

    fn generator_focus_prev(&mut self) {
        let order = self.embedded_generator_focus_order();
        if let Some(idx) = order
            .iter()
            .position(|focus| *focus == self.generator.generator.focus)
        {
            self.generator.generator.focus = order[(idx + order.len() - 1) % order.len()];
        }
    }

    fn handle_expiry_dropdown(&mut self, key: KeyCode) -> ScreenResult {
        let options = ExpiryOption::all_options();
        match key {
            KeyCode::Up => {
                if self.form.expiry_dropdown.selected_index > 0 {
                    self.form.expiry_dropdown.selected_index -= 1;
                }
                ScreenResult::Continue
            }
            KeyCode::Down => {
                if self.form.expiry_dropdown.selected_index < options.len() - 1 {
                    self.form.expiry_dropdown.selected_index += 1;
                }
                ScreenResult::Continue
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char(' ') => {
                self.form.fields.expires_at = options[self.form.expiry_dropdown.selected_index].1;
                self.form.expiry_dropdown.expanded = false;
                self.form.has_changes = true;
                ScreenResult::Continue
            }
            KeyCode::Esc | KeyCode::Left => {
                self.form.expiry_dropdown.expanded = false;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    /// Load existing record data into the form.
    #[allow(clippy::too_many_arguments)]
    pub fn load_record_data(
        &mut self,
        name: String,
        url: String,
        username: Option<String>,
        password: Option<SecureStr>,
        app_id: Option<String>,
        secret_key: Option<SecureStr>,
        public_key: Option<String>,
        private_key: Option<SecureStr>,
        passphrase: Option<SecureStr>,
        tags: Vec<String>,
        notes: String,
    ) {
        self.form.fields.name = name;
        self.form.fields.url = url;
        self.form.fields.username = username;
        self.form.fields.password = password.map(SensitiveInput::from);
        self.form.fields.app_id = app_id;
        self.form.fields.secret_key = secret_key.map(SensitiveInput::from);
        self.form.fields.public_key = public_key;
        self.form.fields.private_key = private_key.map(SensitiveInput::from);
        self.form.fields.passphrase = passphrase.map(SensitiveInput::from);
        self.form.fields.update_strength();
        self.form.fields.tags = tags;
        self.form.fields.set_notes_text(&notes);
        self.form.has_changes = false;
    }
}

impl Screen for EditRecordScreen {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult {
        match msg {
            Message::KeyEvent(key) => self.handle_key(key, ctx),
            Message::CommandCompleted(result) => match result {
                CommandResult::TagsLoaded { tags, tag_stats: _ } => {
                    self.all_tags = tags.into_iter().map(|tag| tag.name).collect();
                    ScreenResult::Continue
                }
                CommandResult::RecordForEditLoaded { record } => {
                    let rec_id = record.id();
                    let rec_tags = record.tags().to_vec();
                    match record {
                        crate::types::record::DecryptedRecord::Login {
                            version,
                            name,
                            url,
                            username,
                            password,
                            notes,
                            ..
                        } => {
                            self.record_version = Some(version);
                            self.load_record_data(
                                name,
                                url.unwrap_or_default(),
                                Some(username),
                                Some(password),
                                None,
                                None,
                                None,
                                None,
                                None,
                                rec_tags,
                                notes.unwrap_or_default(),
                            );
                        }
                        crate::types::record::DecryptedRecord::Api {
                            version,
                            name,
                            url,
                            app_id,
                            secret_key,
                            notes,
                            ..
                        } => {
                            self.record_version = Some(version);
                            self.load_record_data(
                                name,
                                url.unwrap_or_default(),
                                None,
                                None,
                                Some(app_id),
                                Some(secret_key),
                                None,
                                None,
                                None,
                                rec_tags,
                                notes.unwrap_or_default(),
                            );
                        }
                        crate::types::record::DecryptedRecord::Ssh {
                            version,
                            name,
                            public_key,
                            private_key,
                            passphrase,
                            notes,
                            ..
                        } => {
                            self.record_version = Some(version);
                            self.load_record_data(
                                name,
                                String::new(),
                                None,
                                None,
                                None,
                                None,
                                Some(public_key),
                                private_key,
                                passphrase,
                                rec_tags,
                                notes.unwrap_or_default(),
                            );
                        }
                        crate::types::record::DecryptedRecord::SecureNote {
                            version,
                            name,
                            notes,
                            ..
                        } => {
                            self.record_version = Some(version);
                            self.load_record_data(
                                name,
                                String::new(),
                                None,
                                None,
                                None,
                                None,
                                None,
                                None,
                                None,
                                rec_tags,
                                notes.unwrap_or_default(),
                            );
                        }
                    }
                    self.record_id = Some(rec_id);
                    ScreenResult::Continue
                }
                CommandResult::RecordUpdated { .. } => ScreenResult::PopScreen,
                _ => ScreenResult::Continue,
            },
            _ => ScreenResult::Continue,
        }
    }

    fn view(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        crate::tui::screens::form::render::render_form(
            frame,
            area,
            &self.form,
            Some(&self.generator),
            &self.all_tags,
            true, // unicode - TODO: wire up from AppState
        );
    }

    fn on_mount(&mut self, ctx: &mut ScreenContext) {
        if let Some(id) = self.record_id {
            self.form = FormState::new_edit(id, self.form.credential_type);
            self.all_tags.clear();
            ctx.send_system_command(Command::LoadRecordForEdit { id });
        }
        ctx.send_system_command(Command::LoadTags);
    }

    fn on_unmount(&mut self) {
        self.form.clear_sensitive_fields();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::result::CommandResult;
    use crate::types::tag::Tag;
    use std::collections::HashMap;

    use tokio::sync::mpsc;

    fn make_screen() -> EditRecordScreen {
        EditRecordScreen::default()
    }

    fn ctrl(ch: char) -> crossterm::event::KeyEvent {
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(ch),
            crossterm::event::KeyModifiers::CONTROL,
        )
    }

    fn key(code: crossterm::event::KeyCode) -> crossterm::event::KeyEvent {
        crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE)
    }

    struct TestEnv {
        config: crate::config::AppConfig,
    }

    impl TestEnv {
        fn new() -> Self {
            Self {
                config: crate::config::AppConfig::default(),
            }
        }

        fn make_ctx<'a>(&'a self, tx: &'a mpsc::Sender<Command>) -> ScreenContext<'a> {
            ScreenContext {
                command_tx: tx,
                config: &self.config,
            }
        }
    }

    #[test]
    fn on_mount_with_record_id_sends_load_record_and_tags() {
        let (tx, mut rx) = mpsc::channel(2);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.record_id = Some(uuid::Uuid::new_v4());
        screen.on_mount(&mut ctx);
        let cmds: Vec<_> = (0..2).filter_map(|_| rx.try_recv().ok()).collect();
        assert_eq!(cmds.len(), 2);
        assert!(cmds
            .iter()
            .any(|c| matches!(c, Command::LoadRecordForEdit { .. })));
        assert!(cmds.iter().any(|c| matches!(c, Command::LoadTags)));
    }

    #[test]
    fn on_mount_without_record_id_only_sends_load_tags() {
        let (tx, mut rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.on_mount(&mut ctx);
        let cmd = rx.try_recv().unwrap();
        assert!(matches!(cmd, Command::LoadTags));
    }

    #[test]
    fn update_record_updated_pops_screen() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        let result = screen.update(
            Message::CommandCompleted(CommandResult::RecordUpdated {
                id: uuid::Uuid::new_v4(),
            }),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::PopScreen));
    }

    #[test]
    fn update_tags_loaded_populates_all_tags() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        let tags = vec![Tag {
            id: 1,
            name: "work".into(),
        }];
        let result = screen.update(
            Message::CommandCompleted(CommandResult::TagsLoaded {
                tags,
                tag_stats: HashMap::new(),
            }),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.all_tags.len(), 1);
    }

    #[test]
    fn notes_textarea_up_at_first_line_moves_to_previous_field() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.fields.set_notes_text("first\nsecond");
        screen.form.focus_field(7);
        screen.form.fields.notes.move_cursor(CursorMove::Top);

        let result = screen.update(
            Message::KeyEvent(key(crossterm::event::KeyCode::Up)),
            &mut ctx,
        );

        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.form.focused_field, 6);
    }

    #[test]
    fn notes_textarea_down_at_last_line_moves_to_footer() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.fields.set_notes_text("first\nsecond");
        screen.form.focus_field(7);
        screen.form.fields.notes.move_cursor(CursorMove::Bottom);

        let result = screen.update(
            Message::KeyEvent(key(crossterm::event::KeyCode::Down)),
            &mut ctx,
        );

        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.form.footer_focus, Some(FormFooterButton::Save));
    }

    #[test]
    fn weak_dialog_esc_returns_to_edit() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.show_weak_password_dialog = true;
        screen.form.weak_dialog_focus = 1;
        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Esc,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert!(!screen.form.show_weak_password_dialog);
        assert_eq!(screen.form.weak_dialog_focus, 0);
    }

    #[test]
    fn weak_dialog_left_focuses_cancel() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.show_weak_password_dialog = true;
        screen.form.weak_dialog_focus = 1;
        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Left,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.form.weak_dialog_focus, 0);
        assert!(screen.form.show_weak_password_dialog);
    }

    #[test]
    fn weak_dialog_right_focuses_save() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.show_weak_password_dialog = true;
        screen.form.weak_dialog_focus = 0;
        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Right,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.form.weak_dialog_focus, 1);
        assert!(screen.form.show_weak_password_dialog);
    }

    #[test]
    fn weak_dialog_tab_focuses_cancel() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.show_weak_password_dialog = true;
        screen.form.weak_dialog_focus = 1;
        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Tab,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.form.weak_dialog_focus, 0);
    }

    #[test]
    fn weak_dialog_enter_cancel_returns_to_edit() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.show_weak_password_dialog = true;
        screen.form.weak_dialog_focus = 0;
        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert!(!screen.form.show_weak_password_dialog);
        assert_eq!(screen.form.weak_dialog_focus, 0);
    }

    #[test]
    fn footer_cancel_enter_navigates_to_main_without_changes() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.footer_focus = Some(FormFooterButton::Cancel);
        screen.form.has_changes = false;

        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );

        assert!(matches!(
            result,
            ScreenResult::NavigateTo(crate::commands::types::Screen::Main)
        ));
    }

    #[test]
    fn ctrl_shortcuts_work_on_edit_form() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.focus_field(1);
        for ch in "secret".chars() {
            screen.form.fields.password.as_mut().unwrap().push_char(ch);
        }

        let result = screen.update(Message::KeyEvent(ctrl('v')), &mut ctx);
        assert!(matches!(result, ScreenResult::Continue));
        assert!(screen.form.fields.password_visible);
        assert_eq!(screen.form.focused_field, 4);

        let result = screen.update(Message::KeyEvent(ctrl('c')), &mut ctx);
        assert!(matches!(result, ScreenResult::Command(_)));

        let result = screen.update(Message::KeyEvent(ctrl('g')), &mut ctx);
        assert!(matches!(result, ScreenResult::Continue));
        assert!(screen.generator.expanded);
    }

    #[test]
    fn tag_chips_can_be_selected_and_deleted_on_edit_form() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.focus_field(6);
        screen.form.fields.tags = vec!["work".into(), "personal".into()];

        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Right,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.form.fields.tag_focus, Some(0));

        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Delete,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.form.fields.tags, vec!["personal"]);
        assert_eq!(screen.form.fields.tag_focus, Some(0));
    }

    #[test]
    fn unsaved_dialog_shortcuts_match_rendered_hint_on_edit_form() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.show_unsaved_dialog = true;

        let cancel_result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Esc,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(cancel_result, ScreenResult::Continue));
        assert!(!screen.form.show_unsaved_dialog);

        screen.form.show_unsaved_dialog = true;
        let default_result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(default_result, ScreenResult::Continue));
        assert!(!screen.form.show_unsaved_dialog);

        screen.form.show_unsaved_dialog = true;
        let tab_result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Tab,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(tab_result, ScreenResult::Continue));
        assert_eq!(screen.form.unsaved_dialog_focus, 1);

        let exit_result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(
            exit_result,
            ScreenResult::NavigateTo(crate::commands::types::Screen::Main)
        ));
    }

    #[test]
    fn weak_dialog_enter_save_saves() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.show_weak_password_dialog = true;
        screen.form.weak_dialog_focus = 1;
        screen.record_id = Some(uuid::Uuid::new_v4());
        screen.form.fields.name = "Test".into();
        screen.form.fields.username = Some("user".into());
        for c in "weak".chars() {
            screen.form.fields.password.as_mut().unwrap().push_char(c);
        }
        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Command(_)));
        assert!(!screen.form.show_weak_password_dialog);
    }
}
