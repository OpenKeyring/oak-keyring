//! Status bar rendering for the main screen.
//!
//! Renders a single-line bar at the bottom of the terminal with:
//! - Left: keyboard shortcuts (contextual to focused panel)
//! - Center: version string
//! - Right: sync status indicator + status message

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::commands::types::PanelId;
use crate::tui::state::main_state::{StatusBarState, StatusMessage, SyncIndicator};
use crate::tui::theme;

/// Application version displayed in the status bar center.
const VERSION: &str = "v0.1.0";

/// Section separator character.
const SEPARATOR: &str = " \u{2502} "; // " │ "

// ── Shortcut strings per panel (unicode) ──────────────────────────────────────

const SHORTCUTS_SIDEBAR_LIST: &str = "\u{2318}K\u{641C}\u{7D22} n\u{65B0}\u{5EFA} e\u{7F16}\u{8F91} d\u{5220}\u{9664} F1\u{5E2E}\u{52A9}";
// ⌘K搜索 n新建 e编辑 d删除 F1帮助

const SHORTCUTS_DETAIL: &str = "c\u{590D}\u{5236}\u{5BC6}\u{7801} u\u{590D}\u{5236}\u{7528}\u{6237}\u{540D} p\u{663E}\u{793A}/\u{9690}\u{85CF} e\u{7F16}\u{8F91} d\u{5220}\u{9664} F1\u{5E2E}\u{52A9}";
// c复制密码 u复制用户名 p显示/隐藏 e编辑 d删除 F1帮助

// ── ASCII fallbacks for shortcuts ─────────────────────────────────────────────

const SHORTCUTS_SIDEBAR_LIST_ASCII: &str = "Ctrl+K Search  N New  E Edit  D Delete  F1 Help";
const SHORTCUTS_DETAIL_ASCII: &str =
    "C CopyPwd  U CopyUser  P Show/Hide  E Edit  D Delete  F1 Help";

// ── Sync indicator strings (unicode) ─────────────────────────────────────────

const SYNC_SYNCED_UNICODE: &str = "\u{2713} \u{5DF2}\u{540C}\u{6B65}"; // ✓ 已同步
const SYNC_SYNCING_UNICODE: &str = "\u{27F3} \u{540C}\u{6B65}\u{4E2D}"; // ⟳ 同步中
const SYNC_FAILED_UNICODE: &str = "\u{2717} \u{540C}\u{6B65}\u{5931}\u{8D25}"; // ✗ 同步失败
const SYNC_OFFLINE_UNICODE: &str = "\u{25D0} \u{79BB}\u{7EBF}"; // ◐ 离线
const SYNC_NOT_CONFIGURED_UNICODE: &str = "\u{2014} \u{672A}\u{914D}\u{7F6E}"; // — 未配置

// ── ASCII fallbacks for sync indicators ───────────────────────────────────────

const SYNC_SYNCED_ASCII: &str = "+ Synced";
const SYNC_SYNCING_ASCII: &str = "~ Syncing";
const SYNC_FAILED_ASCII: &str = "x Sync failed";
const SYNC_OFFLINE_ASCII: &str = "o Offline";
const SYNC_NOT_CONFIGURED_ASCII: &str = "- Not configured";

/// Panel responsible for rendering the status bar.
pub struct StatusBarPanel;

