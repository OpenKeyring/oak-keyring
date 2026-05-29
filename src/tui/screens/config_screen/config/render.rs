use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::t;
use crate::tui::state::config_state::{ConfigScreenState, ConfigTab, PasswordDefaultsForm};
use crate::tui::theme;

const CONTENT_X_PADDING: u16 = 2;
const CONTENT_Y_PADDING: u16 = 1;
const ROW_ICON_WIDTH: usize = 8;
const CONTROL_WIDTH: usize = 24;
const FOOTER_CLOSE_MIN_WIDTH: usize = 14;

pub fn render(frame: &mut Frame, area: Rect, state: &ConfigScreenState) {
    frame.render_widget(Paragraph::new("").style(theme::Styles::newlook_bg()), area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab bar
            Constraint::Length(3), // Restart hint
            Constraint::Min(0),    // Content panel
            Constraint::Length(3), // Footer
        ])
        .split(area);

    render_tab_bar(frame, chunks[0], state.active_tab);
    render_restart_hint(frame, chunks[1]);

    let content_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::Styles::newlook_focused_border())
        .style(theme::Styles::newlook_bg());
    let content_inner = content_block.inner(chunks[2]);
    frame.render_widget(content_block, chunks[2]);

    let content_inner = padded_rect(content_inner, CONTENT_X_PADDING, CONTENT_Y_PADDING);
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(content_inner);
    let content_area = content_chunks[0];
    let scrollbar_area = content_chunks[1];
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
            state.sync_error_message.as_deref(),
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

    render_scrollbar(
        frame,
        scrollbar_area,
        state.scroll_offset,
        content_area.height,
        state.active_tab.item_count() as u16 + 3,
    );

    if state.is_boundary_flash_active() && state.active_tab.item_count() > 0 {
        let focused_row = focused as u16 + 3;
        let visible_row = focused_row.saturating_sub(state.scroll_offset);
        if visible_row < content_area.height {
            let flash_area = Rect {
                x: content_area.x,
                y: content_area.y + visible_row,
                width: content_area.width,
                height: 1,
            };
            frame.render_widget(
                Paragraph::new("").style(Style::default().bg(theme::NL_HOT)),
                flash_area,
            );
        }
    }

    render_footer(frame, chunks[3], state.footer_focus.is_some());
}

fn render_tab_bar(frame: &mut Frame, area: Rect, active: ConfigTab) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::Styles::newlook_border())
        .style(theme::Styles::newlook_surface());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let mut spans = Vec::new();
    spans.push(Span::styled(
        format!("  {}  {}  ", theme::NF_GEAR, t!("tui.config.title")),
        Style::default()
            .fg(theme::NL_TEXT_MUTED)
            .bg(theme::NL_SURFACE)
            .add_modifier(Modifier::BOLD),
    ));

    for tab in ConfigTab::all() {
        let name = tab_name(*tab);
        let style = if *tab == active {
            Style::default()
                .fg(theme::NL_CYAN)
                .bg(theme::NL_SURFACE)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default()
                .fg(theme::NL_TEXT_MUTED)
                .bg(theme::NL_SURFACE)
                .add_modifier(Modifier::BOLD)
        };
        spans.push(Span::styled(format!("  {}  ", name), style));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::Styles::newlook_surface()),
        inner,
    );
}

fn render_restart_hint(frame: &mut Frame, area: Rect) {
    let inner_area = padded_rect(area, 2, 0);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::NL_HOT).bg(theme::NL_BG))
        .style(theme::Styles::newlook_bg());
    let inner = block.inner(inner_area);
    frame.render_widget(block, inner_area);
    if inner.height == 0 {
        return;
    }
    let line = Line::from(vec![
        Span::styled(
            format!(" {}  ", theme::NF_WARNING_TRIANGLE),
            Style::default()
                .fg(theme::NL_HOT)
                .bg(theme::NL_BG)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            t!("tui.config_screen.restart_hint").to_string(),
            Style::default()
                .fg(theme::NL_HOT)
                .bg(theme::NL_BG)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), inner);
}

fn render_footer(frame: &mut Frame, area: Rect, close_focused: bool) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::Styles::newlook_border())
        .style(theme::Styles::newlook_surface_2());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let close_label = format!("[ {} ]", t!("tui.config.close"));
    let shortcuts = vec![
        ("↑↓", "scroll"),
        ("Tab", "switch"),
        ("Enter", "confirm"),
        ("Esc", "close"),
    ];
    let mut spans = Vec::new();
    spans.push(Span::raw(" "));
    for (idx, (key, label)) in shortcuts.into_iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(
                " │ ",
                Style::default().fg(theme::NL_LINE).bg(theme::NL_SURFACE_2),
            ));
        }
        spans.push(Span::styled(
            format!("{} ", key),
            Style::default()
                .fg(theme::NL_CYAN)
                .bg(theme::NL_SURFACE_2)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            label.to_string(),
            Style::default()
                .fg(theme::NL_TEXT_MUTED)
                .bg(theme::NL_SURFACE_2),
        ));
        spans.push(Span::raw(" "));
    }

    let left_width: usize = spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum();
    let close_width = FOOTER_CLOSE_MIN_WIDTH.max(display_width(&close_label) + 4);
    let padding = inner
        .width
        .saturating_sub(left_width as u16)
        .saturating_sub(close_width as u16) as usize;
    spans.push(Span::styled(
        " ".repeat(padding),
        Style::default().bg(theme::NL_SURFACE_2),
    ));

    let close_style = if close_focused {
        Style::default()
            .fg(theme::NL_DANGER)
            .bg(theme::NL_SURFACE_2)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::NL_TEXT_MUTED)
            .bg(theme::NL_SURFACE_2)
    };
    let close_padding = close_width.saturating_sub(display_width(&close_label));
    spans.push(Span::styled(
        format!("{}{}", close_label, " ".repeat(close_padding)),
        close_style,
    ));

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::Styles::newlook_surface_2()),
        inner,
    );
}

