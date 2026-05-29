//! Full-screen form rendering for U7 Create/Edit.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::t;
use crate::tui::components::text_input::PasswordButton;
use crate::tui::components::{dropdown, strength_bar, tag_input, text_input, textarea};
use crate::tui::state::form_state::{
    ExpiryOption, FormFooterButton, FormState, PasswordFieldFocus,
};
use crate::tui::state::generator_state::GeneratorState;
use crate::tui::theme;
use crate::types::credential::CredentialType;
use unicode_width::UnicodeWidthStr;

/// Render the full-screen form.
pub fn render_form(
    frame: &mut Frame,
    area: Rect,
    state: &FormState,
    generator_state: Option<&crate::tui::state::generator_state::EmbeddedGeneratorState>,
    all_tags: &[String],
    _unicode: bool,
) {
    let title = match state.mode {
        crate::tui::state::form_state::FormMode::Create => t!("tui.form.create_title"),
        crate::tui::state::form_state::FormMode::Edit { .. } => t!("tui.form.edit_title"),
    };

    let mut lines = vec![
        title_line(
            area.width,
            title.as_ref(),
            t!("tui.form.cancel_hint").as_ref(),
        ),
        separator_line(area.width),
        Line::raw(""),
    ];

    let ct = state.credential_type;
    let focused = state.focused_field;

    // Track where the notes textarea starts in the line buffer so we can
    // overlay the actual TextArea widget after the Paragraph is rendered.
    // None means no textarea is rendered (should not happen currently).
    let mut notes_line_offset: Option<(usize, u16)> = None;

    // Field 0: Credential Type dropdown
    let ct_options = [
        t!("tui.form.type_login"),
        t!("tui.form.type_api"),
        t!("tui.form.type_ssh"),
        t!("tui.form.type_secure_note"),
    ];
    let ct_selected = match ct {
        CredentialType::Login => t!("tui.form.type_login"),
        CredentialType::Api => t!("tui.form.type_api"),
        CredentialType::Ssh => t!("tui.form.type_ssh"),
        CredentialType::SecureNote => t!("tui.form.type_secure_note"),
    };
    if state.credential_dropdown.expanded {
        let expanded = dropdown::render_dropdown_expanded(
            t!("tui.form.type_label").as_ref(),
            &ct_options.iter().map(|s| s.as_ref()).collect::<Vec<_>>(),
            state.credential_dropdown.selected_index,
            area.width,
            _unicode,
        );
        lines.extend(expanded);
    } else {
        lines.push(dropdown::render_dropdown(
            t!("tui.form.type_label").as_ref(),
            ct_selected.as_ref(),
            focused == 0,
            !state.is_credential_type_editable(),
            _unicode,
        ));
    }
    lines.push(Line::raw(""));

    // Field 1: Name
    let name_error = state.validation_errors.iter().find(|e| e.field_index == 1);
    lines.extend(text_input::render_text_input(
        t!("tui.form.name_label").as_ref(),
        &state.fields.name,
        focused == 1,
        name_error.is_some(),
        true,
        false,
        area.width,
    ));
    if let Some(err) = name_error {
        if should_render_error_line(&err.message) {
            lines.push(error_line(&err.message));
        }
    }
    lines.push(Line::raw(""));

    // Credential-type-specific fields
    match ct {
        CredentialType::SecureNote => {
            // Field 2: Notes textarea — record offset for overlay rendering
            let notes_rows = textarea::visible_rows(&state.fields.notes);
            notes_line_offset = Some((lines.len(), notes_rows));
            lines.extend(textarea::render_textarea_label(
                t!("tui.form.notes_label").as_ref(),
                focused == 2,
                false,
                area.width,
                notes_rows,
            ));
            lines.push(Line::raw(""));
        }
        _ => {
            // Field 2: URL (for Login, Api, Ssh)
            let url_label = match ct {
                CredentialType::Ssh => t!("tui.form.hostname_label"),
                _ => t!("tui.form.url_label"),
            };
            lines.extend(text_input::render_text_input(
                url_label.as_ref(),
                &state.fields.url,
                focused == 2,
                false,
                false,
                false,
                area.width,
            ));
            lines.push(Line::raw(""));
        }
    }

    // Credential-type-specific fields (3-4 for Login/API, 3-5 for SSH)
    match ct {
        CredentialType::Login => {
            // Field 3: Username
            let user_error = state.validation_errors.iter().find(|e| e.field_index == 3);
            lines.extend(text_input::render_text_input(
                t!("tui.form.username_label").as_ref(),
                state.fields.username.as_deref().unwrap_or(""),
                focused == 3,
                user_error.is_some(),
                true,
                false,
                area.width,
            ));
            if let Some(err) = user_error {
                if should_render_error_line(&err.message) {
                    lines.push(error_line(&err.message));
                }
            }
            lines.push(Line::raw(""));

            // Field 4: Password (special rendering with buttons)
            let pw_error = state.validation_errors.iter().find(|e| e.field_index == 4);
            let login_buttons = [
                PasswordButton {
                    label: t!("tui.form.generate_button").to_string(),
                    focus_variant: PasswordFieldFocus::Generate,
                },
                PasswordButton {
                    label: t!("tui.form.show_button").to_string(),
                    focus_variant: PasswordFieldFocus::Show,
                },
                PasswordButton {
                    label: t!("tui.form.copy_button").to_string(),
                    focus_variant: PasswordFieldFocus::Copy,
                },
            ];
            let password_row = text_input::render_password_input_with_buttons(
                t!("tui.form.password_label").as_ref(),
                state
                    .fields
                    .password
                    .as_ref()
                    .map(|p| {
                        if state.fields.password_visible {
                            p.expose(|s| s.to_string())
                        } else {
                            p.expose(|s| {
                                crate::tui::theme::ICON_PASSWORD_MASK.repeat(s.chars().count())
                            })
                        }
                    })
                    .unwrap_or_default()
                    .as_str(),
                focused == 4,
                pw_error.is_some(),
                state.fields.password_visible,
                &login_buttons,
                if focused == 4 {
                    Some(state.password_sub_focus)
                } else {
                    None
                },
                area.width,
            );
            lines.extend(password_row);
            if let Some(err) = pw_error {
                if should_render_error_line(&err.message) {
                    lines.push(error_line(&err.message));
                }
            }

            // Strength bar with breathing room after the password input row.
            lines.push(Line::raw(""));
            if let Some(ref strength) = state.fields.strength {
                lines.push(strength_bar::render_form_strength_bar(strength, _unicode));
            } else {
                lines.push(strength_bar::render_form_empty_strength_bar());
            }
        }
        CredentialType::Api => {
            // Field 3: AppID
            let appid_error = state.validation_errors.iter().find(|e| e.field_index == 3);
            lines.extend(text_input::render_text_input(
                t!("tui.form.app_id_label").as_ref(),
                state.fields.app_id.as_deref().unwrap_or(""),
                focused == 3,
                appid_error.is_some(),
                true,
                false,
                area.width,
            ));
            if let Some(err) = appid_error {
                if should_render_error_line(&err.message) {
                    lines.push(error_line(&err.message));
                }
            }
            lines.push(Line::raw(""));

            // Field 4: SecretKey
            let api_buttons = [
                PasswordButton {
                    label: t!("tui.form.show_button").to_string(),
                    focus_variant: PasswordFieldFocus::Show,
                },
                PasswordButton {
                    label: t!("tui.form.copy_button").to_string(),
                    focus_variant: PasswordFieldFocus::Copy,
                },
            ];
            let secret_row = text_input::render_password_input_with_buttons(
                t!("tui.form.secret_key_label").as_ref(),
                state
                    .fields
                    .secret_key
                    .as_ref()
                    .map(|k| {
                        if state.fields.secret_visible {
                            k.expose(|s| s.to_string())
                        } else {
                            k.expose(|s| {
                                crate::tui::theme::ICON_PASSWORD_MASK.repeat(s.chars().count())
                            })
                        }
                    })
                    .unwrap_or_default()
                    .as_str(),
                focused == 4,
                false,
                state.fields.secret_visible,
                &api_buttons,
                if focused == 4 {
                    Some(state.password_sub_focus)
                } else {
                    None
                },
                area.width,
            );
            lines.extend(secret_row);
        }
        CredentialType::Ssh => {
            // Field 3: Public Key
            let pubkey_error = state.validation_errors.iter().find(|e| e.field_index == 3);
            let pubkey_buttons = [
                PasswordButton {
                    label: t!("tui.form.paste_button").to_string(),
                    focus_variant: PasswordFieldFocus::Paste,
                },
                PasswordButton {
                    label: t!("tui.form.copy_button").to_string(),
                    focus_variant: PasswordFieldFocus::Copy,
                },
            ];
            lines.extend(text_input::render_password_input_with_buttons(
                t!("tui.form.public_key_label").as_ref(),
                state.fields.public_key.as_deref().unwrap_or(""),
                focused == 3,
                pubkey_error.is_some(),
                true, // public key is always visible
                &pubkey_buttons,
                if focused == 3 {
                    Some(state.password_sub_focus)
                } else {
                    None
                },
                area.width,
            ));
            if let Some(err) = pubkey_error {
                if should_render_error_line(&err.message) {
                    lines.push(error_line(&err.message));
                }
            }
            lines.push(Line::raw(""));

            // Field 4: Private Key
            let privkey_buttons = [
                PasswordButton {
                    label: t!("tui.form.show_button").to_string(),
                    focus_variant: PasswordFieldFocus::Show,
                },
                PasswordButton {
                    label: t!("tui.form.paste_button").to_string(),
                    focus_variant: PasswordFieldFocus::Paste,
                },
                PasswordButton {
                    label: t!("tui.form.copy_button").to_string(),
                    focus_variant: PasswordFieldFocus::Copy,
                },
            ];
            lines.extend(text_input::render_password_input_with_buttons(
                t!("tui.form.private_key_label").as_ref(),
                state
                    .fields
                    .private_key
                    .as_ref()
                    .map(|k| {
                        if state.fields.private_visible {
                            k.expose(|s| s.to_string())
                        } else {
                            k.expose(|s| {
                                crate::tui::theme::ICON_PASSWORD_MASK.repeat(s.chars().count())
                            })
                        }
                    })
                    .unwrap_or_default()
                    .as_str(),
                focused == 4,
                false,
                state.fields.private_visible,
                &privkey_buttons,
                if focused == 4 {
                    Some(state.password_sub_focus)
                } else {
                    None
                },
                area.width,
            ));
            lines.push(Line::from(Span::styled(
                format!("        {}", t!("tui.form.optional_marker")),
                Style::default().fg(theme::TEXT_MUTED),
            )));
            lines.push(Line::raw(""));

            // Field 5: Passphrase
            let passphrase_buttons = [
                PasswordButton {
                    label: t!("tui.form.show_button").to_string(),
                    focus_variant: PasswordFieldFocus::Show,
                },
                PasswordButton {
                    label: t!("tui.form.paste_button").to_string(),
                    focus_variant: PasswordFieldFocus::Paste,
                },
                PasswordButton {
                    label: t!("tui.form.copy_button").to_string(),
                    focus_variant: PasswordFieldFocus::Copy,
                },
            ];
            lines.extend(text_input::render_password_input_with_buttons(
                "Passphrase", // Keep "Passphrase" as is - it's a technical term
                state
                    .fields
                    .passphrase
                    .as_ref()
                    .map(|p| {
                        if state.fields.passphrase_visible {
                            p.expose(|s| s.to_string())
                        } else {
                            p.expose(|s| {
                                crate::tui::theme::ICON_PASSWORD_MASK.repeat(s.chars().count())
                            })
                        }
                    })
                    .unwrap_or_default()
                    .as_str(),
                focused == 5,
                false,
                state.fields.passphrase_visible,
                &passphrase_buttons,
                if focused == 5 {
                    Some(state.password_sub_focus)
                } else {
                    None
                },
                area.width,
            ));
            lines.push(Line::from(Span::styled(
                format!("        {}", t!("tui.form.optional_marker")),
                Style::default().fg(theme::TEXT_MUTED),
            )));
        }
        CredentialType::SecureNote => {
            // No additional fields for SecureNote - notes already rendered above
        }
    }
    lines.push(Line::raw(""));

    // Expiry field (index depends on credential type)
    let expiry_idx = match ct {
        CredentialType::Login | CredentialType::Api => 5,
        CredentialType::Ssh => 6,
        CredentialType::SecureNote => 3,
    };
    if state.expiry_dropdown.expanded {
        let options = ExpiryOption::all_options();
        lines.extend(dropdown::render_dropdown_expanded(
            t!("tui.form.expiry_label").as_ref(),
            &options.iter().map(|(l, _)| l.as_ref()).collect::<Vec<_>>(),
            state.expiry_dropdown.selected_index,
            area.width,
            _unicode,
        ));
    } else {
        let expiry_label = state.fields.expires_at.label();
        lines.push(dropdown::render_dropdown(
            t!("tui.form.expiry_label").as_ref(),
            &expiry_label,
            focused == expiry_idx,
            false,
            _unicode,
        ));
    }

    // Custom date input
    if state.fields.expires_at == ExpiryOption::Custom {
        let date_val = state.fields.custom_date.as_deref().unwrap_or("YYYY-MM-DD");
        lines.push(Line::from(vec![
            Span::raw("              "),
            Span::styled(
                format!("[{}]", date_val),
                Style::default().fg(theme::TEXT).bg(theme::BG_SURFACE),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            format!("              {}", t!("tui.form.custom_date_placeholder")),
            Style::default().fg(theme::TEXT_SECONDARY),
        )));
    }
    lines.push(Line::raw(""));

    // Tags field
    let tags_idx = match ct {
        CredentialType::Login | CredentialType::Api => 6,
        CredentialType::Ssh => 7,
        CredentialType::SecureNote => 4,
    };
    lines.extend(tag_input::render_tag_input(
        &state.fields.tag_input,
        &state.fields.tags,
        focused == tags_idx,
        state.fields.tag_focus,
        state.tag_autocomplete.as_ref(),
        all_tags,
        area.width,
    ));
    lines.push(Line::raw(""));

    // Notes field (only for Login, Api, Ssh - SecureNote already rendered above)
    let notes_idx = match ct {
        CredentialType::Login | CredentialType::Api => 7,
        CredentialType::Ssh => 8,
        CredentialType::SecureNote => 2, // Already rendered
    };
    if ct != CredentialType::SecureNote {
        let notes_rows = textarea::visible_rows(&state.fields.notes);
        notes_line_offset = Some((lines.len(), notes_rows));
        lines.extend(textarea::render_textarea_label(
            t!("tui.form.notes_label").as_ref(),
            focused == notes_idx,
            false,
            area.width,
            notes_rows,
        ));
        lines.push(Line::raw(""));
    }

    let inner_height = area.height.saturating_sub(2) as usize;
    let footer_rows = 3usize;
    while lines.len().saturating_add(footer_rows) < inner_height {
        lines.push(Line::raw(""));
    }

    // Bottom buttons
    lines.push(separator_line(area.width));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("[ {} ]", t!("tui.form.save_button")),
            footer_button_style(state.footer_focus, FormFooterButton::Save, true),
        ),
        Span::raw("  "),
        Span::styled(
            format!("[ {} ]", t!("tui.form.cancel_button")),
            footer_button_style(state.footer_focus, FormFooterButton::Cancel, false),
        ),
    ]));
    lines.push(shortcut_line());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(theme::BG));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(Clear, area);
    frame.render_widget(paragraph, area);

    // Overlay the actual TextArea widget on top of the placeholder rows.
    // No block border on the textarea — content lines only.
    if let Some((offset, notes_rows)) = notes_line_offset {
        let label_width = crate::tui::components::text_input::FORM_LABEL_WIDTH;
        let notes_area = Rect {
            x: area.x + 1 + label_width as u16,
            y: area.y + 1 + offset as u16 + 1, // outer block border + label line
            width: area.width.saturating_sub(label_width as u16 + 3),
            height: notes_rows,
        };
        frame.render_widget(&state.fields.notes, notes_area);
    }

    if let Some(gen) = generator_state {
        if gen.expanded {
            render_generator_dialog(frame, area, &gen.generator, _unicode);
        }
    }

    // Weak password dialog overlay
    if state.show_weak_password_dialog {
        render_weak_password_dialog(frame, area, state.weak_dialog_focus);
    }

    // Unsaved changes dialog overlay
    if state.show_unsaved_dialog {
        render_unsaved_dialog(frame, area, state.unsaved_dialog_focus);
    }
}

