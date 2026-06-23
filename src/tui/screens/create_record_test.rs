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
    let _guard = crate::tui::i18n::LocaleGuard::en();
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
fn notes_textarea_up_at_first_line_moves_to_previous_field() {
    let (tx, _rx) = mpsc::channel(1);
    let mut screen = make_screen();
    let env = TestEnv::new();
    let mut ctx = env.make_ctx(&tx);
    screen.form.fields.set_notes_text("first\nsecond");
    screen.form.focus_field(7);
    screen.form.fields.notes.move_cursor(CursorMove::Top);

    let result = screen.update(Message::KeyEvent(key(KeyCode::Up)), &mut ctx);

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.form.focused_field, 6);
}

#[test]
fn notes_textarea_down_at_last_line_moves_to_next_target() {
    let (tx, _rx) = mpsc::channel(1);
    let mut screen = make_screen();
    let env = TestEnv::new();
    let mut ctx = env.make_ctx(&tx);
    screen
        .form
        .switch_credential_type(CredentialType::SecureNote);
    screen.form.fields.set_notes_text("first\nsecond");
    screen.form.focus_field(2);
    screen.form.fields.notes.move_cursor(CursorMove::Bottom);

    let result = screen.update(Message::KeyEvent(key(KeyCode::Down)), &mut ctx);

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.form.focused_field, 3);
}

#[test]
fn notes_textarea_expands_with_content_up_to_eight_visible_lines() {
    let mut screen = make_screen();
    screen
        .form
        .switch_credential_type(CredentialType::SecureNote);
    screen
        .form
        .fields
        .set_notes_text("line1\nline2\nline3\nline4\nline5\nline6");
    screen.form.focus_field(2);
    screen.form.fields.notes.move_cursor(CursorMove::Top);

    let buffer = render_buffer(&screen, 100, 32);

    assert!(find_text(&buffer, "line1").is_some());
    assert!(find_text(&buffer, "line6").is_some());
}

#[test]
fn type_dropdown_renders_secure_note_label() {
    let mut screen = make_screen();
    screen.form.credential_dropdown.expanded = true;

    let buffer = render_buffer(&screen, 100, 32);

    assert!(find_text(&buffer, "Secure Note").is_some());
    assert!(find_text(&buffer, "tui.form.type_secure_note").is_none());
}

#[test]
fn keyboard_can_select_secure_note_type() {
    let (tx, _rx) = mpsc::channel(1);
    let mut screen = make_screen();
    let env = TestEnv::new();
    let mut ctx = env.make_ctx(&tx);

    screen.update(Message::KeyEvent(key(KeyCode::Right)), &mut ctx);
    screen.update(Message::KeyEvent(key(KeyCode::Down)), &mut ctx);
    screen.update(Message::KeyEvent(key(KeyCode::Down)), &mut ctx);
    screen.update(Message::KeyEvent(key(KeyCode::Down)), &mut ctx);
    let result = screen.update(Message::KeyEvent(key(KeyCode::Enter)), &mut ctx);

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.form.credential_type, CredentialType::SecureNote);
    assert!(!screen.form.credential_dropdown.expanded);
}

