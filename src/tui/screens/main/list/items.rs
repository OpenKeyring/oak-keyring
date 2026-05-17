use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::commands::types::HealthIssue;
use crate::t;
use crate::tui::state::list_state::{
    calculate_remaining_days, format_days_since_deletion, format_relative_time, format_type_prefix,
    trash_warning_tier, TrashWarningTier,
};
use crate::tui::terminal::WidthTier;
use crate::tui::theme;

use super::ListPanel;

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
        let text_lower = text.to_lowercase();
        let len = text.len();
        // Track which characters are matched
        let mut matched = vec![false; len];
        for term in search_terms {
            let term_lower = term.to_lowercase();
            let mut start = 0;
            while start + term_lower.len() <= len {
                if text_lower[start..].starts_with(&term_lower) {
                    let end = (start + term_lower.len()).min(len);
                    for m in &mut matched[start..end] {
                        *m = true;
                    }
                    start = end;
                } else {
                    start += 1;
                }
            }
        }
        // Build spans from matched/unmatched ranges
        let mut spans = Vec::new();
        let mut i = 0;
        while i < len {
            if matched[i] {
                let start = i;
                while i < len && matched[i] {
                    i += 1;
                }
                spans.push(Span::styled(
                    text[start..i].to_string(),
                    Style::default()
                        .fg(theme::WARNING)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                let start = i;
                while i < len && !matched[i] {
                    i += 1;
                }
                spans.push(Span::styled(
                    text[start..i].to_string(),
                    Style::default().fg(theme::TEXT),
                ));
            }
        }
        spans
    }
}

// ---------------------------------------------------------------------------
// Health badge
// ---------------------------------------------------------------------------