pub(crate) fn generator_dialog_area(area: Rect, state: &GeneratorState, unicode: bool) -> Rect {
    let width = if area.width > 68 {
        64
    } else {
        area.width.saturating_sub(4).max(24)
    };
    let panel_len = crate::tui::components::generator_panel::render_generator_panel(
        state,
        true,
        width.saturating_sub(2),
        unicode,
    )
    .len() as u16;
    let height = panel_len
        .saturating_add(5)
        .min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

fn render_generator_dialog(frame: &mut Frame, area: Rect, state: &GeneratorState, unicode: bool) {
    let dialog_area = generator_dialog_area(area, state, unicode);
    let inner_width = dialog_area.width.saturating_sub(2);
    let title = t!("tui.generator_overlay.title").to_string();
    let close_hint = t!("tui.form.cancel_hint").to_string();
    let title_width = UnicodeWidthStr::width(title.as_str());
    let hint_width = UnicodeWidthStr::width(close_hint.as_str());
    let gap = inner_width
        .saturating_sub(title_width as u16)
        .saturating_sub(hint_width as u16)
        .max(1) as usize;

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                title,
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" ".repeat(gap)),
            Span::styled(close_hint, Style::default().fg(theme::TEXT_SECONDARY)),
        ]),
        separator_line(dialog_area.width),
        Line::raw(""),
    ];
    lines.extend(
        crate::tui::components::generator_panel::render_generator_panel(
            state,
            true,
            inner_width,
            unicode,
        ),
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(theme::BG));
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(Clear, dialog_area);
    frame.render_widget(paragraph, dialog_area);
}

