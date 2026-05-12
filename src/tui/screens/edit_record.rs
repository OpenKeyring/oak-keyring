//! Edit record screen (U7).

use crossterm::event::KeyCode;
use uuid::Uuid;

use crate::commands::result::CommandResult;
use crate::commands::{Command, Message};
use crate::tui::screens::form::validation;
use crate::tui::state::form_state::{ExpiryOption, FormState};
use crate::tui::state::generator_state::EmbeddedGeneratorState;
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
use crate::types::credential::CredentialType;
use crate::types::sensitive::SensitiveInput;

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

        // If dropdown is expanded, handle dropdown keys (no credential type dropdown in Edit)
        if self.form.expiry_dropdown.expanded {
            return self.handle_expiry_dropdown(key);
        }

        // Normal form navigation (credential type dropdown is disabled)
        match key {
            KeyCode::Tab | KeyCode::Down => {
                self.form.focus_next();
                ScreenResult::Continue
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.form.focus_prev();
                ScreenResult::Continue
            }
            KeyCode::Right => {
                if self.form.sub_focus_next() {
                    ScreenResult::Continue
                } else {
                    self.form.focus_next();
                    ScreenResult::Continue
                }
            }
            KeyCode::Left => {
                if self.form.sub_focus_prev() {
                    ScreenResult::Continue
                } else {
                    self.form.focus_prev();
                    ScreenResult::Continue
                }
            }
            KeyCode::Esc => {
                if self.form.has_changes {
                    self.form.show_unsaved_dialog = true;
                    ScreenResult::Continue
                } else {
                    ScreenResult::NavigateTo(crate::commands::types::Screen::Main)
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
            _ => ScreenResult::Continue,
        }
    }

    fn handle_enter(&mut self) -> ScreenResult {
        let ct = self.form.credential_type;

        // Check inline button actions first
        match self.form.password_sub_focus {
            crate::tui::state::form_state::PasswordFieldFocus::Show => {
                self.form.toggle_current_visibility();
                return ScreenResult::Continue;
            }
            crate::tui::state::form_state::PasswordFieldFocus::Copy => {
                if let Some(value) = self.form.current_secret_value() {
                    if !value.is_empty() {
                        use crate::types::sensitive::SecureStr;
                        let val = value.to_string();
                        return ScreenResult::Command(Box::new(Command::CopyRawToClipboard {
                            value: SecureStr::new(val),
                        }));
                    }
                }
                return ScreenResult::Continue;
            }
            crate::tui::state::form_state::PasswordFieldFocus::Generate => {
                // Only Login password field has Generate button
                if ct == CredentialType::Login && self.form.focused_field == 4 {
                    self.generator.expand();
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
        };
        if self.form.focused_field == expiry_idx {
            self.form.expiry_dropdown.expanded = true;
            return ScreenResult::Continue;
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
            return ScreenResult::Continue;
        }

        ScreenResult::Continue
    }

    fn handle_char_input(&mut self, c: char) -> ScreenResult {
        // If sub-focus is on a button, don't accept text input
        if self.form.password_sub_focus != crate::tui::state::form_state::PasswordFieldFocus::Input
        {
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
        ScreenResult::Continue
    }

    fn handle_backspace(&mut self) -> ScreenResult {
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
                    self.form.fields.password.as_mut().map(|s| s.pop_char());
                    self.form.fields.update_strength();
                }
                CredentialType::Api => {
                    self.form.fields.secret_key.as_mut().map(|s| s.pop_char());
                }
                CredentialType::Ssh => {
                    self.form.fields.private_key.as_mut().map(|s| s.pop_char());
                }
            },
            5 if ct == CredentialType::Ssh => {
                self.form.fields.passphrase.as_mut().map(|s| s.pop_char());
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
        ScreenResult::Continue
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
                    // "Go Back" — return to editing
                    ScreenResult::Continue
                } else {
                    // "Save Anyway"
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
                ScreenResult::Continue
            }
            KeyCode::Enter | KeyCode::Char('y') => {
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
            KeyCode::Enter => {
                if self.generator.generator.focus
                    == crate::tui::state::generator_state::GeneratorFocus::ActionButton
                {
                    let pw = self.generator.use_password();
                    self.form.fields.password = Some(SensitiveInput::new());
                    for c in pw.chars() {
                        self.form.fields.password.as_mut().unwrap().push_char(c);
                    }
                    self.form.fields.update_strength();
                    self.form.has_changes = true;
                    return ScreenResult::Continue;
                }
                self.generator.generator.regenerate();
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
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.form.fields.expires_at = options[self.form.expiry_dropdown.selected_index].1;
                self.form.expiry_dropdown.expanded = false;
                self.form.has_changes = true;
                ScreenResult::Continue
            }
            KeyCode::Esc => {
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
        self.form.fields.password = password.map(|p| {
            let mut input = SensitiveInput::new();
            for c in p.chars() {
                input.push_char(c);
            }
            input
        });
        self.form.fields.app_id = app_id;
        self.form.fields.secret_key = secret_key.map(|k| {
            let mut input = SensitiveInput::new();
            for c in k.chars() {
                input.push_char(c);
            }
            input
        });
        self.form.fields.public_key = public_key;
        self.form.fields.private_key = private_key.map(|k| {
            let mut input = SensitiveInput::new();
            for c in k.chars() {
                input.push_char(c);
            }
            input
        });
        self.form.fields.passphrase = passphrase.map(|p| {
            let mut input = SensitiveInput::new();
            for c in p.chars() {
                input.push_char(c);
            }
            input
        });
        self.form.fields.update_strength();
        self.form.fields.tags = tags;
        self.form.fields.notes = notes;
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
                                Some(password.expose().to_string()),
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
                                Some(secret_key.expose().to_string()),
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
                                private_key.map(|k| k.expose().to_string()),
                                passphrase.map(|p| p.expose().to_string()),
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
            let _ = ctx.command_tx.try_send(Command::LoadRecordForEdit { id });
        }
        let _ = ctx.command_tx.try_send(Command::LoadTags);
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
    fn weak_dialog_left_focuses_go_back() {
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
    fn weak_dialog_right_focuses_save_anyway() {
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
    fn weak_dialog_tab_focuses_go_back() {
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
    fn weak_dialog_enter_go_back_returns_to_edit() {
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
    fn weak_dialog_enter_save_anyway_saves() {
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