fn render_password_defaults(
    frame: &mut Frame,
    area: Rect,
    form: &PasswordDefaultsForm,
    focused: usize,
    editing_length: bool,
) {
    let chunks = vertical_chunks(
        area,
        &[
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ],
    );

    render_section_title(
        frame,
        chunks[0],
        t!("tui.config.password_defaults").as_ref(),
    );

    if editing_length && focused == 0 {
        let slider_line = crate::tui::components::length_slider::render_length_slider(
            &t!("tui.config.default_length"),
            form.length,
            8,
            128,
            true,
        );
        frame.render_widget(
            Paragraph::new(slider_line).style(row_style(true)),
            chunks[1],
        );
    } else {
        render_setting_row(
            frame,
            chunks[1],
            theme::NF_SLIDERS,
            t!("tui.config.default_length").as_ref(),
            &plain_control(&form.length.to_string()),
            focused == 0,
            true,
        );
    }

    render_setting_row(
        frame,
        chunks[2],
        theme::NF_KEY,
        t!("tui.config.default_digits").as_ref(),
        &switch_control(form.include_digits),
        focused == 1,
        true,
    );
    render_setting_row(
        frame,
        chunks[3],
        theme::NF_USER,
        t!("tui.config.default_uppercase").as_ref(),
        &switch_control(form.include_uppercase),
        focused == 2,
        true,
    );
    render_setting_row(
        frame,
        chunks[4],
        theme::NF_SPARKLES,
        t!("tui.config.default_symbols").as_ref(),
        &switch_control(form.include_special),
        focused == 3,
        true,
    );
}

pub(super) fn render_section_title(frame: &mut Frame, area: Rect, title: &str) {
    let chunks = vertical_chunks(area, &[Constraint::Length(1), Constraint::Length(1)]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "▌",
                Style::default()
                    .fg(theme::NL_CYAN)
                    .bg(theme::NL_BG)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", title),
                Style::default()
                    .fg(theme::NL_CYAN)
                    .bg(theme::NL_BG)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        chunks[0],
    );

    let line = dotted_line(chunks[1].width as usize);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().fg(theme::NL_LINE).bg(theme::NL_BG)),
        chunks[1],
    );
}

pub(super) fn render_setting_row(
    frame: &mut Frame,
    area: Rect,
    icon: &str,
    label: &str,
    control: &str,
    focused: bool,
    enabled: bool,
) {
    let style = row_style(focused);
    let muted = if enabled {
        theme::NL_TEXT_MUTED
    } else {
        theme::TEXT_MUTED
    };
    let fg = if enabled {
        theme::NL_TEXT
    } else {
        theme::TEXT_MUTED
    };
    let bg = style.bg.unwrap_or(theme::NL_BG);
    let leading = format!("  {:<width$}", icon, width = ROW_ICON_WIDTH);
    let base_width = display_width(&leading) + display_width(label) + 2;
    let desired_control_width = CONTROL_WIDTH.max(display_width(control));
    let control_width = desired_control_width.min((area.width as usize).saturating_sub(base_width));
    let used = display_width(&leading) + display_width(label) + control_width;
    let padding = (area.width as usize).saturating_sub(used).max(2);

    let line = Line::from(vec![
        Span::styled(
            leading,
            Style::default()
                .fg(if enabled { theme::NL_CYAN } else { muted })
                .bg(bg),
        ),
        Span::styled(label.to_string(), Style::default().fg(fg).bg(bg)),
        Span::styled(" ".repeat(padding), Style::default().bg(bg)),
        Span::styled(
            fixed_width(control, control_width),
            Style::default()
                .fg(if enabled { theme::NL_TEXT } else { muted })
                .bg(if focused {
                    theme::NL_SURFACE_2
                } else {
                    theme::NL_BG
                }),
        ),
    ]);
    frame.render_widget(Paragraph::new(line).style(style), area);
}

pub(super) fn render_button_row(
    frame: &mut Frame,
    area: Rect,
    left: (&str, &str, bool),
    right: (&str, &str, bool),
) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(42),
            Constraint::Length(3),
            Constraint::Percentage(32),
            Constraint::Min(0),
        ])
        .split(area);
    render_action_button(frame, chunks[0], left.0, left.1, left.2);
    render_action_button(frame, chunks[2], right.0, right.1, right.2);
}

