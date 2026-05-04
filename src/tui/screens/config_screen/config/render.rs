use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::t;
use crate::tui::state::config_state::{ConfigScreenState, ConfigTab, PasswordDefaultsForm};
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
    let tab_names = [
        t!("tui.config.tab_general").to_string(),
        t!("tui.config.tab_sync").to_string(),
        t!("tui.config.tab_security").to_string(),
        t!("tui.config.tab_password").to_string(),
        t!("tui.config.tab_about").to_string(),
    ];

    // Build header with active tab indicator using Spans
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(
        format!(" {}  ", t!("tui.config.title")),
        Style::default().fg(theme::TEXT).bold(),
    ));

    for (i, tab) in ConfigTab::all().iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(
                "\u{2502}",
                Style::default().fg(theme::TEXT_SECONDARY),
            ));
        }
        let name = &tab_names[i];
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

    // Restart hint line + remaining content
    let content_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Restart hint
            Constraint::Min(0),    // Remaining content
        ])
        .split(chunks[1]);

    let hint = Paragraph::new(Line::from(vec![
        Span::styled(
            " \u{2139} ",
            Style::default().fg(ratatui::style::Color::Yellow),
        ),
        Span::styled(
            t!("tui.config_screen.restart_hint").to_string(),
            Style::default().fg(ratatui::style::Color::Yellow),
        ),
    ]));
    frame.render_widget(hint, content_layout[0]);

    // Content area - split into content + scrollbar track
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),    // Content
            Constraint::Length(1), // Scrollbar track
        ])
        .split(content_layout[1]);

    let content_area = content_chunks[0];
    let scrollbar_area = content_chunks[1];

    // Content area - render based on active tab with focused item
    let focused = state.active_tab.clamp_item(state.focused_item);

    match state.active_tab {
        ConfigTab::General => super::general::render(frame, content_area, &state.general, focused),
        ConfigTab::Sync => super::sync::render(
            frame,
            content_area,
            &state.sync,
            state.sync_status,
            state.gdrive_auth_status.clone(),
            state.last_sync,
            focused,
        ),
        ConfigTab::Security => super::security::render(
            frame,
            content_area,
            &state.security,
            focused,
            state.sub_item_focus,
        ),
        ConfigTab::Password => render_password_defaults(
            frame,
            content_area,
            &state.password,
            focused,
            state.editing_length,
        ),
        ConfigTab::About => super::about::render(frame, content_area, &state.about),
    }

    // Scrollbar
    let visible_height = content_area.height;
    let total_items = state.active_tab.item_count() as u16 + 1; // +1 for title row
    render_scrollbar(
        frame,
        scrollbar_area,
        state.scroll_offset,
        visible_height,
        total_items,
    );

    // Scroll boundary flash: overlay orange on the focused item row
    if state.is_boundary_flash_active() && state.active_tab.item_count() > 0 {
        // Each tab layout: row 0 = title, rows 1..N = items. Focused item row = focused + 1.
        let focused_row = focused as u16 + 1;
        let visible_row = focused_row.saturating_sub(state.scroll_offset);
        if visible_row < visible_height {
            let flash_area = Rect {
                x: content_area.x,
                y: content_area.y + visible_row,
                width: content_area.width,
                height: 1,
            };
            let flash = Paragraph::new("").style(
                Style::default()
                    .bg(theme::WARNING)
                    .add_modifier(Modifier::BOLD),
            );
            frame.render_widget(flash, flash_area);
        }
    }

    // Footer bar: keyboard hints + [Exit Program] red + [Close] blue
    let exit_label = t!("tui.config.exit_program");
    let close_label = t!("tui.config.close");

    let exit_style = match state.footer_focus {
        Some(crate::tui::state::config_state::FooterButton::ExitProgram) => Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED),
        _ => Style::default()
            .fg(theme::ERROR)
            .add_modifier(Modifier::BOLD),
    };

    let close_style = match state.footer_focus {
        Some(crate::tui::state::config_state::FooterButton::Close) => Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED),
        _ => Style::default().fg(theme::PRIMARY),
    };

    let footer = Line::from(vec![
        Span::styled(
            " \u{2191}\u{2193} scroll  Tab switch  Enter confirm  Esc close  q exit ",
            Style::default().fg(theme::TEXT_SECONDARY),
        ),
        Span::raw("  "),
        Span::styled(format!("[ {} ]", exit_label), exit_style),
        Span::raw(" "),
        Span::styled(format!("[ {} ]", close_label), close_style),
    ]);
    let footer_widget = Paragraph::new(footer).style(Style::default().bg(theme::BG_BAR));
    frame.render_widget(footer_widget, chunks[2]);
}

fn render_password_defaults(
    frame: &mut Frame,
    area: Rect,
    form: &PasswordDefaultsForm,
    focused: usize,
    editing_length: bool,
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
    let title = Paragraph::new(t!("tui.config.password_defaults").to_string()).style(dim_style);
    frame.render_widget(title, chunks[0]);

    // Row index 0: Length (focused == 0)
    if editing_length && focused == 0 {
        let slider_line = crate::tui::components::length_slider::render_length_slider(
            &t!("tui.config.default_length"),
            form.length,
            8,
            128,
            true,
        );
        frame.render_widget(Paragraph::new(slider_line), chunks[1]);
    } else {
        let length = format!(
            "{}      [ {} ]",
            t!("tui.config.default_length"),
            form.length
        );
        frame.render_widget(
            Paragraph::new(length).style(if focused == 0 {
                focused_style
            } else {
                normal_style
            }),
            chunks[1],
        );
    }

    // Row index 1: Digits (focused == 1)
    let digits = format!(
        "{}          [ {} ]",
        t!("tui.config.default_digits"),
        if form.include_digits {
            theme::ICON_SUCCESS
        } else {
            theme::ICON_ERROR
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
        "{}      [ {} ]",
        t!("tui.config.default_uppercase"),
        if form.include_uppercase {
            theme::ICON_SUCCESS
        } else {
            theme::ICON_ERROR
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
        "{}      [ {} ]",
        t!("tui.config.default_symbols"),
        if form.include_special {
            theme::ICON_SUCCESS
        } else {
            theme::ICON_ERROR
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

/// Render a simple scrollbar track + thumb in the given area.
fn render_scrollbar(
    frame: &mut Frame,
    area: Rect,
    scroll_offset: u16,
    visible_height: u16,
    total_height: u16,
) {
    if area.width == 0 || area.height == 0 || total_height <= visible_height {
        return;
    }

    let max_offset = total_height.saturating_sub(visible_height);
    if max_offset == 0 {
        return;
    }

    // Calculate thumb size proportional to visible/total ratio
    let thumb_ratio = visible_height as f32 / total_height as f32;
    let thumb_height = ((visible_height as f32 * thumb_ratio).max(1.0)) as u16;

    // Calculate thumb position based on scroll offset
    let scroll_ratio = scroll_offset as f32 / max_offset as f32;
    let max_thumb_y = visible_height.saturating_sub(thumb_height);
    let thumb_y = area.y + (scroll_ratio * max_thumb_y as f32) as u16;

    // Render track (full height, dim border color)
    let track = Block::default().style(Style::default().fg(theme::BORDER));
    frame.render_widget(track, area);

    // Render thumb (positioned, brighter secondary text color)
    let thumb_area = Rect {
        x: area.x,
        y: thumb_y,
        width: area.width,
        height: thumb_height.max(1),
    };
    let thumb = Block::default().style(Style::default().fg(theme::TEXT_SECONDARY));
    frame.render_widget(thumb, thumb_area);
}
