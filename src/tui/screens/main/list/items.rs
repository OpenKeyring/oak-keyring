use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;
use unicode_width::UnicodeWidthStr;

use crate::commands::types::HealthIssue;
use crate::t;
use crate::tui::state::list_state::{
    calculate_remaining_days, format_days_since_deletion, format_relative_time, format_type_prefix,
    trash_warning_tier, TrashWarningTier,
};
use crate::tui::terminal::WidthTier;
use crate::tui::theme;
use crate::types::credential::CredentialType;

use super::ListPanel;

const ITEM_LEFT_PADDING: &str = "  ";
const ITEM_RIGHT_MARGIN: usize = 2;

fn credential_type_prefix(cred_type: &CredentialType, unicode: bool) -> String {
    let icon = match (cred_type, unicode) {
        (CredentialType::Login, true) => theme::NF_ACCOUNT_KEY,
        (CredentialType::Api, true) => "API",
        (CredentialType::Ssh, true) => "SSH",
        (CredentialType::SecureNote, true) => theme::NF_FILE_LOCK,
        (CredentialType::Login, false) => theme::ascii::NF_ACCOUNT_KEY,
        (CredentialType::Api, false) => theme::ascii::NF_API,
        (CredentialType::Ssh, false) => theme::ascii::NF_SSH,
        (CredentialType::SecureNote, false) => theme::ascii::NF_FILE_LOCK,
    };
    format!("{icon} ")
}

impl ListPanel {
    /// Highlight matching portions of `text` that match `search_terms` (case-insensitive).
    ///
    /// Each term in `search_terms` is highlighted independently using
    /// `theme::WARNING` + `Modifier::BOLD`. This aligns with the multi-term
    /// AND filter logic in `ListPanelState::apply_search_filter`.
    pub(super) fn highlight_match(text: &str, search_terms: &[String]) -> Vec<Span<'static>> {
        if search_terms.is_empty() {
            return vec![Span::styled(
                text.to_string(),
                Style::default().fg(theme::TEXT),
            )];
        }

        // Character-level matching to handle multi-byte UTF-8 correctly.
        let chars: Vec<char> = text.chars().collect();
        let char_count = chars.len();
        let chars_lower: Vec<char> = text.to_lowercase().chars().collect();

        // If case folding changed char count, we can't safely map positions back.
        if chars.len() != chars_lower.len() {
            return vec![Span::styled(
                text.to_string(),
                Style::default().fg(theme::TEXT),
            )];
        }

        let mut matched = vec![false; char_count];
        for term in search_terms {
            let term_chars: Vec<char> = term.to_lowercase().chars().collect();
            let term_len = term_chars.len();
            if term_len == 0 || term_len > char_count {
                continue;
            }
            let mut start = 0;
            while start + term_len <= char_count {
                if chars_lower[start..start + term_len] == term_chars[..] {
                    for m in &mut matched[start..start + term_len] {
                        *m = true;
                    }
                    start += term_len;
                } else {
                    start += 1;
                }
            }
        }

        // Map char indices to byte offsets for valid string slicing.
        // byte_off[i] = byte position of char i; byte_off[char_count] = text.len()
        let mut byte_off: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        byte_off.push(text.len());

        let mut spans = Vec::new();
        let mut i = 0;
        while i < char_count {
            let start = i;
            let is_match = matched[i];
            while i < char_count && matched[i] == is_match {
                i += 1;
            }
            spans.push(Span::styled(
                text[byte_off[start]..byte_off[i]].to_string(),
                if is_match {
                    Style::default()
                        .fg(theme::WARNING)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::TEXT)
                },
            ));
        }
        spans
    }
}

// ---------------------------------------------------------------------------
// Health badge
// ---------------------------------------------------------------------------