pub(super) fn render_action_button(
    frame: &mut Frame,
    area: Rect,
    icon: &str,
    label: &str,
    focused: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if area.height < 3 {
        let text = compact_button_text(icon, label, area.width as usize);
        let style = if focused {
            Style::default()
                .fg(theme::NL_TEXT)
                .bg(theme::NL_SELECTED)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::NL_CYAN).bg(theme::NL_BG)
        };
        frame.render_widget(
            Paragraph::new(fixed_width(&text, area.width as usize)).style(style),
            area,
        );
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if focused {
            theme::Styles::newlook_focused_border()
        } else {
            Style::default().fg(theme::NL_CYAN).bg(theme::NL_BG)
        })
        .style(theme::Styles::newlook_bg());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let text = format!("{}  {}", icon, label);
    let left_pad = inner
        .width
        .saturating_sub(display_width(&text) as u16)
        .saturating_div(2) as usize;
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ".repeat(left_pad), theme::Styles::newlook_bg()),
            Span::styled(
                text,
                Style::default()
                    .fg(theme::NL_TEXT)
                    .bg(theme::NL_BG)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        inner,
    );
}

pub(super) fn dropdown_control(value: &str) -> String {
    format!("[  {}  {}  ]", value, theme::ICON_DROPDOWN)
}

pub(super) fn plain_control(value: &str) -> String {
    format!("[  {}  ]", value)
}

pub(super) fn switch_control(enabled: bool) -> String {
    if enabled {
        format!("[  {} {}  ]", theme::ICON_SUCCESS, t!("tui.config.enabled"))
    } else {
        format!("[  {} {}  ]", theme::ICON_ERROR, t!("tui.config.disabled"))
    }
}

pub(super) fn row_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(theme::NL_TEXT)
            .bg(theme::NL_SELECTED)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::NL_TEXT).bg(theme::NL_BG)
    }
}

pub(super) fn vertical_chunks(area: Rect, constraints: &[Constraint]) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints.to_vec())
        .split(area)
        .to_vec()
}

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

    let thumb_ratio = visible_height as f32 / total_height as f32;
    let thumb_height = ((visible_height as f32 * thumb_ratio).max(1.0)) as u16;
    let scroll_ratio = scroll_offset as f32 / max_offset as f32;
    let max_thumb_y = visible_height.saturating_sub(thumb_height);
    let thumb_y = area.y + (scroll_ratio * max_thumb_y as f32) as u16;

    frame.render_widget(
        Paragraph::new("│".repeat(area.height as usize))
            .style(Style::default().fg(theme::NL_LINE).bg(theme::NL_BG)),
        area,
    );

    let thumb_area = Rect {
        x: area.x,
        y: thumb_y,
        width: area.width,
        height: thumb_height.max(1),
    };
    frame.render_widget(
        Paragraph::new("█".repeat(thumb_area.height as usize))
            .style(Style::default().fg(theme::NL_CYAN).bg(theme::NL_BG)),
        thumb_area,
    );
}

fn tab_name(tab: ConfigTab) -> String {
    match tab {
        ConfigTab::General => t!("tui.config.tab_general").to_string(),
        ConfigTab::Sync => t!("tui.config.tab_sync").to_string(),
        ConfigTab::Security => t!("tui.config.tab_security").to_string(),
        ConfigTab::Password => t!("tui.config.tab_password").to_string(),
        ConfigTab::About => t!("tui.config.tab_about").to_string(),
    }
}

fn padded_rect(area: Rect, x: u16, y: u16) -> Rect {
    Rect {
        x: area.x.saturating_add(x),
        y: area.y.saturating_add(y),
        width: area.width.saturating_sub(x.saturating_mul(2)),
        height: area.height.saturating_sub(y.saturating_mul(2)),
    }
}

fn fixed_width(text: &str, width: usize) -> String {
    let text_width = display_width(text);
    if text_width > width {
        if width <= 1 {
            return " ".repeat(width);
        }
        let mut out = String::new();
        for ch in text.chars() {
            let candidate = format!("{}{}…", out, ch);
            if display_width(&candidate) > width {
                break;
            }
            out.push(ch);
        }
        out.push('…');
        let out_width = display_width(&out);
        return format!("{}{}", out, " ".repeat(width.saturating_sub(out_width)));
    }
    if text_width >= width {
        return text.to_string();
    }
    format!("{}{}", text, " ".repeat(width - text_width))
}

fn dotted_line(width: usize) -> String {
    if width == 0 {
        String::new()
    } else {
        "┄".repeat(width)
    }
}

fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

fn compact_button_text(icon: &str, label: &str, width: usize) -> String {
    let prefix = format!("[  {}  ", icon);
    let suffix = "  ]";
    let reserved = display_width(&prefix) + display_width(suffix);
    if width <= reserved {
        return fixed_width(&format!("{}{}", prefix, suffix), width);
    }

    let label_width = width - reserved;
    let label = fixed_width(label, label_width);
    format!("{}{}{}", prefix, label, suffix)
}
