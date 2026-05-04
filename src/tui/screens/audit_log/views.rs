use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

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
            " 操作审计".to_string()
        } else {
            format!(" 操作审计 {} 审计已关闭", theme::ICON_WARNING)
        };

        let count_text = format!("{} 条记录", self.state.total_count);

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
            .title(" 类型 ");
        let op_text = Paragraph::new(format!(" {}", op_name)).style(Style::default().fg(TEXT));
        frame.render_widget(op_block, columns[0]);
        let inner = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1)])
            .split(columns[0]);
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
            .title(" 时间 ");
        let time_text = Paragraph::new(format!(" {}", time_name)).style(Style::default().fg(TEXT));
        frame.render_widget(time_block, columns[1]);
        let inner = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1)])
            .split(columns[1]);
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
            .title(" 搜索 ");
        let search_display = if self.state.filter.search.is_empty() {
            Paragraph::new(" 输入关键词...").style(Style::default().fg(theme::TEXT_PLACEHOLDER))
        } else {
            let cursor = if self.state.focused_area == AuditFocus::SearchInput {
                &theme::ICON_PIPE[..] // vertical bar cursor
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
            .split(columns[2]);
        let padded = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(1), Constraint::Fill(1)])
            .split(inner[0]);
        frame.render_widget(search_display, padded[1]);
    }

    pub(super) fn render_log_list(&self, frame: &mut Frame, area: Rect) {
        let filtered = self.filtered_entries();

        if filtered.is_empty() {
            self.render_empty_state(frame, area);
            return;
        }

        // Clamp selected_index to valid range of filtered entries
        let selected = self.state.selected_index.min(filtered.len() - 1);

        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let is_selected = i == selected;
                let op_name = operation_display_name(&entry.operation);
                let op_color = operation_color(&entry.operation);
                let timestamp = format_timestamp(&entry.occurred_at);
                let record_name = entry.record_name.as_deref().unwrap_or("(无记录名)");

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
        list_state.select(Some(selected));

        frame.render_stateful_widget(list, area, &mut list_state);
    }

    fn render_empty_state(&self, frame: &mut Frame, area: Rect) {
        let (icon, message) = if !self.state.audit_enabled {
            (theme::ICON_WARNING, "审计日志功能已关闭")
        } else if !self.state.filter.search.is_empty() {
            (theme::ICON_INFO, "未找到匹配的记录")
        } else if self.state.entries.is_empty() {
            (theme::ICON_INFO, "暂无审计记录")
        } else {
            // There are entries but the operation filter excluded them all
            (theme::ICON_INFO, "当前筛选条件下无记录")
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
        let hints = [
            "\u{2191}\u{2193}/j/k",
            "选择",
            "Tab",
            "切换区域",
            "Enter",
            "查看记录",
            "Esc",
            "返回",
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
