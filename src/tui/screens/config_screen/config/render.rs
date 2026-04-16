use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, layout::Rect};

use crate::tui::state::config_state::{ConfigTab, ConfigScreenState, PasswordDefaultsForm};

pub fn render(frame: &mut Frame, area: Rect, state: &ConfigScreenState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header with tabs
            Constraint::Min(0),    // Content
            Constraint::Length(1), // Footer
        ])
        .split(area);

    // Tab bar
    let tab_names = ["常规", "同步", "安全", "密码", "关于"];
    let tabs_text: Vec<String> = ConfigTab::all()
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let name = tab_names[i];
            if *tab == state.active_tab {
                format!(" {} ", name)
            } else {
                format!(" {} ", name)
            }
        })
        .collect();
    let tabs_display = tabs_text.join("\u{2502}");
    let header = Paragraph::new(format!(" 配置  {}", tabs_display))
        .style(Style::default().fg(Color::White).bold());
    frame.render_widget(header, chunks[0]);

    // Content area - render based on active tab
    match state.active_tab {
        ConfigTab::General => super::general::render(frame, chunks[1], &state.general),
        ConfigTab::Sync => {
            super::sync::render(frame, chunks[1], &state.sync, state.sync_status)
        }
        ConfigTab::Security => super::security::render(frame, chunks[1], &state.security),
        ConfigTab::Password => render_password_defaults(frame, chunks[1], &state.password),
        ConfigTab::About => super::about::render(frame, chunks[1], &state.about),
    }

    // Footer
    let footer = Paragraph::new(" \u{2191}\u{2193} 滚动  Tab 切换  Ctrl+S 保存  Esc 关闭 ")
        .style(Style::default().fg(Color::Rgb(86, 95, 137)));
    frame.render_widget(footer, chunks[2]);
}

fn render_password_defaults(frame: &mut Frame, area: Rect, form: &PasswordDefaultsForm) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Title
            Constraint::Length(1), // Length
            Constraint::Length(1), // Digits
            Constraint::Length(1), // Uppercase
            Constraint::Length(1), // Special
        ])
        .split(area);

    let title =
        Paragraph::new("密码默认设置").style(Style::default().fg(Color::Rgb(86, 95, 137)).bold());
    frame.render_widget(title, chunks[0]);

    let length = format!("默认密码长度      [ {} ]", form.length);
    frame.render_widget(
        Paragraph::new(length).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[1],
    );

    let digits = format!(
        "包含数字          [ {} ]",
        if form.include_digits {
            "\u{2713}"
        } else {
            "\u{2717}"
        }
    );
    frame.render_widget(
        Paragraph::new(digits).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[2],
    );

    let upper = format!(
        "包含大写字母      [ {} ]",
        if form.include_uppercase {
            "\u{2713}"
        } else {
            "\u{2717}"
        }
    );
    frame.render_widget(
        Paragraph::new(upper).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[3],
    );

    let special = format!(
        "包含特殊字符      [ {} ]",
        if form.include_special {
            "\u{2713}"
        } else {
            "\u{2717}"
        }
    );
    frame.render_widget(
        Paragraph::new(special).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[4],
    );
}
