use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::state::config_state::{ConfigTab, ConfigScreenState, PasswordDefaultsForm};
use crate::tui::theme;

pub fn render(frame: &mut Frame, area: Rect, state: &ConfigScreenState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header with tabs
            Constraint::Min(0),    // Content
            Constraint::Length(1), // Footer
        ])
        .split(area);

    // Tab bar with active tab highlight
    let tab_names = ["常规", "同步", "安全", "密码", "关于"];

    // Build header with active tab indicator using Spans
    use ratatui::text::{Line, Span};

    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(
        " 配置  ",
        Style::default().fg(theme::TEXT).bold(),
    ));

    for (i, tab) in ConfigTab::all().iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(
                "\u{2502}",
                Style::default().fg(theme::TEXT_SECONDARY),
            ));
        }
        let name = tab_names[i];
        if *tab == state.active_tab {
            spans.push(Span::styled(
                format!(" {} ", name),
                Style::default()
                    .fg(theme::PRIMARY)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ));
        } else {
            spans.push(Span::styled(
                format!(" {} ", name),
                Style::default().fg(theme::TEXT_SECONDARY),
            ));
        }
    }

    let header = Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::BG_BAR));
    frame.render_widget(header, chunks[0]);

    // Content area - render based on active tab with focused item
    let focused = state.active_tab.clamp_item(state.focused_item);

    match state.active_tab {
        ConfigTab::General => super::general::render(frame, chunks[1], &state.general, focused),
        ConfigTab::Sync => {
            super::sync::render(frame, chunks[1], &state.sync, state.sync_status, focused)
        }
        ConfigTab::Security => super::security::render(frame, chunks[1], &state.security, focused),
        ConfigTab::Password => render_password_defaults(frame, chunks[1], &state.password, focused),
        ConfigTab::About => super::about::render(frame, chunks[1], &state.about),
    }

    // Footer
    let footer = Paragraph::new(" \u{2191}\u{2193} 滚动  Tab 切换  Ctrl+S 保存  Esc 关闭 ")
        .style(Style::default().fg(theme::TEXT_SECONDARY));
    frame.render_widget(footer, chunks[2]);
}

fn render_password_defaults(
    frame: &mut Frame,
    area: Rect,
    form: &PasswordDefaultsForm,
    focused: usize,
) {
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

    let dim_style = Style::default().fg(theme::TEXT_SECONDARY).bold();
    let normal_style = Style::default().fg(theme::TEXT);
    let focused_style = Style::default()
        .fg(theme::TEXT)
        .add_modifier(Modifier::BOLD)
        .bg(theme::BG_SURFACE);
    let title = Paragraph::new("密码默认设置").style(dim_style);
    frame.render_widget(title, chunks[0]);

    // Row index 0: Length (focused == 0)
    let length = format!("默认密码长度      [ {} ]", form.length);
    frame.render_widget(
        Paragraph::new(length).style(if focused == 0 {
            focused_style
        } else {
            normal_style
        }),
        chunks[1],
    );

    // Row index 1: Digits (focused == 1)
    let digits = format!(
        "包含数字          [ {} ]",
        if form.include_digits {
            "\u{2713}"
        } else {
            "\u{2717}"
        }
    );
    frame.render_widget(
        Paragraph::new(digits).style(if focused == 1 {
            focused_style
        } else {
            normal_style
        }),
        chunks[2],
    );

    // Row index 2: Uppercase (focused == 2)
    let upper = format!(
        "包含大写字母      [ {} ]",
        if form.include_uppercase {
            "\u{2713}"
        } else {
            "\u{2717}"
        }
    );
    frame.render_widget(
        Paragraph::new(upper).style(if focused == 2 {
            focused_style
        } else {
            normal_style
        }),
        chunks[3],
    );

    // Row index 3: Special (focused == 3)
    let special = format!(
        "包含特殊字符      [ {} ]",
        if form.include_special {
            "\u{2713}"
        } else {
            "\u{2717}"
        }
    );
    frame.render_widget(
        Paragraph::new(special).style(if focused == 3 {
            focused_style
        } else {
            normal_style
        }),
        chunks[4],
    );
}