fn title_line(width: u16, title: &str, hint: &str) -> Line<'static> {
    let left = format!("  {title}");
    let right = format!("{hint}  ");
    let content_width = width.saturating_sub(2) as usize;
    let left_width = UnicodeWidthStr::width(left.as_str());
    let right_width = UnicodeWidthStr::width(right.as_str());
    let gap = content_width
        .saturating_sub(left_width + right_width)
        .max(1);

    Line::from(vec![
        Span::styled(
            left,
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(gap)),
        Span::styled(right, Style::default().fg(theme::TEXT_SECONDARY)),
    ])
}

fn footer_button_style(
    focus: Option<FormFooterButton>,
    button: FormFooterButton,
    primary: bool,
) -> Style {
    let base = if primary {
        Style::default()
            .fg(theme::PRIMARY)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };

    if focus == Some(button) {
        base.add_modifier(Modifier::REVERSED)
    } else {
        base
    }
}

fn shortcut_line() -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled("Ctrl+G", Style::default().fg(theme::PRIMARY)),
        Span::styled(
            format!(" {}  ", t!("tui.form.shortcut_generate")),
            Style::default().fg(theme::TEXT_SECONDARY),
        ),
        Span::styled("Ctrl+V", Style::default().fg(theme::PRIMARY)),
        Span::styled(
            format!(" {}  ", t!("tui.form.shortcut_toggle_visibility")),
            Style::default().fg(theme::TEXT_SECONDARY),
        ),
        Span::styled("Ctrl+C", Style::default().fg(theme::PRIMARY)),
        Span::styled(
            format!(" {}  ", t!("tui.form.shortcut_copy")),
            Style::default().fg(theme::TEXT_SECONDARY),
        ),
        Span::styled("Ctrl+S", Style::default().fg(theme::PRIMARY)),
        Span::styled(
            format!(" {}  ", t!("tui.form.shortcut_save")),
            Style::default().fg(theme::TEXT_SECONDARY),
        ),
        Span::styled("Esc", Style::default().fg(theme::PRIMARY)),
        Span::styled(
            format!(" {}", t!("tui.form.shortcut_cancel")),
            Style::default().fg(theme::TEXT_SECONDARY),
        ),
    ])
}