/// Build a styled health badge span for a given `HealthIssue`.
///
/// Priority: Compromised (red) > Weak (orange) > Duplicate (orange) > Expired (blue).
/// Returns `None` when `issue` is `None`.
pub(super) fn health_badge(issue: Option<&HealthIssue>, unicode: bool) -> Option<Span<'static>> {
    issue.map(|i| match i {
        HealthIssue::Compromised => {
            let badge = if unicode { "\u{F06BD}" } else { "[leaked]" };
            Span::styled(
                format!(" {}", badge),
                Style::default()
                    .fg(theme::ERROR)
                    .add_modifier(Modifier::BOLD),
            )
        }
        HealthIssue::Weak => {
            let icon = if unicode {
                theme::ICON_WARNING
            } else {
                theme::ascii::ICON_WARNING
            };
            let label = t!("tui.password_list.health_weak");
            Span::styled(
                format!(" {}{}", icon, label),
                Style::default().fg(theme::WARNING),
            )
        }
        HealthIssue::Duplicate { group_size } => {
            let icon = if unicode {
                theme::ICON_WARNING
            } else {
                theme::ascii::ICON_WARNING
            };
            let label = t!("tui.health.duplicate_label", count = group_size);
            Span::styled(
                format!(" {}{}", icon, label),
                Style::default().fg(theme::WARNING),
            )
        }
        HealthIssue::Expired => {
            let icon = if unicode {
                theme::ICON_ERROR
            } else {
                theme::ascii::ICON_ERROR
            };
            let label = t!("tui.password_list.health_expired");
            Span::styled(
                format!(" {}{}", icon, label),
                Style::default().fg(theme::INFO),
            )
        }
    })
}

// ---------------------------------------------------------------------------
// List rendering
// ---------------------------------------------------------------------------

