//! Status bar rendering for the main screen.
//!
//! Renders a single-line bar at the bottom of the terminal with:
//! - Left: keyboard shortcuts (contextual to focused panel)
//! - Center: version string
//! - Right: sync status indicator + status message

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::commands::types::PanelId;
use crate::t;
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

// ── Trash shortcut strings (unicode) ──────────────────────────────────────────

const SHORTCUTS_TRASH_LIST: &str = "r\u{6062}\u{590D} D\u{6C38}\u{4E45}\u{5220}\u{9664} a\u{6E05}\u{7A7A}\u{56DE}\u{6536}\u{7AD9} F1\u{5E2E}\u{52A9}";
// r恢复 D永久删除 a清空回收站 F1帮助

const SHORTCUTS_TRASH_DETAIL: &str =
    "c\u{590D}\u{5236}\u{5BC6}\u{7801} u\u{590D}\u{5236}\u{7528}\u{6237}\u{540D} p\u{663E}\u{793A}/\u{9690}\u{85CF} r\u{6062}\u{590D} D\u{6C38}\u{4E45}\u{5220}\u{9664} F1\u{5E2E}\u{52A9}";
// c复制密码 u复制用户名 p显示/隐藏 r恢复 D永久删除 F1帮助

// ── Trash shortcut ASCII fallbacks ─────────────────────────────────────────────

const SHORTCUTS_TRASH_LIST_ASCII: &str = "R Restore  D PermDelete  A EmptyTrash  F1 Help";
const SHORTCUTS_TRASH_DETAIL_ASCII: &str =
    "C CopyPwd  U CopyUser  P Show/Hide  R Restore  D PermDelete  F1 Help";

// ── Visual mode shortcut strings ──────────────────────────────────────────────

const SHORTCUTS_VISUAL_UNICODE: &str = "Space\u{9009}\u{62E9} a\u{5168}\u{9009} d\u{6279}\u{91CF}\u{5220}\u{9664} t\u{6279}\u{91CF}\u{6807}\u{7B7E} Esc\u{9000}\u{51FA}";
// Space选择 a全选 d批量删除 t批量标签 Esc退出

const SHORTCUTS_VISUAL_ASCII: &str = "Space Select  A All  D BatchDel  T BatchTag  Esc Exit";