fn render_weak_password_dialog(frame: &mut Frame, area: Rect, focus: usize) {
    let go_back_style = if focus == 0 {
        Style::default()
            .fg(theme::PRIMARY)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };
    let save_anyway_style = if focus == 1 {
        Style::default()
            .fg(theme::PRIMARY)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };

    let lines = vec![
        Line::from(Span::styled(
            format!(
                "  {} {}",
                theme::ICON_WARNING,
                t!("tui.overlay.weak_password_title")
            ),
            Style::default()
                .fg(theme::WARNING)
                .add_modifier(Modifier::BOLD),
        )),
        separator_line(48),
        Line::raw(""),
        Line::from(Span::raw(format!(
            "  {}",
            t!("tui.form.weak_password_hint")
        ))),
        Line::raw(""),
        Line::from(Span::raw(format!(
            "  {}",
            t!("tui.form.weak_password_suggestion")
        ))),
        Line::raw(""),
        separator_line(48),
        Line::raw(""),
        Line::from(vec![
            Span::raw("      "),
            Span::styled(format!("[ {} ]", t!("tui.form.go_back")), go_back_style),
            Span::raw("      "),
            Span::styled(
                format!("[ {} ]", t!("tui.form.save_anyway")),
                save_anyway_style,
            ),
        ]),
    ];
    // Render centered
    let w = 48.min(area.width);
    let h = 10.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let dialog_area = Rect::new(x, y, w, h);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(theme::BG));
    let p = Paragraph::new(lines).block(block);
    frame.render_widget(Clear, dialog_area);
    frame.render_widget(p, dialog_area);
}