impl StatusBarPanel {
    /// Render the status bar into the given frame area.
    ///
    /// The status bar is a single-line bar showing keyboard shortcuts (left),
    /// version string (center), and sync status + message (right).
    ///
    /// # Arguments
    /// * `frame` - The ratatui frame to render into.
    /// * `area` - The rectangular area allocated to the status bar (typically 1 row).
    /// * `state` - The current status bar state.
    /// * `focused_panel` - Which panel currently has keyboard focus (affects shortcut display).
    /// * `unicode` - Whether to use unicode characters (vs ASCII fallbacks).
    pub fn view(
        frame: &mut Frame,
        area: Rect,
        state: &StatusBarState,
        focused_panel: PanelId,
        unicode: bool,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let bar_bg = theme::BG_BAR;
        let sep_style = Style::default().fg(theme::BORDER).bg(bar_bg);
        let shortcut_style = Style::default().fg(theme::TEXT_SECONDARY).bg(bar_bg);
        let version_style = Style::default().fg(theme::TEXT_MUTED).bg(bar_bg);
        let sync_style = Style::default()
            .fg(sync_color(&state.sync_status))
            .bg(bar_bg);
        let msg_style = Style::default().fg(theme::TEXT_SECONDARY).bg(bar_bg);

        let shortcuts = shortcuts_text(focused_panel, unicode);
        let sync_text = sync_indicator_text(&state.sync_status, unicode);
        let status_msg = status_message_text(&state.status_message);

        let spans = vec![
            Span::styled("  ", shortcut_style),
            Span::styled(shortcuts, shortcut_style),
            Span::styled(SEPARATOR, sep_style),
            Span::styled(VERSION, version_style),
            Span::styled(SEPARATOR, sep_style),
            Span::styled(sync_text, sync_style),
        ];

        // Add status message if present
        let mut all_spans = spans;
        if let Some(msg) = status_msg {
            all_spans.push(Span::styled(SEPARATOR, sep_style));
            all_spans.push(Span::styled(msg, msg_style));
        }

        let paragraph = Paragraph::new(Line::from(all_spans)).style(Style::default().bg(bar_bg));

        frame.render_widget(paragraph, area);
    }
}

/// Return the shortcut hint string based on the focused panel.
fn shortcuts_text(focused_panel: PanelId, unicode: bool) -> &'static str {
    match focused_panel {
        PanelId::Sidebar | PanelId::List => {
            if unicode {
                SHORTCUTS_SIDEBAR_LIST
            } else {
                SHORTCUTS_SIDEBAR_LIST_ASCII
            }
        }
        PanelId::Detail => {
            if unicode {
                SHORTCUTS_DETAIL
            } else {
                SHORTCUTS_DETAIL_ASCII
            }
        }
    }
}

/// Return the sync indicator display string.
fn sync_indicator_text(sync: &SyncIndicator, unicode: bool) -> &'static str {
    match sync {
        SyncIndicator::Synced => {
            if unicode {
                SYNC_SYNCED_UNICODE
            } else {
                SYNC_SYNCED_ASCII
            }
        }
        SyncIndicator::Syncing => {
            if unicode {
                SYNC_SYNCING_UNICODE
            } else {
                SYNC_SYNCING_ASCII
            }
        }
        SyncIndicator::Failed => {
            if unicode {
                SYNC_FAILED_UNICODE
            } else {
                SYNC_FAILED_ASCII
            }
        }
        SyncIndicator::Offline => {
            if unicode {
                SYNC_OFFLINE_UNICODE
            } else {
                SYNC_OFFLINE_ASCII
            }
        }
        SyncIndicator::NotConfigured => {
            if unicode {
                SYNC_NOT_CONFIGURED_UNICODE
            } else {
                SYNC_NOT_CONFIGURED_ASCII
            }
        }
    }
}

/// Return the color for a sync indicator state.
fn sync_color(sync: &SyncIndicator) -> Color {
    match sync {
        SyncIndicator::Synced => theme::SUCCESS,
        SyncIndicator::Syncing => theme::PRIMARY,
        SyncIndicator::Failed => theme::ERROR,
        SyncIndicator::Offline => theme::WARNING,
        SyncIndicator::NotConfigured => theme::TEXT_MUTED,
    }
}