/// Visual indicator text shown in status bar when visual mode is active.
const VISUAL_INDICATOR_UNICODE: &str = "VISUAL";
const VISUAL_INDICATOR_ASCII: &str = "[VISUAL]";

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
        is_trash: bool,
        visual_mode: bool,
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

        let shortcuts = if visual_mode && matches!(focused_panel, PanelId::List | PanelId::Detail) {
            visual_shortcuts_text(unicode)
        } else {
            shortcuts_text(focused_panel, unicode, is_trash)
        };
        let sync_text = sync_indicator_text(&state.sync_status, unicode);
        let status_msg = status_message_text(&state.status_message);

        let mut all_spans = vec![
            Span::styled("  ", shortcut_style),
            Span::styled(shortcuts, shortcut_style),
        ];

        // VISUAL indicator when in visual mode
        if visual_mode {
            let indicator = if unicode {
                VISUAL_INDICATOR_UNICODE
            } else {
                VISUAL_INDICATOR_ASCII
            };
            all_spans.push(Span::styled(SEPARATOR, sep_style));
            all_spans.push(Span::styled(
                format!(" {} ", indicator),
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD)
                    .bg(theme::BG_BAR),
            ));
        }

        all_spans.push(Span::styled(SEPARATOR, sep_style));
        all_spans.push(Span::styled(VERSION, version_style));

        // Health Check status — show phase-appropriate message
        use crate::tui::state::main_state::HealthCheckPhase;
        match state.health_check_phase {
            HealthCheckPhase::Checking => {
                let icon = if unicode { theme::ICON_SEARCH } else { "[?]" };
                let text = t!("tui.health.checking");
                all_spans.push(Span::styled(SEPARATOR, sep_style));
                all_spans.push(Span::styled(
                    format!("{} {}", icon, text),
                    Style::default().fg(theme::PRIMARY).bg(bar_bg),
                ));
            }
            HealthCheckPhase::NeedsAttention {
                weak,
                compromised,
                duplicate_groups,
            } => {
                let icon = if unicode { theme::ICON_WARNING } else { "[!]" };
                let text = t!(
                    "tui.health.needs_attention",
                    weak = weak,
                    compromised = compromised,
                    duplicate = duplicate_groups
                );
                all_spans.push(Span::styled(SEPARATOR, sep_style));
                all_spans.push(Span::styled(
                    format!("{} {}", icon, text),
                    Style::default().fg(theme::WARNING).bg(bar_bg),
                ));
            }
            HealthCheckPhase::AllSecure => {
                let text = if unicode {
                    format!("{} {}", theme::ICON_SUCCESS, t!("tui.health.all_secure"))
                } else {
                    format!("+ {}", t!("tui.health.all_secure"))
                };
                all_spans.push(Span::styled(SEPARATOR, sep_style));
                all_spans.push(Span::styled(
                    text,
                    Style::default().fg(theme::SUCCESS).bg(bar_bg),
                ));
            }
            HealthCheckPhase::Skipped => {
                let text = if unicode {
                    format!(
                        "{} {}",
                        theme::ICON_NOT_CONFIGURED,
                        t!("tui.health.skipped_short")
                    )
                } else {
                    format!("- {}", t!("tui.health.skipped_short"))
                };
                all_spans.push(Span::styled(SEPARATOR, sep_style));
                all_spans.push(Span::styled(
                    text,
                    Style::default().fg(theme::TEXT_MUTED).bg(bar_bg),
                ));
            }
            HealthCheckPhase::Inactive => {
                // No health check indicator when inactive (initial state)
            }
        }

        all_spans.push(Span::styled(SEPARATOR, sep_style));
        all_spans.push(Span::styled(sync_text, sync_style));

        // Add status message if present
        if let Some(msg) = status_msg {
            all_spans.push(Span::styled(SEPARATOR, sep_style));
            all_spans.push(Span::styled(msg, msg_style));
        }

        let paragraph = Paragraph::new(Line::from(all_spans)).style(Style::default().bg(bar_bg));

        frame.render_widget(paragraph, area);
    }
}

/// Return the shortcut hint string based on the focused panel and trash state.
fn shortcuts_text(focused_panel: PanelId, unicode: bool, is_trash: bool) -> &'static str {
    match (focused_panel, is_trash) {
        (PanelId::Sidebar, _) | (PanelId::List, false) => {
            if unicode {
                SHORTCUTS_SIDEBAR_LIST
            } else {
                SHORTCUTS_SIDEBAR_LIST_ASCII
            }
        }
        (PanelId::List, true) => {
            if unicode {
                SHORTCUTS_TRASH_LIST
            } else {
                SHORTCUTS_TRASH_LIST_ASCII
            }
        }
        (PanelId::Detail, false) => {
            if unicode {
                SHORTCUTS_DETAIL
            } else {
                SHORTCUTS_DETAIL_ASCII
            }
        }
        (PanelId::Detail, true) => {
            if unicode {
                SHORTCUTS_TRASH_DETAIL
            } else {
                SHORTCUTS_TRASH_DETAIL_ASCII
            }
        }
    }
}

/// Return the visual mode shortcut text.
fn visual_shortcuts_text(unicode: bool) -> &'static str {
    if unicode {
        SHORTCUTS_VISUAL_UNICODE
    } else {
        SHORTCUTS_VISUAL_ASCII
    }
}