fn render_unsaved_dialog(frame: &mut Frame, area: Rect, focus: usize) {
    let key_style = Style::default()
        .fg(theme::PRIMARY)
        .add_modifier(Modifier::BOLD);
    let continue_style = if focus == 0 {
        Style::default()
            .fg(theme::BG)
            .bg(theme::PRIMARY)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::PRIMARY)
    };
    let discard_style = if focus == 1 {
        Style::default()
            .fg(theme::BG)
            .bg(theme::ERROR)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::ERROR)
    };
    let lines = vec![
        Line::from(Span::styled(
            format!("  {}", t!("tui.overlay.unsaved_title")),
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )),
        separator_line(52),
        Line::raw(""),
        Line::from(Span::raw(format!("  {}", t!("tui.overlay.unsaved_body")))),
        Line::raw(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Esc", key_style),
            Span::raw(format!(" {}    ", t!("tui.form.unsaved_cancel_shortcut"))),
            Span::styled("Enter", key_style),
            Span::raw(format!(" {}", t!("tui.form.unsaved_discard_shortcut"))),
        ]),
        Line::raw(""),
        separator_line(52),
        Line::raw(""),
        Line::from(vec![
            Span::raw("        "),
            Span::styled(
                format!(" {} ", t!("tui.form.continue_editing")),
                continue_style,
            ),
            Span::raw("      "),
            Span::styled(
                format!(" {} ", t!("tui.form.discard_changes")),
                discard_style,
            ),
        ]),
    ];
    let w = 52.min(area.width);
    let h = 12.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let dialog_area = Rect::new(x, y, w, h);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(theme::BG));
    let p = Paragraph::new(lines).block(block);
    frame.render_widget(Clear, dialog_area);
    frame.render_widget(p, dialog_area);
}

fn separator_line(width: u16) -> Line<'static> {
    Line::from(Span::styled(
        "─".repeat(width.saturating_sub(2) as usize),
        Style::default().fg(theme::BORDER),
    ))
}

fn error_line(msg: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {}", msg),
        Style::default().fg(theme::ERROR),
    ))
}

fn should_render_error_line(msg: &str) -> bool {
    msg != t!("tui.form.validation_required").as_ref()
}