/// Build a single two-line list item with blank separator.
///
/// Line 1 (title): `  [Type] Name [badge]    timestamp ◀`
/// Line 2 (subtitle): `  subtitle`
/// Line 3 (separator): blank/empty line
pub(super) fn build_record_item<'a>(
    record: &crate::types::record::TuiRecord,
    is_selected: bool,
    is_visual_selected: bool,
    focused: bool,
    unicode: bool,
    area_width: u16,
    search_query: Option<&str>,
) -> ListItem<'a> {
    let is_min_width = WidthTier::from_width(area_width) == WidthTier::Minimum;

    // ── Line 1: Title ──
    let type_prefix = if unicode {
        credential_type_prefix(&record.credential_type, true)
    } else {
        format_type_prefix(&record.credential_type)
    };
    let timestamp = format_relative_time(&record.updated_at);

    let gutter_width = usize::from(is_selected);
    let prefix_str = if is_selected {
        format!(" {}", type_prefix)
    } else {
        format!("{}{}", ITEM_LEFT_PADDING, type_prefix)
    };

    // Priority: Compromised > Weak > Duplicate > Expired (matches S3 spec)
    let badge = if is_min_width {
        None // hide badge at minimum width
    } else if record.is_compromised {
        health_badge(Some(&HealthIssue::Compromised), unicode)
    } else if record.has_weak_password {
        health_badge(Some(&HealthIssue::Weak), unicode)
    } else if let Some(group_size) = record.duplicate_group_size {
        if group_size > 1 {
            health_badge(Some(&HealthIssue::Duplicate { group_size }), unicode)
        } else {
            None
        }
    } else if record.is_expired {
        health_badge(Some(&HealthIssue::Expired), unicode)
    } else {
        None
    };
    let badge_str = badge.as_ref().map(|s| s.content.as_ref()).unwrap_or("");

    let name_width = gutter_width + display_width(&prefix_str) + display_width(&record.name);
    let badge_width = display_width(badge_str);
    let ts_width = display_width(&timestamp);
    let available_after_name = (area_width as usize)
        .saturating_sub(ITEM_RIGHT_MARGIN)
        .saturating_sub(name_width)
        .saturating_sub(badge_width);

    let (right_part, right_width) = if available_after_name >= ts_width {
        (timestamp.clone(), ts_width)
    } else {
        (String::new(), 0)
    };

    let padding_len = (area_width as usize)
        .saturating_sub(ITEM_RIGHT_MARGIN)
        .saturating_sub(name_width)
        .saturating_sub(badge_width)
        .saturating_sub(right_width);

    let base_style = if is_visual_selected {
        Style::default()
            .bg(theme::NL_SELECTED)
            .fg(theme::NL_TEXT)
            .add_modifier(Modifier::DIM)
    } else if is_selected {
        Style::default()
            .bg(theme::NL_SELECTED)
            .fg(theme::NL_TEXT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::NL_TEXT).bg(theme::NL_BG)
    };

    // Determine badge span style (override for selection context)
    let badge_span = badge.map(|span| {
        let badge_fg = span.style.fg.unwrap_or(theme::TEXT);
        if is_visual_selected {
            Span::styled(
                span.content,
                Style::default()
                    .bg(theme::NL_SELECTED)
                    .fg(badge_fg)
                    .add_modifier(Modifier::DIM),
            )
        } else if is_selected {
            Span::styled(
                span.content,
                Style::default().bg(theme::NL_SELECTED).fg(badge_fg),
            )
        } else {
            span
        }
    });

    // Build title spans with optional search highlighting
    let mut title_spans = Vec::new();
    if is_selected {
        let gutter_style = Style::default().bg(if focused {
            theme::NL_CYAN
        } else {
            theme::NL_LINE
        });
        title_spans.push(Span::styled(" ", gutter_style));
    }
    title_spans.push(Span::styled(prefix_str, base_style));
    if let Some(query) = search_query {
        let terms: Vec<String> = query
            .to_lowercase()
            .split_whitespace()
            .map(String::from)
            .collect();
        title_spans.extend(ListPanel::highlight_match(&record.name, &terms));
    } else {
        title_spans.push(Span::styled(record.name.clone(), base_style));
    }
    if let Some(badge_s) = badge_span {
        title_spans.push(badge_s);
    }
    title_spans.push(Span::styled(" ".repeat(padding_len), base_style));
    title_spans.push(Span::styled(right_part, base_style));
    title_spans.push(Span::styled(" ".repeat(ITEM_RIGHT_MARGIN), base_style));

    let title_line = Line::from(title_spans);

    // ── Line 2: Subtitle ──
    let subtitle_style = if is_visual_selected {
        Style::default()
            .bg(theme::NL_SELECTED)
            .fg(theme::NL_TEXT_MUTED)
            .add_modifier(Modifier::DIM)
    } else if is_selected {
        Style::default()
            .bg(theme::NL_SELECTED)
            .fg(theme::NL_TEXT_MUTED)
    } else {
        Style::default().fg(theme::NL_TEXT_MUTED).bg(theme::NL_BG)
    };

    let subtitle_prefix = if is_selected { " " } else { ITEM_LEFT_PADDING };
    let subtitle_line = if let Some(query) = search_query {
        let terms: Vec<String> = query
            .to_lowercase()
            .split_whitespace()
            .map(String::from)
            .collect();
        let mut sub_spans = Vec::new();
        if is_selected {
            let gutter_style = Style::default().bg(if focused {
                theme::NL_CYAN
            } else {
                theme::NL_LINE
            });
            sub_spans.push(Span::styled(" ", gutter_style));
        }
        sub_spans.push(Span::styled(subtitle_prefix, subtitle_style));
        sub_spans.extend(ListPanel::highlight_match(&record.subtitle, &terms));
        Line::from(sub_spans)
    } else {
        let text = format!("{}{}", ITEM_LEFT_PADDING, record.subtitle);
        let selected_text = format!("{}{}", subtitle_prefix, record.subtitle);
        let displayed_text = if is_selected { selected_text } else { text };
        let padding = " ".repeat(
            (area_width as usize)
                .saturating_sub(gutter_width)
                .saturating_sub(display_width(&displayed_text)),
        );
        let mut spans = Vec::new();
        if is_selected {
            let gutter_style = Style::default().bg(if focused {
                theme::NL_CYAN
            } else {
                theme::NL_LINE
            });
            spans.push(Span::styled(" ", gutter_style));
        }
        spans.push(Span::styled(
            format!("{}{}", displayed_text, padding),
            subtitle_style,
        ));
        Line::from(spans)
    };

    // ── Line 3: Separator ──
    let sep_char = if unicode { '\u{2500}' } else { '-' };
    let sep_text: String = std::iter::repeat_n(sep_char, area_width as usize).collect();
    let separator_line = Line::from(Span::styled(
        sep_text,
        Style::default().fg(theme::NL_LINE).bg(theme::NL_BG),
    ));

    if is_min_width {
        ListItem::new(vec![title_line, separator_line])
    } else {
        ListItem::new(vec![title_line, subtitle_line, separator_line])
    }
}