/// Build a styled health badge span for a given `HealthIssue`.
///
/// Returns `None` for `Expired` (shown only in the detail panel) or when
/// `issue` is `None`.
pub(super) fn health_badge(issue: Option<&HealthIssue>, unicode: bool) -> Option<Span<'static>> {
    issue.and_then(|i| match i {
        HealthIssue::Compromised => {
            let icon = if unicode { "\u{1F534}" } else { "!" }; // 🔴 / !
            let label = t!("tui.password_list.health_leaked");
            Some(Span::styled(
                format!(" {}{}", icon, label),
                Style::default().fg(theme::ERROR),
            ))
        }
        HealthIssue::Weak => {
            let icon = if unicode {
                theme::ICON_WARNING
            } else {
                theme::ascii::ICON_WARNING
            };
            let label = t!("tui.password_list.health_weak");
            Some(Span::styled(
                format!(" {}{}", icon, label),
                Style::default().fg(theme::WARNING),
            ))
        }
        HealthIssue::Duplicate { group_size } => {
            let icon = if unicode {
                theme::ICON_WARNING
            } else {
                theme::ascii::ICON_WARNING
            };
            let label = t!("tui.health.duplicate_label", count = group_size);
            Some(Span::styled(
                format!(" {}{}", icon, label),
                Style::default().fg(theme::WARNING),
            ))
        }
        HealthIssue::Expired => None, // Shown in detail panel, not list badge
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
    let indicator = if unicode { " \u{25C0}" } else { " <" }; // ◀ / <
    let is_min_width = WidthTier::from_width(area_width) == WidthTier::Minimum;

    // ── Line 1: Title ──
    let type_prefix = format_type_prefix(&record.credential_type);
    let timestamp = format_relative_time(&record.updated_at);

    // Build name spans: prefix (plain) + highlighted name (if search active)
    let prefix_str = format!("  {}", type_prefix);

    // Priority: Compromised > Weak > Duplicate (matches S3 spec)
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
    } else {
        None
    };
    let badge_str = badge.as_ref().map(|s| s.content.as_ref()).unwrap_or("");
    let right_part = format!("{}{}", timestamp, if is_selected { indicator } else { "" });

    // Calculate total name content length for padding
    let name_len = prefix_str.chars().count() + record.name.chars().count();

    let padding_len = (area_width as usize)
        .saturating_sub(name_len)
        .saturating_sub(badge_str.chars().count())
        .saturating_sub(right_part.chars().count());

    let base_style = if is_visual_selected {
        Style::default()
            .bg(theme::BRAND)
            .fg(theme::TEXT)
            .add_modifier(Modifier::DIM)
    } else if is_selected && focused {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT)
    };

    // Determine badge span style (override for visual-selected context)
    let badge_span = badge.map(|span| {
        if is_visual_selected {
            // Preserve original badge color for visual-selected override
            let badge_fg = if record.is_compromised {
                theme::ERROR
            } else {
                theme::WARNING
            };
            Span::styled(
                span.content,
                Style::default()
                    .bg(theme::BRAND)
                    .fg(badge_fg)
                    .add_modifier(Modifier::DIM),
            )
        } else {
            span
        }
    });

    // Build title spans with optional search highlighting
    let mut title_spans = vec![Span::styled(prefix_str, base_style)];
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

    let title_line = Line::from(title_spans);

    // ── Line 2: Subtitle ──
    let subtitle_style = if is_visual_selected {
        Style::default()
            .bg(theme::BRAND)
            .fg(theme::TEXT_SECONDARY)
            .add_modifier(Modifier::DIM)
    } else if is_selected && focused {
        Style::default()
            .fg(theme::TEXT_SECONDARY)
            .add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };

    // Build subtitle with optional search highlighting
    let subtitle_prefix = "  ";
    let subtitle_line = if let Some(query) = search_query {
        let terms: Vec<String> = query
            .to_lowercase()
            .split_whitespace()
            .map(String::from)
            .collect();
        let mut sub_spans = vec![Span::styled(subtitle_prefix, subtitle_style)];
        sub_spans.extend(ListPanel::highlight_match(&record.subtitle, &terms));
        Line::from(sub_spans)
    } else {
        Line::from(Span::styled(
            format!("  {}", record.subtitle),
            subtitle_style,
        ))
    };

    // ── Line 3: Blank separator (empty line) ──
    let separator_line = Line::from("");

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
    let indicator = if unicode { " \u{25C0}" } else { " <" };
    let is_min_width = WidthTier::from_width(area_width) == WidthTier::Minimum;

    // ── Line 1: Title with type prefix ──
    let type_prefix = format_type_prefix(&record.credential_type);
    let prefix_str = format!("  {}", type_prefix);

    let right_part = if is_selected { indicator } else { "" };
    let name_len = prefix_str.chars().count() + record.name.chars().count();
    let padding_len = (area_width as usize)
        .saturating_sub(name_len)
        .saturating_sub(right_part.chars().count());

    let base_style = if is_visual_selected {
        Style::default()
            .bg(theme::BRAND)
            .fg(theme::TEXT)
            .add_modifier(Modifier::DIM)
    } else if is_selected && focused {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT)
    };

    let title_spans = vec![
        Span::styled(prefix_str, base_style),
        Span::styled(record.name.clone(), base_style),
        Span::styled(" ".repeat(padding_len), base_style),
        Span::styled(right_part.to_string(), base_style),
    ];
    let title_line = Line::from(title_spans);

    // ── Line 2: Deletion metadata with progressive warnings ──
    let deleted_at = match record.deleted_at {
        Some(dt) => dt,
        None => record.updated_at,
    };

    let days_ago_text = format_days_since_deletion(&deleted_at);

    let mut meta_spans = vec![Span::styled(
        format!("  {}", days_ago_text),
        Style::default().fg(theme::TEXT_SECONDARY),
    )];

    match calculate_remaining_days(&deleted_at, retention_days) {
        None => {
            let label = t!("tui.trash.will_not_auto_delete");
            meta_spans.push(Span::styled(
                format!("  {}", label),
                Style::default().fg(theme::TEXT_MUTED),
            ));
        }
        Some(remaining) => {
            let tier = trash_warning_tier(remaining);
            let remaining_text = t!("tui.trash.auto_delete_in", days = remaining.max(0));

            let (warning_prefix, warning_color, add_bold) = match tier {
                TrashWarningTier::Safe => ("", theme::TEXT_SECONDARY, false),
                TrashWarningTier::Moderate => ("\u{26A0} ", theme::WARNING, false),
                TrashWarningTier::Urgent => ("\u{26A0}\u{26A0} ", theme::WARNING, true),
                TrashWarningTier::Critical => ("\u{26A0}\u{26A0}\u{26A0} ", theme::ERROR, true),
            };

            let mut style = Style::default().fg(warning_color);
            if add_bold {
                style = style.add_modifier(Modifier::BOLD);
            }

            if !warning_prefix.is_empty() {
                meta_spans.push(Span::styled(format!("  {}", warning_prefix), style));
            }
            meta_spans.push(Span::styled(format!("  {}", remaining_text), style));
        }
    }
    let meta_line = Line::from(meta_spans);

    // ── Line 3: Separator ──
    let sep_char = if unicode { '\u{2500}' } else { '-' };
    let sep_text: String = std::iter::repeat_n(sep_char, area_width as usize).collect();
    let separator_line = Line::from(Span::styled(sep_text, Style::default().fg(theme::BORDER)));

    if is_min_width {
        ListItem::new(vec![title_line, separator_line])
    } else {
        ListItem::new(vec![title_line, meta_line, separator_line])
    }
}