/// Return the sync indicator display string.
fn sync_indicator_text(sync: &SyncIndicator, unicode: bool) -> String {
    match sync {
        SyncIndicator::Synced => {
            let text = t!("tui.status_bar.sync_synced");
            if unicode {
                format!("{} {}", theme::ICON_SUCCESS, text)
            } else {
                format!("+ {}", text)
            }
        }
        SyncIndicator::Syncing => {
            let text = t!("tui.status_bar.sync_syncing");
            if unicode {
                format!("{} {}", theme::ICON_SYNC_SYNCING, text)
            } else {
                format!("~ {}", text)
            }
        }
        SyncIndicator::Failed => {
            let text = t!("tui.status_bar.sync_failed");
            if unicode {
                format!("{} {}", theme::ICON_ERROR, text)
            } else {
                format!("x {}", text)
            }
        }
        SyncIndicator::Offline => {
            let text = t!("tui.status_bar.sync_offline");
            if unicode {
                format!("{} {}", theme::ICON_SYNC_OFFLINE, text)
            } else {
                format!("o {}", text)
            }
        }
        SyncIndicator::NotConfigured => {
            let text = t!("tui.status_bar.sync_not_configured");
            if unicode {
                format!("{} {}", theme::ICON_NOT_CONFIGURED, text)
            } else {
                format!("- {}", text)
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
        Some(StatusMessage::RecordCount(n)) => {
            Some(t!("tui.status_bar.record_count", count = n).to_string())
        }
        Some(StatusMessage::ClipboardCountdown { field: _, seconds }) => Some(format!(
            "{} {}",
            theme::ICON_SUCCESS,
            t!("tui.notification.clipboard_clearing", seconds = seconds)
        )),
        Some(StatusMessage::Temporary { text, .. }) => Some(text.clone()),
        Some(StatusMessage::Search(q)) => Some(format!(
            "{}: {}...",
            t!("tui.password_list.search_prompt"),
            q
        )),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_sidebar_unicode() {
        let text = shortcuts_text(PanelId::Sidebar, true, false);
        assert!(text.contains('\u{2318}')); // ⌘
    }

    #[test]
    fn shortcuts_list_same_as_sidebar() {
        assert_eq!(
            shortcuts_text(PanelId::Sidebar, true, false),
            shortcuts_text(PanelId::List, true, false)
        );
    }

    #[test]
    fn shortcuts_detail_unicode() {
        let text = shortcuts_text(PanelId::Detail, true, false);
        assert!(text.contains('c'));
    }

    #[test]
    fn shortcuts_sidebar_ascii() {
        let text = shortcuts_text(PanelId::Sidebar, false, false);
        assert!(text.contains("Search"));
    }

    #[test]
    fn shortcuts_detail_ascii() {
        let text = shortcuts_text(PanelId::Detail, false, false);
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
        // Text may be localized, so we just check it's non-empty
        assert!(!text.is_empty());
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

    // ── Trash-aware status bar tests ─────────────────────────────────────────

    #[test]
    fn trash_list_shortcuts_unicode() {
        let text = shortcuts_text(PanelId::List, true, true);
        assert!(
            text.contains('r'),
            "trash list shortcuts should contain 'r' for restore"
        );
        assert!(
            text.contains('D'),
            "trash list shortcuts should contain 'D' for permanent delete"
        );
        assert!(
            text.contains('a'),
            "trash list shortcuts should contain 'a' for empty trash"
        );
    }

    #[test]
    fn trash_detail_shortcuts_unicode() {
        let text = shortcuts_text(PanelId::Detail, true, true);
        assert!(
            text.contains('c'),
            "trash detail shortcuts should contain 'c' for copy"
        );
        assert!(
            text.contains('r'),
            "trash detail shortcuts should contain 'r' for restore"
        );
        assert!(
            text.contains('D'),
            "trash detail shortcuts should contain 'D' for permanent delete"
        );
    }

    #[test]
    fn normal_list_shortcuts_no_trash() {
        let text = shortcuts_text(PanelId::List, true, false);
        assert!(
            !text.contains("\u{6E05}\u{7A7A}\u{56DE}\u{6536}\u{7AD9}"),
            "normal list should not contain '清空回收站'"
        );
    }

    #[test]
    fn normal_detail_shortcuts_no_trash() {
        let text = shortcuts_text(PanelId::Detail, true, false);
        assert!(
            !text.contains("\u{6062}\u{590D}"),
            "normal detail should not contain '恢复'"
        );
    }

    #[test]
    fn visual_mode_shortcuts_shown_when_visual_active() {
        let text = visual_shortcuts_text(true);
        assert!(
            text.contains("Space") || text.contains("Space\u{9009}\u{62E9}"),
            "visual shortcuts should mention Space"
        );
    }

    #[test]
    fn normal_shortcuts_shown_when_not_visual() {
        let text = shortcuts_text(PanelId::List, true, false);
        assert!(
            text.is_empty() || !text.contains("Space\u{9009}\u{62E9}"),
            "non-visual should not show Space shortcut"
        );
    }
}