/// Format the status message as a display string, if present.
fn status_message_text(msg: &Option<StatusMessage>) -> Option<String> {
    match msg {
        Some(StatusMessage::RecordCount(n)) => Some(format!("{}\u{6761}\u{5BC6}\u{7801}", n)), // "{n} 条密码"
        Some(StatusMessage::ClipboardCountdown { field, seconds }) => {
            Some(format!(
                "\u{2713} \u{5DF2}\u{590D}\u{5236}{}\u{FF08}{}s \u{540E}\u{6E05}\u{9664}\u{FF09}",
                field, seconds
            ))
            // "✓ 已复制{field}（{seconds}s 后清除）"
        }
        Some(StatusMessage::Temporary { text, .. }) => Some(text.clone()),
        Some(StatusMessage::Search(q)) => Some(format!("\u{641C}\u{7D22}: {}...", q)), // "搜索: {q}..."
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_sidebar_unicode() {
        let text = shortcuts_text(PanelId::Sidebar, true);
        assert!(text.contains('\u{2318}')); // ⌘
    }

    #[test]
    fn shortcuts_list_same_as_sidebar() {
        assert_eq!(
            shortcuts_text(PanelId::Sidebar, true),
            shortcuts_text(PanelId::List, true)
        );
    }

    #[test]
    fn shortcuts_detail_unicode() {
        let text = shortcuts_text(PanelId::Detail, true);
        assert!(text.contains('c'));
    }

    #[test]
    fn shortcuts_sidebar_ascii() {
        let text = shortcuts_text(PanelId::Sidebar, false);
        assert!(text.contains("Search"));
    }

    #[test]
    fn shortcuts_detail_ascii() {
        let text = shortcuts_text(PanelId::Detail, false);
        assert!(text.contains("CopyPwd"));
    }

    #[test]
    fn sync_indicator_synced_unicode() {
        let text = sync_indicator_text(&SyncIndicator::Synced, true);
        assert!(text.starts_with('\u{2713}')); // ✓
    }

    #[test]
    fn sync_indicator_syncing_unicode() {
        let text = sync_indicator_text(&SyncIndicator::Syncing, true);
        assert!(text.starts_with('\u{27F3}')); // ⟳
    }

    #[test]
    fn sync_indicator_failed_unicode() {
        let text = sync_indicator_text(&SyncIndicator::Failed, true);
        assert!(text.starts_with('\u{2717}')); // ✗
    }

    #[test]
    fn sync_indicator_offline_unicode() {
        let text = sync_indicator_text(&SyncIndicator::Offline, true);
        assert!(text.starts_with('\u{25D0}')); // ◐
    }

    #[test]
    fn sync_indicator_not_configured_unicode() {
        let text = sync_indicator_text(&SyncIndicator::NotConfigured, true);
        assert!(text.starts_with('\u{2014}')); // —
    }

    #[test]
    fn sync_indicator_ascii_fallbacks() {
        assert_eq!(
            sync_indicator_text(&SyncIndicator::Synced, false),
            "+ Synced"
        );
        assert_eq!(
            sync_indicator_text(&SyncIndicator::Syncing, false),
            "~ Syncing"
        );
        assert_eq!(
            sync_indicator_text(&SyncIndicator::Failed, false),
            "x Sync failed"
        );
        assert_eq!(
            sync_indicator_text(&SyncIndicator::Offline, false),
            "o Offline"
        );
        assert_eq!(
            sync_indicator_text(&SyncIndicator::NotConfigured, false),
            "- Not configured"
        );
    }

    #[test]
    fn sync_colors() {
        assert_eq!(sync_color(&SyncIndicator::Synced), theme::SUCCESS);
        assert_eq!(sync_color(&SyncIndicator::Syncing), theme::PRIMARY);
        assert_eq!(sync_color(&SyncIndicator::Failed), theme::ERROR);
        assert_eq!(sync_color(&SyncIndicator::Offline), theme::WARNING);
        assert_eq!(sync_color(&SyncIndicator::NotConfigured), theme::TEXT_MUTED);
    }

    #[test]
    fn status_message_record_count() {
        let msg = StatusMessage::RecordCount(42);
        let text = status_message_text(&Some(msg)).unwrap();
        assert!(text.contains("42"));
        assert!(text.contains('\u{6761}')); // 条
    }

    #[test]
    fn status_message_clipboard_countdown() {
        let msg = StatusMessage::ClipboardCountdown {
            field: "\u{5BC6}\u{7801}".to_string(), // 密码
            seconds: 30,
        };
        let text = status_message_text(&Some(msg)).unwrap();
        assert!(text.contains("30s"));
    }

    #[test]
    fn status_message_temporary() {
        let msg = StatusMessage::Temporary {
            text: "hello world".to_string(),
            ttl: 10,
        };
        let text = status_message_text(&Some(msg)).unwrap();
        assert_eq!(text, "hello world");
    }

    #[test]
    fn status_message_search() {
        let msg = StatusMessage::Search("test query".to_string());
        let text = status_message_text(&Some(msg)).unwrap();
        assert!(text.contains("test query"));
        assert!(text.ends_with("..."));
    }

    #[test]
    fn status_message_none() {
        assert!(status_message_text(&None).is_none());
    }

    #[test]
    fn version_constant() {
        assert_eq!(VERSION, "v0.1.0");
    }
}