/// Build a trash-specific list item with deletion metadata and progressive warnings.
///
/// Line 1 (title): `  [Type] Name    ◀`
/// Line 2 (metadata): `  X 天前删除  剩余 N 天` with progressive color warnings
/// Line 3 (separator): `─────` or `-----`
pub(super) fn build_trash_item<'a>(
    record: &crate::types::record::TuiRecord,
    is_selected: bool,
    is_visual_selected: bool,
    focused: bool,
    unicode: bool,
    area_width: u16,
    retention_days: u32,
) -> ListItem<'a> {
    let is_min_width = WidthTier::from_width(area_width) == WidthTier::Minimum;

    // ── Line 1: Title with type prefix ──
    let type_prefix = if unicode {
        credential_type_prefix(&record.credential_type, true)
    } else {
        format_type_prefix(&record.credential_type)
    };
    let gutter_width = usize::from(is_selected);
    let prefix_str = if is_selected {
        format!(" {}", type_prefix)
    } else {
        format!("{}{}", ITEM_LEFT_PADDING, type_prefix)
    };

    let name_len = gutter_width + display_width(&prefix_str) + display_width(&record.name);
    let padding_len = (area_width as usize)
        .saturating_sub(ITEM_RIGHT_MARGIN)
        .saturating_sub(name_len);

    let base_style = if is_visual_selected {
        Style::default()
            .bg(theme::NL_SELECTED)
            .fg(theme::NL_TEXT)
            .add_modifier(Modifier::DIM)
    } else if is_selected {
        Style::default()
            .bg(theme::NL_SELECTED)
            .fg(theme::NL_TEXT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::NL_TEXT).bg(theme::NL_BG)
    };

    let mut title_spans = Vec::new();
    if is_selected {
        let gutter_style = Style::default().bg(if focused {
            theme::NL_CYAN
        } else {
            theme::NL_LINE
        });
        title_spans.push(Span::styled(" ", gutter_style));
    }
    title_spans.push(Span::styled(prefix_str, base_style));
    title_spans.push(Span::styled(record.name.clone(), base_style));
    title_spans.push(Span::styled(" ".repeat(padding_len), base_style));
    title_spans.push(Span::styled(" ".repeat(ITEM_RIGHT_MARGIN), base_style));
    let title_line = Line::from(title_spans);

    // ── Line 2: Deletion metadata with progressive warnings ──
    let deleted_at = match record.deleted_at {
        Some(dt) => dt,
        None => record.updated_at,
    };

    let days_ago_text = format_days_since_deletion(&deleted_at);

    let meta_style = if is_visual_selected {
        Style::default()
            .bg(theme::NL_SELECTED)
            .fg(theme::NL_TEXT_MUTED)
            .add_modifier(Modifier::DIM)
    } else if is_selected {
        Style::default()
            .bg(theme::NL_SELECTED)
            .fg(theme::NL_TEXT_MUTED)
    } else {
        Style::default().fg(theme::NL_TEXT_MUTED).bg(theme::NL_BG)
    };

    let mut meta_spans = Vec::new();
    if is_selected {
        let gutter_style = Style::default().bg(if focused {
            theme::NL_CYAN
        } else {
            theme::NL_LINE
        });
        meta_spans.push(Span::styled(" ", gutter_style));
    }
    let meta_prefix = if is_selected { " " } else { ITEM_LEFT_PADDING };
    let mut meta_text_width =
        gutter_width + display_width(meta_prefix) + display_width(&days_ago_text);
    meta_spans.push(Span::styled(
        format!("{}{}", meta_prefix, days_ago_text),
        meta_style,
    ));

    match calculate_remaining_days(&deleted_at, retention_days) {
        None => {
            let label = t!("tui.trash.will_not_auto_delete");
            let text = format!("  {}", label);
            meta_text_width += display_width(&text);
            meta_spans.push(Span::styled(text, meta_style.fg(theme::NL_TEXT_MUTED)));
        }
        Some(remaining) => {
            let tier = trash_warning_tier(remaining);
            let remaining_text = t!("tui.trash.auto_delete_in", days = remaining.max(0));

            let (warning_prefix, warning_color, add_bold) = match tier {
                TrashWarningTier::Safe => ("", theme::NL_TEXT_MUTED, false),
                TrashWarningTier::Moderate => ("\u{26A0} ", theme::WARNING, false),
                TrashWarningTier::Urgent => ("\u{26A0}\u{26A0} ", theme::WARNING, true),
                TrashWarningTier::Critical => ("\u{26A0}\u{26A0}\u{26A0} ", theme::ERROR, true),
            };

            let mut style = meta_style.fg(warning_color);
            if add_bold {
                style = style.add_modifier(Modifier::BOLD);
            }

            if !warning_prefix.is_empty() {
                let text = format!("  {}", warning_prefix);
                meta_text_width += display_width(&text);
                meta_spans.push(Span::styled(text, style));
            }
            let text = format!("  {}", remaining_text);
            meta_text_width += display_width(&text);
            meta_spans.push(Span::styled(text, style));
        }
    }
    let meta_padding = " ".repeat((area_width as usize).saturating_sub(meta_text_width));
    meta_spans.push(Span::styled(meta_padding, meta_style));
    let meta_line = Line::from(meta_spans);

    // ── Line 3: Separator ──
    let sep_char = if unicode { '\u{2500}' } else { '-' };
    let sep_text: String = std::iter::repeat_n(sep_char, area_width as usize).collect();
    let separator_line = Line::from(Span::styled(
        sep_text,
        Style::default().fg(theme::NL_LINE).bg(theme::NL_BG),
    ));

    if is_min_width {
        ListItem::new(vec![title_line, separator_line])
    } else {
        ListItem::new(vec![title_line, meta_line, separator_line])
    }
}

fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

#[cfg(test)]
mod width_tests {
    use super::*;

    #[test]
    fn display_width_cjk_yesterday() {
        assert_eq!(display_width("昨天"), 4, "昨天 should be 4 display columns");
    }

    #[test]
    fn display_width_english_yesterday() {
        assert_eq!(display_width("yesterday"), 9);
    }

    #[test]
    fn display_width_marker() {
        let marker = "\u{25C0}";
        let w = display_width(marker);
        assert!(
            w == 1 || w == 2,
            "◀ display width should be 1 or 2, got {}",
            w
        );
    }

    #[test]
    fn chars_count_vs_display_width_for_marker() {
        let right = "\u{25C0} ".to_string();
        assert_eq!(right.chars().count(), 2, "chars().count() for ◀ + 1 space");
        let dw = display_width(&right);
        println!(
            "display_width of '◀ ' = {} (chars().count = {})",
            dw,
            right.chars().count()
        );
    }
}
