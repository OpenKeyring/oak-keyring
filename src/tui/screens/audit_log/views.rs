use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::t;
use crate::tui::state::audit_state::AuditFocus;
use crate::tui::state::audit_state::AuditOperationFilter;
use crate::tui::theme::{self, Styles, BG_BAR, PRIMARY, TEXT, TEXT_MUTED, TEXT_SECONDARY};

use super::filter::{
    format_timestamp, operation_color, operation_display_name, time_range_display, TIME_RANGES,
};
use super::screen::AuditLogScreen;

impl AuditLogScreen {
    pub(super) fn render_title_bar(&self, frame: &mut Frame, area: Rect) {
        let title_text = if self.state.audit_enabled {
            t!("tui.audit.title").to_string()
        } else {
            format!(" {} {}", t!("tui.audit.title"), t!("tui.audit.disabled"))
        };

        let count_text = t!("tui.audit.record_count", n = self.state.total_count).to_string();

        let line = Line::from(vec![
            Span::styled(
                title_text,
                Style::default()
                    .bg(BG_BAR)
                    .fg(TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:>width$}", count_text, width = area.width as usize - 24),
                Style::default().bg(BG_BAR).fg(TEXT_SECONDARY),
            ),
        ]);

        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(BG_BAR)),
            area,
        );
    }

    pub(super) fn render_filter_bar(&self, frame: &mut Frame, area: Rect) {
        // Split into 3 filter sections: operation | time | search
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(14), // operation filter
                Constraint::Length(14), // time filter
                Constraint::Fill(1),    // search input
            ])
            .split(area);

        // Operation filter
        let op_filter = AuditOperationFilter::all_variants()[self.operation_filter_idx];
        let op_name = op_filter.display_name();
        let op_border = if self.state.focused_area == AuditFocus::OperationFilter {
            Styles::focused_border()
        } else {
            Styles::unfocused_border()
        };
        let op_block = Block::default()
            .borders(Borders::ALL)
            .border_style(op_border)
            .title(t!("tui.audit.filter_type").to_string());
        let op_text = Paragraph::new(format!(" {}", op_name)).style(Style::default().fg(TEXT));
        let op_inner = op_block.inner(columns[0]);
        frame.render_widget(op_block, columns[0]);
        let inner = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1)])
            .split(op_inner);
        let padded = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(1), Constraint::Fill(1)])
            .split(inner[0]);
        frame.render_widget(op_text, padded[1]);

        // Time filter
        let time_range = TIME_RANGES[self.time_filter_idx];
        let time_name = time_range_display(&time_range);
        let time_border = if self.state.focused_area == AuditFocus::TimeFilter {
            Styles::focused_border()
        } else {
            Styles::unfocused_border()
        };
        let time_block = Block::default()
            .borders(Borders::ALL)
            .border_style(time_border)
            .title(t!("tui.audit.filter_time").to_string());
        let time_text = Paragraph::new(format!(" {}", time_name)).style(Style::default().fg(TEXT));
        let time_inner = time_block.inner(columns[1]);
        frame.render_widget(time_block, columns[1]);
        let inner = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1)])
            .split(time_inner);
        let padded = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(1), Constraint::Fill(1)])
            .split(inner[0]);
        frame.render_widget(time_text, padded[1]);

        // Search input
        let search_border = if self.state.focused_area == AuditFocus::SearchInput {
            Styles::focused_border()
        } else {
            Styles::unfocused_border()
        };
        let search_block = Block::default()
            .borders(Borders::ALL)
            .border_style(search_border)
            .title(t!("tui.audit.filter_search").to_string());
        let search_inner = search_block.inner(columns[2]);
        let search_display = if self.state.filter.search.is_empty() {
            Paragraph::new(t!("tui.audit.search_placeholder").to_string())
                .style(Style::default().fg(theme::TEXT_PLACEHOLDER))
        } else {
            let cursor = if self.state.focused_area == AuditFocus::SearchInput {
                theme::ICON_PIPE
            } else {
                ""
            };
            Paragraph::new(format!(" {}{}", self.state.filter.search, cursor))
                .style(Style::default().fg(TEXT))
        };
        frame.render_widget(search_block, columns[2]);
        let inner = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1)])
            .split(search_inner);
        let padded = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(1), Constraint::Fill(1)])
            .split(inner[0]);
        frame.render_widget(search_display, padded[1]);
    }

    pub(super) fn render_log_list(&self, frame: &mut Frame, area: Rect) {
        let filtered = self.filtered_entries();
        let visible_rows = area.height.max(1) as usize;
        self.visible_log_rows.set(visible_rows);

        if filtered.is_empty() {
            self.render_empty_state(frame, area);
            return;
        }

        // Clamp selected_index to valid range of filtered entries
        let selected = self.state.selected_index.min(filtered.len() - 1);
        let max_offset = filtered.len().saturating_sub(visible_rows);
        let offset = self.state.scroll_offset.min(max_offset);
        let scrollbar_total = self.state.total_count.max(filtered.len());
        let needs_scrollbar = scrollbar_total > visible_rows;
        let (list_area, scrollbar_area) = if needs_scrollbar && area.width > 1 {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Fill(1), Constraint::Length(1)])
                .split(area);
            (chunks[0], chunks[1])
        } else {
            (area, Rect::default())
        };

        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .skip(offset)
            .take(visible_rows)
            .map(|(i, entry)| {
                let is_selected = i == selected;
                let op_name = operation_display_name(&entry.operation);
                let op_color = operation_color(&entry.operation);
                let timestamp = format_timestamp(&entry.occurred_at);
                let no_record_name = t!("tui.audit.no_record_name").to_string();
                let record_name = entry.record_name.as_deref().unwrap_or(&no_record_name);

                let mut spans = vec![
                    Span::styled(
                        format!(" {} ", timestamp),
                        Style::default().fg(TEXT_SECONDARY),
                    ),
                    Span::styled(format!("[{:6}]", op_name), Style::default().fg(op_color)),
                    Span::styled(
                        format!(" {}", record_name),
                        if is_selected {
                            Style::default().fg(TEXT).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(TEXT)
                        },
                    ),
                ];

                // Show detail if present
                if let Some(ref detail) = entry.detail {
                    spans.push(Span::styled(
                        format!(" - {}", detail),
                        Style::default().fg(TEXT_MUTED),
                    ));
                }

                let line = Line::from(spans);
                let style = if is_selected && self.state.focused_area == AuditFocus::LogList {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else if is_selected {
                    Style::default().fg(PRIMARY)
                } else {
                    Style::default()
                };

                ListItem::new(line).style(style)
            })
            .collect();

        let block = Block::default()
            .borders(Borders::NONE)
            .style(Style::default().bg(theme::BG));

        let list = List::new(items).block(block);

        // Use ListState for scrolling
        let mut list_state = ListState::default();
        let selected_in_view = selected
            .checked_sub(offset)
            .filter(|idx| *idx < visible_rows);
        list_state.select(selected_in_view);

        frame.render_stateful_widget(list, list_area, &mut list_state);
        self.render_log_scrollbar(frame, scrollbar_area, offset, visible_rows, scrollbar_total);
    }

    fn render_log_scrollbar(
        &self,
        frame: &mut Frame,
        area: Rect,
        offset: usize,
        visible_rows: usize,
        total_rows: usize,
    ) {
        if area.width == 0 || area.height == 0 || total_rows <= visible_rows {
            return;
        }

        let max_offset = total_rows.saturating_sub(visible_rows);
        if max_offset == 0 {
            return;
        }

        let thumb_ratio = visible_rows as f32 / total_rows as f32;
        let thumb_height = ((area.height as f32 * thumb_ratio).max(1.0)).ceil() as u16;
        let scroll_ratio = offset.min(max_offset) as f32 / max_offset as f32;
        let max_thumb_y = area.height.saturating_sub(thumb_height);
        let thumb_y = (scroll_ratio * max_thumb_y as f32) as u16;

        frame.render_widget(
            Paragraph::new("\u{2502}".repeat(area.height as usize))
                .style(Style::default().fg(theme::NL_LINE).bg(theme::BG)),
            area,
        );

        let thumb_area = Rect {
            x: area.x,
            y: area.y + thumb_y,
            width: area.width,
            height: thumb_height.max(1),
        };
        frame.render_widget(
            Paragraph::new("\u{2588}".repeat(thumb_area.height as usize))
                .style(Style::default().fg(theme::NL_CYAN).bg(theme::BG)),
            thumb_area,
        );
    }

    fn render_empty_state(&self, frame: &mut Frame, area: Rect) {
        let (icon, message) = if !self.state.audit_enabled {
            (theme::ICON_WARNING, t!("tui.audit.disabled_message"))
        } else if !self.state.filter.search.is_empty() {
            (theme::ICON_INFO, t!("tui.audit.no_matches"))
        } else if self.state.entries.is_empty() {
            (theme::ICON_INFO, t!("tui.audit.empty_log"))
        } else {
            // There are entries but the operation filter excluded them all
            (theme::ICON_INFO, t!("tui.audit.no_records_for_filter"))
        };

        let line = Line::from(vec![
            Span::styled(format!(" {} ", icon), Style::default().fg(theme::WARNING)),
            Span::styled(message, Style::default().fg(TEXT_MUTED)),
        ]);

        let paragraph = Paragraph::new(line)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        // Vertically center in the available area
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(1),
                Constraint::Fill(1),
            ])
            .split(area);

        frame.render_widget(paragraph, outer[1]);
    }

    pub(super) fn render_help_bar(&self, frame: &mut Frame, area: Rect) {
        let help_select = t!("tui.audit_log_view.help_select").to_string();
        let help_switch_area = t!("tui.audit_log_view.help_switch_area").to_string();
        let help_view_record = t!("tui.audit_log_view.help_view_record").to_string();
        let help_back = t!("tui.audit_log_view.help_back").to_string();

        let hints = [
            "\u{2191}\u{2193}/j/k",
            help_select.as_str(),
            "Tab",
            help_switch_area.as_str(),
            "Enter",
            help_view_record.as_str(),
            "Esc",
            help_back.as_str(),
        ];

        let hint_text = hints.chunks(2).fold(String::new(), |mut acc, pair| {
            if !acc.is_empty() {
                acc.push_str(&format!("  {}  ", theme::ICON_PIPE));
            }
            acc.push_str(&format!("{} {}", pair[0], pair[1]));
            acc
        });

        // Also show the hint message if any
        let line = if let Some(ref msg) = self.state.hint_message {
            Line::from(vec![
                Span::styled(format!(" {} ", hint_text), Style::default().fg(TEXT_MUTED)),
                Span::styled(
                    format!("  {} {}", theme::ICON_INFO, msg),
                    Style::default().fg(theme::INFO),
                ),
            ])
        } else {
            Line::from(Span::styled(
                format!(" {} ", hint_text),
                Style::default().fg(TEXT_MUTED),
            ))
        };

        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(BG_BAR)),
            area,
        );
    }
}