#[test]
fn mouse_can_select_secure_note_type() {
    let (tx, _rx) = mpsc::channel(1);
    let mut screen = make_screen();
    let env = TestEnv::new();
    let mut ctx = env.make_ctx(&tx);

    screen.form.credential_dropdown.expanded = true;
    let buffer = render_buffer(&screen, 100, 32);
    let (x, y) = find_text(&buffer, "Secure Note").expect("secure note option should render");
    let result = screen.update(Message::MouseEvent(click(x, y)), &mut ctx);

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.form.credential_type, CredentialType::SecureNote);
    assert!(!screen.form.credential_dropdown.expanded);
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
fn valid_save_enters_saving_state_and_queues_create_command() {
    let (tx, mut rx) = mpsc::channel(2);
    let mut screen = make_screen();
    let env = TestEnv::new();
    let mut ctx = env.make_ctx(&tx);
    screen.form.fields.name = "Github Page".into();
    screen.form.fields.username = Some("p1024k".into());
    for c in "correct horse battery staple 2026!".chars() {
        screen.form.fields.password.as_mut().unwrap().push_char(c);
    }
    screen.form.fields.tags = vec!["github".into()];

    let result = screen.update(Message::KeyEvent(ctrl('s')), &mut ctx);

    assert!(matches!(result, ScreenResult::Continue));
    assert!(screen.form.saving);
    assert!(matches!(
        rx.try_recv().unwrap(),
        Command::CreateRecord { .. }
    ));
    let buffer = render_buffer(&screen, 100, 32);
    assert!(find_text(&buffer, "Saving")
        .or_else(|| find_text(&buffer, "正在保存"))
        .is_some());
}

#[test]
fn saving_form_ignores_duplicate_save_keys() {
    let (tx, mut rx) = mpsc::channel(3);
    let mut screen = make_screen();
    let env = TestEnv::new();
    let mut ctx = env.make_ctx(&tx);
    screen.form.fields.name = "Github Page".into();
    screen.form.fields.username = Some("p1024k".into());
    for c in "correct horse battery staple 2026!".chars() {
        screen.form.fields.password.as_mut().unwrap().push_char(c);
    }

    assert!(matches!(
        screen.update(Message::KeyEvent(ctrl('s')), &mut ctx),
        ScreenResult::Continue
    ));
    assert!(matches!(
        rx.try_recv().unwrap(),
        Command::CreateRecord { .. }
    ));

    assert!(matches!(
        screen.update(Message::KeyEvent(ctrl('s')), &mut ctx),
        ScreenResult::Continue
    ));
    assert!(matches!(
        rx.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
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
fn tag_input_stops_at_ten_tags() {
    let (tx, _rx) = mpsc::channel(1);
    let mut screen = make_screen();
    let env = TestEnv::new();
    let mut ctx = env.make_ctx(&tx);
    screen.form.focus_field(6);
    screen.form.fields.tags = (0..10).map(|n| format!("tag-{n}")).collect();

    let result = screen.update(Message::KeyEvent(key(KeyCode::Char('x'))), &mut ctx);

    assert!(matches!(result, ScreenResult::Continue));
    assert!(screen.form.fields.tag_input.is_empty());
    assert_eq!(screen.form.fields.tags.len(), 10);
    assert!(screen
        .form
        .validation_errors
        .iter()
        .any(|error| error.field_index == 6));
}

#[test]
fn name_input_stops_at_limit() {
    let (tx, _rx) = mpsc::channel(1);
    let mut screen = make_screen();
    let env = TestEnv::new();
    let mut ctx = env.make_ctx(&tx);
    screen.form.focus_field(1);
    screen.form.fields.name = "a".repeat(120);

    let result = screen.update(Message::KeyEvent(key(KeyCode::Char('b'))), &mut ctx);

    assert!(matches!(result, ScreenResult::Continue));
    assert_eq!(screen.form.fields.name.chars().count(), 120);
    assert!(screen
        .form
        .validation_errors
        .iter()
        .any(|error| error.field_index == 1));
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
        // notes is a multi-line textarea — intentionally spans multiple rows
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
fn weak_dialog_left_focuses_cancel() {
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
fn weak_dialog_right_focuses_save() {
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
fn weak_dialog_tab_focuses_cancel() {
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
fn weak_dialog_enter_cancel_returns_to_edit() {
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
fn weak_dialog_enter_save_saves() {
    let (tx, mut rx) = mpsc::channel(1);
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
    assert!(matches!(result, ScreenResult::Continue));
    assert!(!screen.form.show_weak_password_dialog);
    assert!(screen.form.saving);
    assert!(matches!(
        rx.try_recv().unwrap(),
        Command::CreateRecord { .. }
    ));
}
