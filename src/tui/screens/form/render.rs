//! Full-screen form rendering for U7 Create/Edit.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::tui::components::text_input::PasswordButton;
use crate::tui::components::{dropdown, strength_bar, tag_input, text_input};
use crate::tui::state::form_state::{ExpiryOption, FormState, PasswordFieldFocus};
use crate::tui::theme;
use crate::types::credential::CredentialType;

/// Render the full-screen form.
pub fn render_form(
    frame: &mut Frame,
    area: Rect,
    state: &FormState,
    generator_state: Option<&crate::tui::state::generator_state::EmbeddedGeneratorState>,
    all_tags: &[String],
) {
    let title = match state.mode {
        crate::tui::state::form_state::FormMode::Create => "新建密码",
        crate::tui::state::form_state::FormMode::Edit { .. } => "编辑密码",
    };

    let mut lines = vec![
        // Title bar
        Line::from(vec![
            Span::styled(
                format!("  {}", title),
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("                                        "),
            Span::styled("Esc 取消", Style::default().fg(theme::TEXT_SECONDARY)),
        ]),
        separator_line(area.width),
        Line::raw(""),
    ];

    let ct = state.credential_type;
    let focused = state.focused_field;

    // Field 0: Credential Type dropdown
    let ct_options = ["Login", "API", "SSH"];
    let ct_selected = match ct {
        CredentialType::Login => "Login",
        CredentialType::Api => "API",
        CredentialType::Ssh => "SSH",
    };
    if state.credential_dropdown.expanded {
        let expanded = dropdown::render_dropdown_expanded(
            "凭证类型",
            &ct_options,
            state.credential_dropdown.selected_index,
            area.width,
        );
        lines.extend(expanded);
    } else {
        lines.push(dropdown::render_dropdown(
            "凭证类型",
            ct_selected,
            focused == 0,
            !state.is_credential_type_editable(),
        ));
    }
    lines.push(Line::raw(""));

    // Field 1: Name
    let name_error = state.validation_errors.iter().find(|e| e.field_index == 1);
    lines.extend(text_input::render_text_input(
        "名称",
        &state.fields.name,
        focused == 1,
        name_error.is_some(),
        true,
        false,
        area.width,
    ));
    if let Some(err) = name_error {
        lines.push(error_line(&err.message));
    }
    lines.push(Line::raw(""));

    // Field 2: URL
    let url_label = match ct {
        CredentialType::Ssh => "主机地址",
        _ => "网址",
    };
    lines.extend(text_input::render_text_input(
        url_label,
        &state.fields.url,
        focused == 2,
        false,
        false,
        false,
        area.width,
    ));
    lines.push(Line::raw(""));

    // Credential-type-specific fields (3-4 for Login/API, 3-5 for SSH)
    match ct {
        CredentialType::Login => {
            // Field 3: Username
            let user_error = state.validation_errors.iter().find(|e| e.field_index == 3);
            lines.extend(text_input::render_text_input(
                "用户名",
                state.fields.username.as_deref().unwrap_or(""),
                focused == 3,
                user_error.is_some(),
                true,
                false,
                area.width,
            ));
            if let Some(err) = user_error {
                lines.push(error_line(&err.message));
            }
            lines.push(Line::raw(""));

            // Field 4: Password (special rendering with buttons)
            let pw_error = state.validation_errors.iter().find(|e| e.field_index == 4);
            let login_buttons = [
                PasswordButton {
                    label: "生成",
                    focus_variant: PasswordFieldFocus::Generate,
                },
                PasswordButton {
                    label: "显示",
                    focus_variant: PasswordFieldFocus::Show,
                },
                PasswordButton {
                    label: "复制",
                    focus_variant: PasswordFieldFocus::Copy,
                },
            ];
            let password_row = text_input::render_password_input_with_buttons(
                "密码",
                state.fields.password.as_deref().unwrap_or(""),
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
                lines.push(error_line(&err.message));
            }

            // Strength bar on next line
            if let Some(ref strength) = state.fields.strength {
                lines.push(strength_bar::render_strength_bar(strength));
            } else {
                lines.push(strength_bar::render_empty_strength_bar());
            }

            // Embedded generator panel (if expanded)
            if let Some(gen) = generator_state {
                if gen.expanded {
                    lines.push(Line::raw(""));
                    let panel = crate::tui::components::generator_panel::render_generator_panel(
                        &gen.generator,
                        true,
                        area.width,
                    );
                    lines.extend(panel);
                }
            }
        }
        CredentialType::Api => {
            // Field 3: AppID
            let appid_error = state.validation_errors.iter().find(|e| e.field_index == 3);
            lines.extend(text_input::render_text_input(
                "AppID",
                state.fields.app_id.as_deref().unwrap_or(""),
                focused == 3,
                appid_error.is_some(),
                true,
                false,
                area.width,
            ));
            if let Some(err) = appid_error {
                lines.push(error_line(&err.message));
            }
            lines.push(Line::raw(""));

            // Field 4: SecretKey
            let api_buttons = [
                PasswordButton {
                    label: "显示",
                    focus_variant: PasswordFieldFocus::Show,
                },
                PasswordButton {
                    label: "复制",
                    focus_variant: PasswordFieldFocus::Copy,
                },
            ];
            let secret_row = text_input::render_password_input_with_buttons(
                "SecretKey",
                state.fields.secret_key.as_deref().unwrap_or(""),
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
                    label: "粘贴",
                    focus_variant: PasswordFieldFocus::Paste,
                },
                PasswordButton {
                    label: "复制",
                    focus_variant: PasswordFieldFocus::Copy,
                },
            ];
            lines.extend(text_input::render_password_input_with_buttons(
                "公钥",
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
                lines.push(error_line(&err.message));
            }
            lines.push(Line::raw(""));

            // Field 4: Private Key
            let privkey_buttons = [
                PasswordButton {
                    label: "显示",
                    focus_variant: PasswordFieldFocus::Show,
                },
                PasswordButton {
                    label: "粘贴",
                    focus_variant: PasswordFieldFocus::Paste,
                },
                PasswordButton {
                    label: "复制",
                    focus_variant: PasswordFieldFocus::Copy,
                },
            ];
            lines.extend(text_input::render_password_input_with_buttons(
                "私钥",
                state.fields.private_key.as_deref().unwrap_or(""),
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
                "        (可选)",
                Style::default().fg(theme::TEXT_MUTED),
            )));
            lines.push(Line::raw(""));

            // Field 5: Passphrase
            let passphrase_buttons = [
                PasswordButton {
                    label: "显示",
                    focus_variant: PasswordFieldFocus::Show,
                },
                PasswordButton {
                    label: "粘贴",
                    focus_variant: PasswordFieldFocus::Paste,
                },
                PasswordButton {
                    label: "复制",
                    focus_variant: PasswordFieldFocus::Copy,
                },
            ];
            lines.extend(text_input::render_password_input_with_buttons(
                "Passphrase",
                state.fields.passphrase.as_deref().unwrap_or(""),
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
                "        (选填)",
                Style::default().fg(theme::TEXT_MUTED),
            )));
        }
    }
    lines.push(Line::raw(""));

    // Expiry field (index depends on credential type)
    let expiry_idx = match ct {
        CredentialType::Login | CredentialType::Api => 5,
        CredentialType::Ssh => 6,
    };
    if state.expiry_dropdown.expanded {
        let options: Vec<&str> = ExpiryOption::all_options()
            .iter()
            .map(|(l, _)| *l)
            .collect();
        lines.extend(dropdown::render_dropdown_expanded(
            "过期时间",
            &options,
            state.expiry_dropdown.selected_index,
            area.width,
        ));
    } else {
        let expiry_label = state.fields.expires_at.label();
        lines.push(dropdown::render_dropdown(
            "过期时间",
            expiry_label,
            focused == expiry_idx,
            false,
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
            "              格式: YYYY-MM-DD",
            Style::default().fg(theme::TEXT_SECONDARY),
        )));
    }
    lines.push(Line::raw(""));

    // Tags field
    let tags_idx = match ct {
        CredentialType::Login | CredentialType::Api => 6,
        CredentialType::Ssh => 7,
    };
    lines.extend(tag_input::render_tag_input(
        &state.fields.tag_input,
        &state.fields.tags,
        focused == tags_idx,
        state.tag_autocomplete.as_ref(),
        all_tags,
        area.width,
    ));
    lines.push(Line::raw(""));

    // Notes field
    let notes_idx = match ct {
        CredentialType::Login | CredentialType::Api => 7,
        CredentialType::Ssh => 8,
    };
    lines.extend(text_input::render_text_input(
        "备注",
        &state.fields.notes,
        focused == notes_idx,
        false,
        false,
        false,
        area.width,
    ));
    lines.push(Line::raw(""));

    // Bottom buttons
    lines.push(separator_line(area.width));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::raw("                     "),
        Span::styled(
            " [ Ctrl+S 保存 ] ",
            Style::default()
                .fg(theme::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(" [ 取消 ] ", Style::default().fg(theme::TEXT_SECONDARY)),
    ]));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(theme::BG));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(Clear, area);
    frame.render_widget(paragraph, area);

    // Weak password dialog overlay
    if state.show_weak_password_dialog {
        render_weak_password_dialog(frame, area);
    }

    // Unsaved changes dialog overlay
    if state.show_unsaved_dialog {
        render_unsaved_dialog(frame, area);
    }
}

