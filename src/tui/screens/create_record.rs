//! Create record screen (U7).

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use tui_textarea::CursorMove;
use unicode_width::UnicodeWidthStr;

use crate::commands::result::CommandResult;
use crate::commands::{Command, Message};
use crate::tui::components::textarea::TEXTAREA_TOTAL_ROWS;
use crate::tui::screens::form::validation;
use crate::tui::state::form_state::{
    ExpiryOption, FormFooterButton, FormState, PasswordFieldFocus,
};
use crate::tui::state::generator_state::{EmbeddedGeneratorState, GenerationStyle, GeneratorFocus};
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
use crate::types::credential::CredentialType;
use crate::types::sensitive::SensitiveInput;

/// Create record screen state.
pub struct CreateRecordScreen {
    pub form: FormState,
    pub generator: EmbeddedGeneratorState,
    pub all_tags: Vec<String>,
    last_area: Cell<Rect>,
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
            last_area: Cell::new(Rect::default()),
        }
    }

    /// Handle a key event.
    fn handle_key(
        &mut self,
        key_event: crossterm::event::KeyEvent,
        _ctx: &mut ScreenContext,
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
                ScreenResult::Continue
            }
            KeyCode::Down => {
                // If textarea is focused, move cursor down instead of next field
                if self.form.textarea_captures_vertical() {
                    self.form.fields.notes.move_cursor(CursorMove::Down);
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
                // If textarea is focused, move cursor up instead of prev field
                if self.form.textarea_captures_vertical() {
                    self.form.fields.notes.move_cursor(CursorMove::Up);
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
                    // Right at end of buttons → move to next field
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
                    // Left at start of buttons → move to prev field
                    self.form.focus_prev();
                    ScreenResult::Continue
                }
            }
            KeyCode::Esc => self.cancel_form(),
            KeyCode::Char('g') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.activate_generate_shortcut()
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
            KeyCode::Enter => self.handle_enter(),
            KeyCode::Char(' ')
                if self.is_dropdown_focused() || self.form.footer_focus.is_some() =>
            {
                self.handle_enter()
            }
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

    fn handle_enter(&mut self) -> ScreenResult {
        let focused = self.form.focused_field;
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
        if focused == notes_idx {
            self.form.fields.notes.insert_newline();
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
                if ct == CredentialType::Login && focused == 4 {
                    self.generator.expand();
                    return ScreenResult::Continue;
                }
            }
            crate::tui::state::form_state::PasswordFieldFocus::Paste => {
                // Paste is not wired to a command here -- future clipboard paste support
                return ScreenResult::Continue;
            }
            crate::tui::state::form_state::PasswordFieldFocus::Input => {}
        }

        // Check if focused on a dropdown field - toggle it
        if focused == 0 && self.form.is_credential_type_editable() {
            self.form.credential_dropdown.expanded = true;
            return ScreenResult::Continue;
        }
        let expiry_idx = match ct {
            CredentialType::Login | CredentialType::Api => 5,
            CredentialType::Ssh => 6,
            CredentialType::SecureNote => 3,
        };
        if focused == expiry_idx {
            self.form.expiry_dropdown.expanded = true;
            return ScreenResult::Continue;
        }

        // If on tag input, add tag
        let tags_idx = match ct {
            CredentialType::Login | CredentialType::Api => 6,
            CredentialType::Ssh => 7,
            CredentialType::SecureNote => 4,
        };
        if focused == tags_idx && !self.form.fields.tag_input.is_empty() {
            if self.form.fields.commit_tag_input() {
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
        if self.form.password_sub_focus != PasswordFieldFocus::Input {
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
                        // Field 2 for SecureNote is the notes textarea
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
                        // Field 3 for SecureNote is expiry - handled in default branch
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
                        // Field 4 for SecureNote is tags - handled in default branch
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
        ScreenResult::Continue
    }

    fn handle_backspace(&mut self) -> ScreenResult {
        if self.form.footer_focus.is_some() {
            return ScreenResult::Continue;
        }

        // If sub-focus is on a button, don't delete text
        if self.form.password_sub_focus != PasswordFieldFocus::Input {
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

    fn is_dropdown_focused(&self) -> bool {
        if self.form.footer_focus.is_some() {
            return false;
        }
        let expiry_idx = match self.form.credential_type {
            CredentialType::Login | CredentialType::Api => 5,
            CredentialType::Ssh => 6,
            CredentialType::SecureNote => 3,
        };
        (self.form.focused_field == 0 && self.form.is_credential_type_editable())
            || self.form.focused_field == expiry_idx
    }

    fn is_tags_focused(&self) -> bool {
        self.form.footer_focus.is_none() && self.form.focused_field == self.tags_field_index()
    }

    fn open_focused_dropdown(&mut self) -> bool {
        if self.form.footer_focus.is_some() {
            return false;
        }
        if self.form.focused_field == 0 && self.form.is_credential_type_editable() {
            self.form.credential_dropdown.expanded = true;
            return true;
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

    fn activate_generate_shortcut(&mut self) -> ScreenResult {
        if self.form.credential_type == CredentialType::Login {
            self.form.focus_field(4);
            self.generator.expand();
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

        self.create_record_command()
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
                    // "Go Back" — return to editing
                    ScreenResult::Continue
                } else {
                    // "Save Anyway"
                    self.create_record_command()
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

    fn handle_credential_dropdown(&mut self, key: KeyCode) -> ScreenResult {
        match key {
            KeyCode::Up => {
                if self.form.credential_dropdown.selected_index > 0 {
                    self.form.credential_dropdown.selected_index -= 1;
                }
                ScreenResult::Continue
            }
            KeyCode::Down => {
                if self.form.credential_dropdown.selected_index < 2 {
                    self.form.credential_dropdown.selected_index += 1;
                }
                ScreenResult::Continue
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char(' ') => {
                let ct = match self.form.credential_dropdown.selected_index {
                    0 => CredentialType::Login,
                    1 => CredentialType::Api,
                    _ => CredentialType::Ssh,
                };
                self.form.switch_credential_type(ct);
                self.form.credential_dropdown.expanded = false;
                ScreenResult::Continue
            }
            KeyCode::Esc | KeyCode::Left => {
                self.form.credential_dropdown.expanded = false;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
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

    fn create_record_command(&mut self) -> ScreenResult {
        ScreenResult::Command(Box::new(Command::CreateRecord {
            credential_type: self.form.credential_type,
            payload: self.form.build_payload(),
            tags: std::mem::take(&mut self.form.fields.tags),
            is_favorite: false,
            expires_at: self.form.expiry_datetime(),
        }))
    }
}

impl Screen for CreateRecordScreen {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult {
        match msg {
            Message::KeyEvent(key) => self.handle_key(key, ctx),
            Message::MouseEvent(event) => self.handle_mouse(event),
            Message::CommandCompleted(result) => match result {
                CommandResult::TagsLoaded { tags, tag_stats: _ } => {
                    self.all_tags = tags.into_iter().map(|tag| tag.name).collect();
                    ScreenResult::Continue
                }
                CommandResult::RecordCreated { .. } => ScreenResult::PopScreen,
                _ => ScreenResult::Continue,
            },
            _ => ScreenResult::Continue,
        }
    }

    fn view(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        self.last_area.set(area);
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
        self.form = FormState::new_create();
        self.all_tags.clear();
        ctx.send_system_command(Command::LoadTags);
    }

    fn on_unmount(&mut self) {
        self.form.clear_sensitive_fields();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormMouseTarget {
    Field(usize),
    CredentialDropdown,
    CredentialOption(usize),
    ExpiryDropdown,
    ExpiryOption(usize),
    PasswordButton(PasswordFieldFocus),
    Generator(GeneratorMouseTarget),
    TagChip(usize),
    Footer(FormFooterButton),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeneratorMouseTarget {
    LengthSlider,
    LengthMinus,
    LengthPlus,
    Toggle(usize),
    Regenerate,
    Action,
}

#[derive(Default)]
struct FormRowMap {
    credential: u16,
    credential_options: Option<(u16, u16)>,
    name: u16,
    url: u16,
    account: u16,
    secret: u16,
    passphrase: Option<u16>,
    expiry: u16,
    expiry_options: Option<(u16, u16)>,
    tags: u16,
    notes: u16,
    footer: u16,
}

impl CreateRecordScreen {
    fn handle_mouse(&mut self, event: MouseEvent) -> ScreenResult {
        let is_click = matches!(event.kind, MouseEventKind::Down(MouseButton::Left));
        let is_move = matches!(event.kind, MouseEventKind::Moved);
        if !is_click && !is_move {
            return ScreenResult::Continue;
        }

        let Some(target) = self.hit_test(event.column, event.row) else {
            return ScreenResult::Continue;
        };

        match target {
            FormMouseTarget::Field(index) => {
                self.form.focus_field(index);
                ScreenResult::Continue
            }
            FormMouseTarget::CredentialDropdown => {
                self.form.focus_field(0);
                if is_click {
                    self.form.credential_dropdown.expanded =
                        !self.form.credential_dropdown.expanded;
                }
                ScreenResult::Continue
            }
            FormMouseTarget::CredentialOption(index) => {
                self.form.focus_field(0);
                self.form.credential_dropdown.selected_index = index.min(2);
                if is_click {
                    return self.handle_credential_dropdown(KeyCode::Enter);
                }
                ScreenResult::Continue
            }
            FormMouseTarget::ExpiryDropdown => {
                self.form.focus_field(self.expiry_field_index());
                if is_click {
                    self.form.expiry_dropdown.expanded = !self.form.expiry_dropdown.expanded;
                }
                ScreenResult::Continue
            }
            FormMouseTarget::ExpiryOption(index) => {
                self.form.focus_field(self.expiry_field_index());
                self.form.expiry_dropdown.selected_index =
                    index.min(ExpiryOption::all_options().len().saturating_sub(1));
                if is_click {
                    return self.handle_expiry_dropdown(KeyCode::Enter);
                }
                ScreenResult::Continue
            }
            FormMouseTarget::PasswordButton(button) => {
                self.form.focus_field(self.secret_field_index());
                self.form.password_sub_focus = button;
                if is_click {
                    return self.handle_enter();
                }
                ScreenResult::Continue
            }
            FormMouseTarget::Generator(target) => {
                self.handle_generator_mouse_target(target, is_click)
            }
            FormMouseTarget::TagChip(index) => {
                self.form.focus_field(self.tags_field_index());
                self.form.fields.focus_tag(index);
                ScreenResult::Continue
            }
            FormMouseTarget::Footer(button) => {
                self.form.footer_focus = Some(button);
                self.form.password_sub_focus = PasswordFieldFocus::Input;
                if is_click {
                    return self.activate_footer_button(button);
                }
                ScreenResult::Continue
            }
        }
    }

    fn hit_test(&self, column: u16, row: u16) -> Option<FormMouseTarget> {
        let area = self.last_area.get();
        if area.width == 0 || area.height == 0 || !contains(area, column, row) {
            return None;
        }

        if self.generator.expanded {
            if let Some(target) = self.hit_test_generator_dialog(column, row) {
                return Some(FormMouseTarget::Generator(target));
            }
            return None;
        }

        let content_row = row.checked_sub(area.y + 1)?;
        let content_col = column.checked_sub(area.x + 1)?;
        let rows = self.form_row_map();

        if content_row == rows.credential {
            return Some(FormMouseTarget::CredentialDropdown);
        }
        if let Some((start, len)) = rows.credential_options {
            if content_row >= start && content_row < start + len {
                return Some(FormMouseTarget::CredentialOption(
                    (content_row - start) as usize,
                ));
            }
        }
        if content_row == rows.name {
            return Some(FormMouseTarget::Field(1));
        }
        if content_row == rows.url {
            return Some(FormMouseTarget::Field(2));
        }
        if content_row == rows.account {
            return Some(FormMouseTarget::Field(3));
        }
        if content_row == rows.secret {
            if let Some(button) = self.password_button_at(content_col, area.width) {
                return Some(FormMouseTarget::PasswordButton(button));
            }
            return Some(FormMouseTarget::Field(self.secret_field_index()));
        }
        if let Some(passphrase_row) = rows.passphrase {
            if content_row == passphrase_row {
                return Some(FormMouseTarget::Field(5));
            }
        }
        if content_row == rows.expiry {
            return Some(FormMouseTarget::ExpiryDropdown);
        }
        if let Some((start, len)) = rows.expiry_options {
            if content_row >= start && content_row < start + len {
                return Some(FormMouseTarget::ExpiryOption(
                    (content_row - start) as usize,
                ));
            }
        }
        if content_row == rows.tags {
            return Some(FormMouseTarget::Field(self.tags_field_index()));
        }
        if !self.form.fields.tags.is_empty() && content_row == rows.tags + 1 {
            if let Some(index) = self.tag_chip_at(content_col) {
                return Some(FormMouseTarget::TagChip(index));
            }
        }
        if content_row == rows.notes {
            return Some(FormMouseTarget::Field(self.notes_field_index()));
        }
        if content_row == rows.footer {
            if let Some(button) = footer_button_at(content_col) {
                return Some(FormMouseTarget::Footer(button));
            }
        }

        None
    }

    fn form_row_map(&self) -> FormRowMap {
        let mut row = 3;
        let credential = row;
        let credential_options = if self.form.credential_dropdown.expanded {
            Some((row + 1, 3))
        } else {
            None
        };
        row += 1 + credential_options.map_or(0, |(_, len)| len) + 1;

        let name = row;
        row += 1 + self.error_rows_for(1) + 1;
        let url = row;
        row += 1 + 1;
        let account = row;
        row += 1 + self.error_rows_for(3) + 1;
        let secret = row;
        row += 1 + self.error_rows_for(4);

        let mut passphrase = None;
        match self.form.credential_type {
            CredentialType::Login => {
                row += 1; // padding before strength
                row += 1; // strength row
            }
            CredentialType::Api => {}
            CredentialType::Ssh => {
                row += 2; // optional marker + blank after private key
                passphrase = Some(row);
                row += 2; // passphrase row + optional marker
            }
            CredentialType::SecureNote => {
                // SecureNote has no account/secret/passphrase fields
                // After name (field 1), notes is at field 2
            }
        }
        row += 1;

        let expiry = row;
        let expiry_options = if self.form.expiry_dropdown.expanded {
            Some((row + 1, ExpiryOption::all_options().len() as u16))
        } else {
            None
        };
        row += 1 + expiry_options.map_or(0, |(_, len)| len);
        if self.form.fields.expires_at == ExpiryOption::Custom {
            row += 2;
        }
        row += 1;

        let tags = row;
        row += 1;
        if !self.form.fields.tags.is_empty() {
            row += 1;
        }
        if let Some(ac) = &self.form.tag_autocomplete {
            row += ac.matches.len() as u16;
        }
        row += 1;

        let notes = row;
        row += 1 + TEXTAREA_TOTAL_ROWS as u16;

        let inner_height = self.last_area.get().height.saturating_sub(2);
        if row.saturating_add(3) < inner_height {
            row = inner_height.saturating_sub(3);
        }
        row += 1; // separator
        let footer = row;

        FormRowMap {
            credential,
            credential_options,
            name,
            url,
            account,
            secret,
            passphrase,
            expiry,
            expiry_options,
            tags,
            notes,
            footer,
        }
    }

    fn error_rows_for(&self, field_index: usize) -> u16 {
        u16::from(self.form.validation_errors.iter().any(|error| {
            error.field_index == field_index
                && error.message != crate::t!("tui.form.validation_required").as_ref()
        }))
    }

    fn expiry_field_index(&self) -> usize {
        match self.form.credential_type {
            CredentialType::Login | CredentialType::Api => 5,
            CredentialType::Ssh => 6,
            CredentialType::SecureNote => 3,
        }
    }

    fn tags_field_index(&self) -> usize {
        match self.form.credential_type {
            CredentialType::Login | CredentialType::Api => 6,
            CredentialType::Ssh => 7,
            CredentialType::SecureNote => 4,
        }
    }

    fn notes_field_index(&self) -> usize {
        match self.form.credential_type {
            CredentialType::Login | CredentialType::Api => 7,
            CredentialType::Ssh => 8,
            CredentialType::SecureNote => 2,
        }
    }

    fn secret_field_index(&self) -> usize {
        match self.form.credential_type {
            CredentialType::Login | CredentialType::Api | CredentialType::Ssh => 4,
            CredentialType::SecureNote => {
                // SecureNote has no secret field
                0
            }
        }
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

    fn password_button_at(&self, content_col: u16, width: u16) -> Option<PasswordFieldFocus> {
        let buttons = self.secret_row_buttons()?;
        let labels: Vec<String> = buttons
            .iter()
            .map(|button| match button {
                PasswordFieldFocus::Generate => crate::t!("tui.form.generate_button").to_string(),
                PasswordFieldFocus::Show => crate::t!("tui.form.show_button").to_string(),
                PasswordFieldFocus::Copy => crate::t!("tui.form.copy_button").to_string(),
                PasswordFieldFocus::Paste => crate::t!("tui.form.paste_button").to_string(),
                PasswordFieldFocus::Input => String::new(),
            })
            .collect();
        let button_width = labels
            .iter()
            .map(|label| UnicodeWidthStr::width(format!(" [ {label} ]").as_str()))
            .sum::<usize>()
            + labels.len();
        let content_width = width.saturating_sub(2) as usize;
        let input_width = content_width
            .saturating_sub(13)
            .saturating_sub(button_width)
            .saturating_sub(2)
            .max(1);
        let mut start = 13 + input_width + 2;
        let col = content_col as usize;
        for (button, label) in buttons.into_iter().zip(labels) {
            start += 1;
            let text_width = UnicodeWidthStr::width(format!("[ {label} ]").as_str());
            if col >= start && col < start + text_width {
                return Some(button);
            }
            start += text_width;
        }
        None
    }

    fn secret_row_buttons(&self) -> Option<Vec<PasswordFieldFocus>> {
        match self.form.credential_type {
            CredentialType::Login => Some(vec![
                PasswordFieldFocus::Generate,
                PasswordFieldFocus::Show,
                PasswordFieldFocus::Copy,
            ]),
            CredentialType::Api => Some(vec![PasswordFieldFocus::Show, PasswordFieldFocus::Copy]),
            CredentialType::Ssh => Some(vec![
                PasswordFieldFocus::Show,
                PasswordFieldFocus::Paste,
                PasswordFieldFocus::Copy,
            ]),
            CredentialType::SecureNote => None, // No secret field
        }
    }

    fn tag_chip_at(&self, content_col: u16) -> Option<usize> {
        let mut start = crate::tui::components::text_input::FORM_LABEL_WIDTH;
        let col = content_col as usize;
        for (index, tag) in self.form.fields.tags.iter().enumerate() {
            let width = UnicodeWidthStr::width(format!("[ {tag} ×] ").as_str());
            if col >= start && col < start + width {
                return Some(index);
            }
            start += width;
        }
        None
    }

    fn handle_generator_mouse_target(
        &mut self,
        target: GeneratorMouseTarget,
        is_click: bool,
    ) -> ScreenResult {
        self.form.password_sub_focus = PasswordFieldFocus::Input;
        self.form.footer_focus = None;
        match target {
            GeneratorMouseTarget::LengthSlider => {
                self.generator.generator.focus = GeneratorFocus::LengthSlider;
            }
            GeneratorMouseTarget::LengthMinus => {
                self.generator.generator.focus = GeneratorFocus::LengthSlider;
                if is_click {
                    self.generator.generator.decrement_length();
                }
            }
            GeneratorMouseTarget::LengthPlus => {
                self.generator.generator.focus = GeneratorFocus::LengthSlider;
                if is_click {
                    self.generator.generator.increment_length();
                }
            }
            GeneratorMouseTarget::Toggle(idx) => {
                self.generator.generator.focus = GeneratorFocus::Toggle(idx);
                if is_click {
                    self.toggle_generator_option(idx);
                }
            }
            GeneratorMouseTarget::Regenerate => {
                self.generator.generator.focus = GeneratorFocus::RegenerateButton;
                if is_click {
                    self.generator.generator.regenerate();
                }
            }
            GeneratorMouseTarget::Action => {
                self.generator.generator.focus = GeneratorFocus::ActionButton;
                if is_click {
                    return self.activate_generator_focus();
                }
            }
        }
        ScreenResult::Continue
    }

    fn generator_mouse_target(
        &self,
        content_col: u16,
        panel_row: u16,
        panel_len: u16,
    ) -> Option<GeneratorMouseTarget> {
        let col = content_col as usize;
        match panel_row {
            0 => self.generator_length_target(col),
            2 => self.generator_options_target(col),
            row if row + 1 == panel_len => self.generator_button_target(col),
            _ => None,
        }
    }

    fn hit_test_generator_dialog(&self, column: u16, row: u16) -> Option<GeneratorMouseTarget> {
        let area = self.last_area.get();
        let dialog = crate::tui::screens::form::render::generator_dialog_area(
            area,
            &self.generator.generator,
            true,
        );
        if !contains(dialog, column, row) {
            return None;
        }
        let content_col = column.checked_sub(dialog.x + 1)?;
        let content_row = row.checked_sub(dialog.y + 1)?;
        let panel_start = 3;
        if content_row < panel_start {
            return None;
        }
        let panel_len = crate::tui::components::generator_panel::render_generator_panel(
            &self.generator.generator,
            true,
            dialog.width.saturating_sub(2),
            true,
        )
        .len() as u16;
        let panel_row = content_row - panel_start;
        if panel_row >= panel_len {
            return None;
        }
        self.generator_mouse_target(content_col, panel_row, panel_len)
    }

    fn generator_length_target(&self, col: usize) -> Option<GeneratorMouseTarget> {
        let label = match self.generator.generator.style {
            GenerationStyle::Random | GenerationStyle::Pin => {
                crate::t!("tui.generator.length").to_string()
            }
            GenerationStyle::Memorable => crate::t!("tui.generator.word_count").to_string(),
        };
        let value = self.generator.generator.current_length();
        let label_width = UnicodeWidthStr::width(format!("  {label} ").as_str());
        let value_width = UnicodeWidthStr::width(format!("[ {value} ]").as_str());
        let minus_start = label_width + value_width + 2;
        let minus_end = minus_start + 3;
        let bar_start = minus_end + 1;
        let plus_start = bar_start + crate::tui::components::length_slider::SLIDER_BAR_WIDTH + 1;
        let plus_end = plus_start + 3;

        if col >= minus_start && col < minus_end {
            Some(GeneratorMouseTarget::LengthMinus)
        } else if col >= plus_start && col < plus_end {
            Some(GeneratorMouseTarget::LengthPlus)
        } else if col >= label_width && col < plus_end {
            Some(GeneratorMouseTarget::LengthSlider)
        } else {
            None
        }
    }

    fn generator_options_target(&self, col: usize) -> Option<GeneratorMouseTarget> {
        match self.generator.generator.style {
            GenerationStyle::Random => {
                let labels = [
                    crate::t!("tui.generator.uppercase").to_string(),
                    crate::t!("tui.generator.lowercase").to_string(),
                    crate::t!("tui.generator.digits").to_string(),
                    crate::t!("tui.generator.symbols").to_string(),
                ];
                let mut start = 2usize;
                for (idx, label) in labels.iter().enumerate() {
                    let enabled = self.generator.generator.is_toggle_enabled(idx);
                    let check = if enabled { "✓" } else { " " };
                    let width = UnicodeWidthStr::width(format!("[{check}] {label}  ").as_str());
                    if self.generator.generator.is_toggle_interactive(idx)
                        && col >= start
                        && col < start + width
                    {
                        return Some(GeneratorMouseTarget::Toggle(idx));
                    }
                    start += width;
                }
                None
            }
            GenerationStyle::Memorable => {
                let label = crate::t!("tui.generator.capitalize").to_string();
                let width = UnicodeWidthStr::width(format!("[✓] {label}").as_str());
                if col >= 2 && col < 2 + width {
                    Some(GeneratorMouseTarget::Toggle(0))
                } else {
                    None
                }
            }
            GenerationStyle::Pin => None,
        }
    }

    fn generator_button_target(&self, col: usize) -> Option<GeneratorMouseTarget> {
        let regen = crate::t!("tui.generator.regenerate").to_string();
        let action = crate::t!("tui.generator.use_password").to_string();
        let regen_start = 5usize;
        let regen_width = UnicodeWidthStr::width(format!(" [ {regen} ] ").as_str());
        let action_start = regen_start + regen_width + 8;
        let action_width = UnicodeWidthStr::width(format!(" [ {action} ] ").as_str());

        if col >= regen_start && col < regen_start + regen_width {
            Some(GeneratorMouseTarget::Regenerate)
        } else if col >= action_start && col < action_start + action_width {
            Some(GeneratorMouseTarget::Action)
        } else {
            None
        }
    }
}

fn footer_button_at(content_col: u16) -> Option<FormFooterButton> {
    let save = crate::t!("tui.form.save_button").to_string();
    let cancel = crate::t!("tui.form.cancel_button").to_string();
    let save_start = 2usize;
    let save_width = UnicodeWidthStr::width(format!("[ {save} ]").as_str());
    let cancel_start = save_start + save_width + 2;
    let cancel_width = UnicodeWidthStr::width(format!("[ {cancel} ]").as_str());
    let col = content_col as usize;
    if col >= save_start && col < save_start + save_width {
        Some(FormFooterButton::Save)
    } else if col >= cancel_start && col < cancel_start + cancel_width {
        Some(FormFooterButton::Cancel)
    } else {
        None
    }
}

fn contains(area: Rect, col: u16, row: u16) -> bool {
    col >= area.x
        && col < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::result::CommandResult;
    use crate::tui::state::form_state::PasswordFieldFocus;
    use crate::tui::state::generator_state::GeneratorFocus;
    use crate::types::tag::Tag;
    use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;
    use std::collections::HashMap;
    use tokio::sync::mpsc;

    fn make_screen() -> CreateRecordScreen {
        CreateRecordScreen::new()
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

    fn render_buffer(screen: &CreateRecordScreen, width: u16, height: u16) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                screen.view(frame, frame.area());
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn find_text(buffer: &Buffer, needle: &str) -> Option<(u16, u16)> {
        let needle_chars: Vec<char> = needle.chars().collect();
        for y in buffer.area.y..buffer.area.y + buffer.area.height {
            let row: Vec<String> = (buffer.area.x..buffer.area.x + buffer.area.width)
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .map(ToOwned::to_owned)
                .collect();
            for start in 0..row.len() {
                if needle_chars.iter().enumerate().all(|(offset, ch)| {
                    row.get(start + offset)
                        .is_some_and(|cell| cell == &ch.to_string())
                }) {
                    return Some((buffer.area.x + start as u16, y));
                }
            }
        }
        None
    }

    fn contains_required_marker(row: &str) -> bool {
        row.contains("Required") || (row.contains('←') && row.contains('必') && row.contains('填'))
    }

    fn first_symbol_in_row(buffer: &Buffer, row: u16, symbol: &str) -> Option<u16> {
        (buffer.area.x..buffer.area.x + buffer.area.width).find(|x| {
            buffer
                .cell((*x, row))
                .is_some_and(|cell| cell.symbol() == symbol)
        })
    }

    fn click(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn mouse_move(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Moved,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn key(code: KeyCode) -> crossterm::event::KeyEvent {
        crossterm::event::KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(ch: char) -> crossterm::event::KeyEvent {
        crossterm::event::KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
    }

    #[test]
    fn on_mount_sends_load_tags() {
        let (tx, mut rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.on_mount(&mut ctx);
        let cmd = rx.try_recv().unwrap();
        assert!(matches!(cmd, Command::LoadTags));
    }

    #[test]
    fn update_tags_loaded_populates_all_tags() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        let tags = vec![
            Tag {
                id: 1,
                name: "work".into(),
            },
            Tag {
                id: 2,
                name: "personal".into(),
            },
        ];
        let result = screen.update(
            Message::CommandCompleted(CommandResult::TagsLoaded {
                tags,
                tag_stats: HashMap::new(),
            }),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.all_tags.len(), 2);
    }

    #[test]
    fn update_record_created_pops_screen() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        let result = screen.update(
            Message::CommandCompleted(CommandResult::RecordCreated {
                id: uuid::Uuid::new_v4(),
            }),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::PopScreen));
    }

    #[test]
    fn esc_without_changes_navigates_to_main() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.has_changes = false;
        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Esc,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(
            result,
            ScreenResult::NavigateTo(crate::commands::types::Screen::Main)
        ));
    }

    #[test]
    fn esc_with_changes_shows_unsaved_dialog() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.has_changes = true;
        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Esc,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert!(screen.form.show_unsaved_dialog);
    }

    #[test]
    fn unsaved_dialog_esc_cancels_and_keeps_editing() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.show_unsaved_dialog = true;

        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Esc,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );

        assert!(matches!(result, ScreenResult::Continue));
        assert!(!screen.form.show_unsaved_dialog);
    }

    #[test]
    fn unsaved_dialog_enter_defaults_to_continue_editing() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.show_unsaved_dialog = true;

        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );

        assert!(matches!(result, ScreenResult::Continue));
        assert!(!screen.form.show_unsaved_dialog);
    }

    #[test]
    fn unsaved_dialog_tab_then_enter_discards_and_exits() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.show_unsaved_dialog = true;

        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Tab,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.form.unsaved_dialog_focus, 1);

        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(
            result,
            ScreenResult::NavigateTo(crate::commands::types::Screen::Main)
        ));
    }

    #[test]
    fn right_arrow_on_type_field_opens_dropdown() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);

        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Right,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );

        assert!(matches!(result, ScreenResult::Continue));
        assert!(screen.form.credential_dropdown.expanded);
        assert_eq!(screen.form.focused_field, 0);
    }

    #[test]
    fn mouse_click_show_password_button_toggles_visibility() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        for c in "secret".chars() {
            screen.form.fields.password.as_mut().unwrap().push_char(c);
        }

        let buffer = render_buffer(&screen, 100, 32);
        let (x, y) = find_text(&buffer, "Show").expect("show button should be rendered");
        let result = screen.update(Message::MouseEvent(click(x, y)), &mut ctx);

        assert!(matches!(result, ScreenResult::Continue));
        assert!(screen.form.fields.password_visible);
        assert_eq!(screen.form.focused_field, 4);
        assert_eq!(screen.form.password_sub_focus, PasswordFieldFocus::Show);
    }

    #[test]
    fn mouse_click_type_dropdown_opens_options() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);

        let buffer = render_buffer(&screen, 100, 32);
        let (x, y) = find_text(&buffer, "Login").expect("type dropdown should be rendered");
        let result = screen.update(Message::MouseEvent(click(x, y)), &mut ctx);

        assert!(matches!(result, ScreenResult::Continue));
        assert!(screen.form.credential_dropdown.expanded);
        assert_eq!(screen.form.focused_field, 0);
    }

    #[test]
    fn mouse_hover_save_button_sets_footer_focus() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);

        let buffer = render_buffer(&screen, 100, 32);
        let (x, y) = find_text(&buffer, "Save").expect("save button should be rendered");
        let result = screen.update(Message::MouseEvent(mouse_move(x, y)), &mut ctx);

        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(
            screen.form.footer_focus,
            Some(crate::tui::state::form_state::FormFooterButton::Save)
        );
    }

    #[test]
    fn empty_required_fields_stay_on_one_row() {
        let screen = make_screen();
        let buffer = render_buffer(&screen, 80, 24);
        let (_, name_row) = find_text(&buffer, "Name").expect("name field should render");
        let name_line = (0..80)
            .filter_map(|x| buffer.cell((x, name_row)).map(|cell| cell.symbol()))
            .collect::<String>();
        let next_line = (0..80)
            .filter_map(|x| buffer.cell((x, name_row + 1)).map(|cell| cell.symbol()))
            .collect::<String>();

        assert!(contains_required_marker(&name_line), "{name_line:?}");
        assert!(
            !next_line.trim_start().starts_with(']'),
            "input closing bracket wrapped to next row: {next_line:?}"
        );
    }

    #[test]
    fn required_validation_does_not_render_duplicate_error_rows() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);

        let result = screen.update(Message::KeyEvent(ctrl('s')), &mut ctx);
        assert!(matches!(result, ScreenResult::Continue));

        let buffer = render_buffer(&screen, 100, 32);
        for y in buffer.area.y..buffer.area.y + buffer.area.height {
            let row = (buffer.area.x..buffer.area.x + buffer.area.width)
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>();
            let duplicate_required_row = row.contains("│  ← Required")
                || (row.contains("│  ←") && row.contains('必') && row.contains('填'));
            assert!(
                !duplicate_required_row,
                "required validation should not render a duplicate error row: {row:?}"
            );
        }
    }

    #[test]
    fn focused_dropdown_has_visible_highlight_style() {
        let screen = make_screen();
        let buffer = render_buffer(&screen, 80, 24);
        let (x, y) = find_text(&buffer, "Login").expect("focused type value should render");
        let cell = buffer.cell((x, y)).expect("cell should exist");

        assert_eq!(cell.bg, crate::tui::theme::PRIMARY);
    }

    #[test]
    fn form_auxiliary_controls_align_to_text_input_column() {
        let mut screen = make_screen();
        for c in "weak".chars() {
            screen.form.fields.password.as_mut().unwrap().push_char(c);
        }
        screen.form.fields.update_strength();

        let buffer = render_buffer(&screen, 100, 32);
        let (_, name_row) = find_text(&buffer, "Name").expect("name should render");
        let (_, type_row) = find_text(&buffer, "Type").expect("type should render");
        let (_, expiry_row) = find_text(&buffer, "Expiry").expect("expiry should render");
        let (_, tags_row) = find_text(&buffer, "Tags").expect("tags should render");
        let (_, strength_row) = find_text(&buffer, "Strength").expect("strength should render");

        let input_col = first_symbol_in_row(&buffer, name_row, "[").expect("name input bracket");
        assert_eq!(first_symbol_in_row(&buffer, type_row, "["), Some(input_col));
        assert_eq!(
            first_symbol_in_row(&buffer, expiry_row, "["),
            Some(input_col)
        );
        assert_eq!(first_symbol_in_row(&buffer, tags_row, "["), Some(input_col));

        let strength_bar_col = (0..100)
            .find(|x| {
                buffer
                    .cell((*x, strength_row))
                    .is_some_and(|cell| cell.symbol() == crate::tui::theme::ICON_PROGRESS_FILL)
            })
            .expect("strength bar should render");
        assert_eq!(strength_bar_col, input_col);
    }

    #[test]
    fn focused_empty_text_input_renders_single_block_cursor() {
        let mut screen = make_screen();
        screen.form.focus_field(1);

        let buffer = render_buffer(&screen, 100, 32);
        let (_, name_row) = find_text(&buffer, "Name").expect("name should render");
        let input_col = first_symbol_in_row(&buffer, name_row, "[").expect("name input bracket");
        let cursor_cell = buffer
            .cell((input_col + 1, name_row))
            .expect("cursor cell should exist");
        let next_cell = buffer
            .cell((input_col + 2, name_row))
            .expect("next input cell should exist");

        assert_eq!(cursor_cell.bg, crate::tui::theme::PRIMARY);
        assert_ne!(next_cell.bg, crate::tui::theme::PRIMARY);
    }

    #[test]
    fn expanded_generator_renders_as_dialog_over_form() {
        let mut screen = make_screen();
        let collapsed_expiry_row = screen.form_row_map().expiry;
        screen.generator.expand();
        let expanded_expiry_row = screen.form_row_map().expiry;

        let buffer = render_buffer(&screen, 100, 32);
        let (_, title_row) = find_text(&buffer, "Password Generator")
            .or_else(|| find_text(&buffer, "密码生成器"))
            .expect("generator dialog title should render");
        let (_, use_row) = find_text(&buffer, "Use Password")
            .or_else(|| find_text(&buffer, "使用此密码"))
            .expect("use password action should render");

        assert_eq!(
            expanded_expiry_row, collapsed_expiry_row,
            "generator dialog should not insert rows into the form layout"
        );
        assert!(title_row > 4, "dialog should be centered over the form");
        assert!(use_row > title_row);
    }

    #[test]
    fn tag_input_commits_trimmed_tags_with_comma_separators() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.focus_field(6);

        for ch in " work,personal，work,".chars() {
            let result = screen.update(Message::KeyEvent(key(KeyCode::Char(ch))), &mut ctx);
            assert!(matches!(result, ScreenResult::Continue));
        }

        assert_eq!(screen.form.fields.tags, vec!["work", "personal"]);
        assert!(screen.form.fields.tag_input.is_empty());
    }

    #[test]
    fn tag_input_shows_enter_add_and_delete_hint() {
        let mut screen = make_screen();
        screen.form.focus_field(6);

        let buffer = render_buffer(&screen, 120, 32);

        assert!(find_text(&buffer, "Enter Add")
            .or_else(|| find_text(&buffer, "Enter 添加"))
            .is_some());
        assert!(find_text(&buffer, "Del Delete")
            .or_else(|| find_text(&buffer, "Del 删除"))
            .is_some());
    }

    #[test]
    fn tag_chips_can_be_selected_and_deleted_with_keyboard() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.focus_field(6);
        screen.form.fields.tags = vec!["work".into(), "personal".into()];

        let result = screen.update(Message::KeyEvent(key(KeyCode::Right)), &mut ctx);
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.form.fields.tag_focus, Some(0));

        let result = screen.update(Message::KeyEvent(key(KeyCode::Right)), &mut ctx);
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.form.fields.tag_focus, Some(1));

        let result = screen.update(Message::KeyEvent(key(KeyCode::Backspace)), &mut ctx);
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.form.fields.tags, vec!["work"]);
        assert_eq!(screen.form.fields.tag_focus, Some(0));
    }

    #[test]
    fn mouse_click_tag_chip_selects_it_for_deletion() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.fields.tags = vec!["work".into(), "personal".into()];

        let buffer = render_buffer(&screen, 120, 32);
        let (x, y) = find_text(&buffer, "personal").expect("tag chip should render");
        let result = screen.update(Message::MouseEvent(click(x, y)), &mut ctx);

        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.form.focused_field, 6);
        assert_eq!(screen.form.fields.tag_focus, Some(1));
    }

    #[test]
    fn form_shortcuts_render_at_bottom() {
        let screen = make_screen();
        let buffer = render_buffer(&screen, 100, 32);

        assert!(find_text(&buffer, "Ctrl+G").is_some());
        assert!(find_text(&buffer, "Ctrl+V").is_some());
        assert!(find_text(&buffer, "Ctrl+C").is_some());
        assert!(find_text(&buffer, "Ctrl+S").is_some());
    }

    #[test]
    fn ctrl_g_opens_password_generator_from_any_field() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.focus_field(1);

        let result = screen.update(Message::KeyEvent(ctrl('g')), &mut ctx);

        assert!(matches!(result, ScreenResult::Continue));
        assert!(screen.generator.expanded);
        assert_eq!(screen.form.focused_field, 4);
    }

    #[test]
    fn ctrl_v_toggles_password_visibility_from_any_field() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.focus_field(1);

        let result = screen.update(Message::KeyEvent(ctrl('v')), &mut ctx);

        assert!(matches!(result, ScreenResult::Continue));
        assert!(screen.form.fields.password_visible);
        assert_eq!(screen.form.focused_field, 4);
    }

    #[test]
    fn ctrl_c_copies_password_from_any_field() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.focus_field(1);
        for ch in "secret".chars() {
            screen.form.fields.password.as_mut().unwrap().push_char(ch);
        }

        let result = screen.update(Message::KeyEvent(ctrl('c')), &mut ctx);

        assert!(matches!(result, ScreenResult::Command(_)));
        assert_eq!(screen.form.focused_field, 4);
    }

    #[test]
    fn unicode_input_values_do_not_expand_form_rows() {
        let mut screen = make_screen();
        screen.form.fields.name = "求求了的".into();
        screen.form.fields.url = "例子.example".into();
        screen.form.fields.username = Some("用户甲".into());
        screen.form.fields.tag_input = "等dddddd".into();
        screen.form.fields.set_notes_text("备注中文");

        let rows = screen.form_row_map();
        let buffer = render_buffer(&screen, 80, 32);
        for (text, row) in [
            ("name", rows.name + 1),
            ("url", rows.url + 1),
            ("account", rows.account + 1),
            ("tags", rows.tags + 1),
            ("notes", rows.notes + 1),
        ] {
            let next_line = (1..79)
                .filter_map(|x| buffer.cell((x, row + 1)).map(|cell| cell.symbol()))
                .collect::<String>();
            assert!(
                next_line.trim().is_empty(),
                "{text:?} input wrapped into next row: {next_line:?}"
            );
        }
    }

    #[test]
    fn embedded_generator_arrow_keys_adjust_parameters() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.generator.expand();
        screen.generator.generator.random_config.length = 16;
        screen.generator.generator.focus = GeneratorFocus::LengthSlider;

        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Right,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );

        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.generator.generator.random_config.length, 17);

        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Down,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );

        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.generator.generator.focus, GeneratorFocus::Toggle(0));
    }

    #[test]
    fn mouse_click_embedded_generator_plus_increments_length() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.generator.expand();
        screen.generator.generator.random_config.length = 16;
        screen.generator.generator.focus = GeneratorFocus::LengthSlider;

        let buffer = render_buffer(&screen, 100, 32);
        let (x, y) = find_text(&buffer, "[+]").expect("generator plus button should render");
        let result = screen.update(Message::MouseEvent(click(x, y)), &mut ctx);

        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.generator.generator.random_config.length, 17);
        assert_eq!(
            screen.generator.generator.focus,
            GeneratorFocus::LengthSlider
        );
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
                KeyCode::Esc,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert!(!screen.form.show_weak_password_dialog);
        assert_eq!(screen.form.weak_dialog_focus, 0);
    }

    #[test]
    fn weak_dialog_left_focuses_go_back() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.show_weak_password_dialog = true;
        screen.form.weak_dialog_focus = 1;
        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Left,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.form.weak_dialog_focus, 0);
        assert!(screen.form.show_weak_password_dialog);
    }

    #[test]
    fn weak_dialog_right_focuses_save_anyway() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.show_weak_password_dialog = true;
        screen.form.weak_dialog_focus = 0;
        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Right,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.form.weak_dialog_focus, 1);
        assert!(screen.form.show_weak_password_dialog);
    }

    #[test]
    fn weak_dialog_tab_focuses_go_back() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.show_weak_password_dialog = true;
        screen.form.weak_dialog_focus = 1;
        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Tab,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.form.weak_dialog_focus, 0);
    }

    #[test]
    fn weak_dialog_enter_go_back_returns_to_edit() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.show_weak_password_dialog = true;
        screen.form.weak_dialog_focus = 0;
        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Continue));
        assert!(!screen.form.show_weak_password_dialog);
        assert_eq!(screen.form.weak_dialog_focus, 0);
    }

    #[test]
    fn weak_dialog_enter_save_anyway_saves() {
        let (tx, _rx) = mpsc::channel(1);
        let mut screen = make_screen();
        let env = TestEnv::new();
        let mut ctx = env.make_ctx(&tx);
        screen.form.show_weak_password_dialog = true;
        screen.form.weak_dialog_focus = 1;
        screen.form.fields.name = "Test".into();
        screen.form.fields.username = Some("user".into());
        for c in "weak".chars() {
            screen.form.fields.password.as_mut().unwrap().push_char(c);
        }
        let result = screen.update(
            Message::KeyEvent(crossterm::event::KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            )),
            &mut ctx,
        );
        assert!(matches!(result, ScreenResult::Command(_)));
        assert!(!screen.form.show_weak_password_dialog);
    }
}
