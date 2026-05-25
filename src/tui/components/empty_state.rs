//! Empty state display widget with 7 variants per U11 spec.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::t;
use crate::tui::theme;

/// Matches U11 spec EmptyStateVariant.
#[derive(Debug, Clone)]
pub enum EmptyStateVariant {
    NoPasswords,
    EmptyTrash,
    NoFavorites,
    NoExpired,
    NoHealthIssues,
    EmptyTag { tag_name: String },
    NoSearchResults { query: String },
}

impl EmptyStateVariant {
    fn icon(&self, unicode: bool) -> &'static str {
        match self {
            Self::NoPasswords => {
                if unicode {
                    theme::ICON_LOCK
                } else {
                    theme::ascii::ICON_LOCK
                }
            }
            Self::EmptyTrash => {
                if unicode {
                    theme::ICON_TRASH
                } else {
                    theme::ascii::ICON_TRASH
                }
            }
            Self::NoFavorites => {
                if unicode {
                    theme::ICON_STAR
                } else {
                    theme::ascii::ICON_STAR
                }
            }
            Self::NoExpired => {
                if unicode {
                    theme::ICON_SUCCESS
                } else {
                    theme::ascii::ICON_SUCCESS
                }
            }
            Self::NoHealthIssues => {
                if unicode {
                    theme::ICON_SUCCESS
                } else {
                    theme::ascii::ICON_SUCCESS
                }
            }
            Self::EmptyTag { .. } => {
                if unicode {
                    theme::ICON_FOLDER
                } else {
                    theme::ascii::ICON_FOLDER
                }
            }
            Self::NoSearchResults { .. } => {
                if unicode {
                    theme::ICON_SEARCH
                } else {
                    theme::ascii::ICON_SEARCH
                }
            }
        }
    }

    fn title(&self) -> String {
        match self {
            Self::NoPasswords => t!("tui.empty_state.no_passwords").to_string(),
            Self::EmptyTrash => t!("tui.empty_state.empty_trash").to_string(),
            Self::NoFavorites => t!("tui.empty_state.no_favorites").to_string(),
            Self::NoExpired => t!("tui.empty_state.no_expired").to_string(),
            Self::NoHealthIssues => t!("tui.empty_state.no_health_issues").to_string(),
            Self::EmptyTag { .. } => t!("tui.empty_state.empty_tag").to_string(),
            Self::NoSearchResults { .. } => t!("tui.empty_state.no_search_results").to_string(),
        }
    }

    fn description(&self) -> String {
        match self {
            Self::NoPasswords => t!("tui.empty_state.no_passwords_hint").to_string(),
            Self::EmptyTrash => t!("tui.empty_state.empty_trash_hint").to_string(),
            Self::NoFavorites => t!("tui.empty_state.no_favorites_hint").to_string(),
            Self::NoExpired => t!("tui.empty_state.no_expired_hint").to_string(),
            Self::NoHealthIssues => t!("tui.empty_state.no_health_issues_hint").to_string(),
            Self::EmptyTag { tag_name } => {
                t!("tui.empty_state.empty_tag_hint", tag = tag_name).to_string()
            }
            Self::NoSearchResults { query } => {
                t!("tui.empty_state.no_search_results_hint", query = query).to_string()
            }
        }
    }
}

pub struct EmptyStateWidget;

impl EmptyStateWidget {
    pub fn view(frame: &mut Frame, area: Rect, variant: &EmptyStateVariant, unicode: bool) {
        let icon = variant.icon(unicode);
        let content = vec![
            Line::from(Span::styled(
                format!("  {}  ", icon),
                Style::default()
                    .fg(theme::TEXT_MUTED)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                variant.title(),
                Style::default().fg(theme::TEXT_SECONDARY),
            )),
            Line::from(""),
            Line::from(Span::styled(
                variant.description(),
                Style::default().fg(theme::TEXT_MUTED),
            )),
        ];
        let top_pad = area.height.saturating_sub(content.len() as u16) / 2;
        let mut lines: Vec<Line> = (0..top_pad).map(|_| Line::from("")).collect();
        lines.extend(content);
        let para = Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        frame.render_widget(para, area);
    }
}