fn render_weak_password_dialog(frame: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(Span::styled(
            "  ⚠ 密码强度不足",
            Style::default()
                .fg(theme::WARNING)
                .add_modifier(Modifier::BOLD),
        )),
        separator_line(48),
        Line::raw(""),
        Line::from(Span::raw("  当前密码强度为\"弱\"，容易受到暴力破解攻击。")),
        Line::raw(""),
        Line::from(Span::raw("  建议使用生成器创建更强的密码。")),
        Line::raw(""),
        separator_line(48),
        Line::raw(""),
        Line::from(vec![
            Span::raw("      "),
            Span::styled(" [ 返回修改 ] ", Style::default().fg(theme::PRIMARY)),
            Span::raw("    "),
            Span::styled(" [ 仍然保存 ] ", Style::default().fg(theme::WARNING)),
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

fn render_unsaved_dialog(frame: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(Span::styled(
            "  未保存的更改",
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )),
        separator_line(40),
        Line::raw(""),
        Line::from(Span::raw("  你有未保存的更改，确定要丢弃吗？")),
        Line::raw(""),
        separator_line(40),
        Line::raw(""),
        Line::from(vec![
            Span::raw("      "),
            Span::styled(" [ 继续编辑 ] ", Style::default().fg(theme::PRIMARY)),
            Span::raw("    "),
            Span::styled(" [ 丢弃 ] ", Style::default().fg(theme::ERROR)),
        ]),
    ];
    let w = 40.min(area.width);
    let h = 9.min(area.height);
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
