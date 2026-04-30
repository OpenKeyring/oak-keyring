//! Empty state display widget with 7 variants per U11 spec.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

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

    fn title(&self) -> &str {
        match self {
            Self::NoPasswords => "No passwords yet",
            Self::EmptyTrash => "回收站为空",
            Self::NoFavorites => "No favorites",
            Self::NoExpired => "No expired passwords",
            Self::NoHealthIssues => "No security issues",
            Self::EmptyTag { .. } => "No passwords under this tag",
            Self::NoSearchResults { .. } => "No results found",
        }
    }

    fn description(&self) -> String {
        match self {
            Self::NoPasswords => "Press n to create your first password".into(),
            Self::EmptyTrash => "已删除的密码会出现在这里".into(),
            Self::NoFavorites => "Press f in password detail to favorite".into(),
            Self::NoExpired => "All credentials are valid".into(),
            Self::NoHealthIssues => "All passwords passed security check".into(),
            Self::EmptyTag { tag_name } => {
                format!("Create a password and add '{}' tag", tag_name)
            }
            Self::NoSearchResults { query } => {
                format!("No matches for '{}'", query)
            }
        }
    }
}

pub struct EmptyStateWidget;

impl EmptyStateWidget {
    pub fn view(frame: &mut Frame, area: Rect, variant: &EmptyStateVariant, unicode: bool) {
        let icon = variant.icon(unicode);
        let lines = vec![
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
        let para = Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        frame.render_widget(para, area);
    }
}
