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
use crate::tui::state::detail_state::DetailViewData;
use crate::tui::state::main_state::{StatusBarState, StatusMessage, SyncIndicator};
use crate::tui::theme;
use crate::types::credential::CredentialType;

/// Application version displayed in the status bar center.
const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// Section separator character.
const SEPARATOR: &str = " \u{2502} "; // " │ "

/// Visual indicator text shown in status bar when visual mode is active.
const VISUAL_INDICATOR_UNICODE: &str = "VISUAL";
const VISUAL_INDICATOR_ASCII: &str = "[VISUAL]";

/// Panel responsible for rendering the status bar.
pub struct StatusBarPanel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailShortcutContext {
    Login,
    Api,
    Ssh,
    SecureNote,
}

impl DetailShortcutContext {
    pub fn from_record(record: Option<&DetailViewData>) -> Self {
        match record.map(|record| record.credential_type) {
            Some(CredentialType::Api) => Self::Api,
            Some(CredentialType::Ssh) => Self::Ssh,
            Some(CredentialType::SecureNote) => Self::SecureNote,
            Some(CredentialType::Login) | None => Self::Login,
        }
    }
}

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
    #[allow(clippy::too_many_arguments)]
    pub fn view(
        frame: &mut Frame,
        area: Rect,
        state: &StatusBarState,
        focused_panel: PanelId,
        unicode: bool,
        is_trash: bool,
        visual_mode: bool,
        detail_context: DetailShortcutContext,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let bar_bg = theme::NL_SURFACE_2;
        let sep_style = Style::default().fg(theme::NL_LINE).bg(bar_bg);
        let shortcut_style = Style::default().fg(theme::NL_TEXT_MUTED).bg(bar_bg);
        let version_style = Style::default().fg(theme::NL_TEXT_MUTED).bg(bar_bg);
        let sync_style = Style::default()
            .fg(sync_color(&state.sync_status))
            .bg(bar_bg);
        let msg_style = Style::default().fg(theme::NL_TEXT_MUTED).bg(bar_bg);

        let shortcuts = if visual_mode && matches!(focused_panel, PanelId::List | PanelId::Detail) {
            visual_shortcuts_text(unicode, is_trash)
        } else {
            shortcuts_text(focused_panel, unicode, is_trash, detail_context)
        };
        let sync_text = sync_indicator_text(&state.sync_status, unicode);
        let status_msg = status_message_text(&state.status_message);

        let mut left_spans = vec![
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
            left_spans.push(Span::styled(SEPARATOR, sep_style));
            left_spans.push(Span::styled(
                format!(" {} ", indicator),
                Style::default()
                    .fg(theme::NL_TEXT)
                    .add_modifier(Modifier::BOLD)
                    .bg(theme::NL_SELECTED),
            ));
        }

        let mut right_spans = vec![Span::styled(VERSION, version_style)];

        // Health Check status — show phase-appropriate message
        use crate::tui::state::main_state::HealthCheckPhase;
        match state.health_check_phase {
            HealthCheckPhase::Checking => {
                let icon = if unicode { theme::ICON_SEARCH } else { "[?]" };
                let text = t!("tui.health.checking");
                right_spans.push(Span::styled(SEPARATOR, sep_style));
                right_spans.push(Span::styled(
                    format!("{} {}", icon, text),
                    Style::default().fg(theme::NL_CYAN).bg(bar_bg),
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
                right_spans.push(Span::styled(SEPARATOR, sep_style));
                right_spans.push(Span::styled(
                    format!("{} {}", icon, text),
                    Style::default().fg(theme::NL_HOT).bg(bar_bg),
                ));
            }
            HealthCheckPhase::AllSecure => {
                let text = if unicode {
                    format!("{} {}", theme::ICON_SUCCESS, t!("tui.health.all_secure"))
                } else {
                    format!("+ {}", t!("tui.health.all_secure"))
                };
                right_spans.push(Span::styled(SEPARATOR, sep_style));
                right_spans.push(Span::styled(
                    text,
                    Style::default().fg(theme::NL_SUCCESS).bg(bar_bg),
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
                right_spans.push(Span::styled(SEPARATOR, sep_style));
                right_spans.push(Span::styled(
                    text,
                    Style::default().fg(theme::NL_TEXT_MUTED).bg(bar_bg),
                ));
            }
            HealthCheckPhase::Inactive => {
                // No health check indicator when inactive (initial state)
            }
        }

        right_spans.push(Span::styled(SEPARATOR, sep_style));
        right_spans.push(Span::styled(sync_text, sync_style));

        // Add status message if present
        if let Some(msg) = status_msg {
            right_spans.push(Span::styled(SEPARATOR, sep_style));
            right_spans.push(Span::styled(msg, msg_style));
        }

        right_spans.push(Span::styled("  ", shortcut_style));
        let left_width = Line::from(left_spans.clone()).width();
        let right_width = Line::from(right_spans.clone()).width();
        let spacer = area
            .width
            .saturating_sub(left_width as u16)
            .saturating_sub(right_width as u16) as usize;
        let mut all_spans = left_spans;
        all_spans.push(Span::styled(" ".repeat(spacer), shortcut_style));
        all_spans.extend(right_spans);

        let paragraph = Paragraph::new(Line::from(all_spans)).style(Style::default().bg(bar_bg));

        frame.render_widget(paragraph, area);
    }
}

/// Return the shortcut hint string based on the focused panel and trash state.
fn shortcuts_text(
    focused_panel: PanelId,
    unicode: bool,
    is_trash: bool,
    detail_context: DetailShortcutContext,
) -> String {
    match (focused_panel, is_trash) {
        (PanelId::Sidebar, _) | (PanelId::List, false) => {
            if unicode {
                t!("tui.status_bar.shortcuts_sidebar_list").to_string()
            } else {
                t!("tui.status_bar.shortcuts_sidebar_list_ascii").to_string()
            }
        }
        (PanelId::List, true) => {
            if unicode {
                t!("tui.status_bar.shortcuts_trash_list").to_string()
            } else {
                t!("tui.status_bar.shortcuts_trash_list_ascii").to_string()
            }
        }
        (PanelId::Detail, false) => detail_shortcuts_text(unicode, detail_context, false),
        (PanelId::Detail, true) => detail_shortcuts_text(unicode, detail_context, true),
    }
}

fn detail_shortcuts_text(
    unicode: bool,
    detail_context: DetailShortcutContext,
    is_trash: bool,
) -> String {
    let key = match (detail_context, is_trash, unicode) {
        (DetailShortcutContext::Login, false, true) => "tui.status_bar.shortcuts_detail",
        (DetailShortcutContext::Login, false, false) => "tui.status_bar.shortcuts_detail_ascii",
        (DetailShortcutContext::Login, true, true) => "tui.status_bar.shortcuts_trash_detail",
        (DetailShortcutContext::Login, true, false) => {
            "tui.status_bar.shortcuts_trash_detail_ascii"
        }
        (DetailShortcutContext::Api, false, true) => "tui.status_bar.shortcuts_detail_api",
        (DetailShortcutContext::Api, false, false) => "tui.status_bar.shortcuts_detail_api_ascii",
        (DetailShortcutContext::Api, true, true) => "tui.status_bar.shortcuts_trash_detail_api",
        (DetailShortcutContext::Api, true, false) => {
            "tui.status_bar.shortcuts_trash_detail_api_ascii"
        }
        (DetailShortcutContext::Ssh, false, true) => "tui.status_bar.shortcuts_detail_ssh",
        (DetailShortcutContext::Ssh, false, false) => "tui.status_bar.shortcuts_detail_ssh_ascii",
        (DetailShortcutContext::Ssh, true, true) => "tui.status_bar.shortcuts_trash_detail_ssh",
        (DetailShortcutContext::Ssh, true, false) => {
            "tui.status_bar.shortcuts_trash_detail_ssh_ascii"
        }
        (DetailShortcutContext::SecureNote, false, true) => {
            "tui.status_bar.shortcuts_detail_secure_note"
        }
        (DetailShortcutContext::SecureNote, false, false) => {
            "tui.status_bar.shortcuts_detail_secure_note_ascii"
        }
        (DetailShortcutContext::SecureNote, true, true) => {
            "tui.status_bar.shortcuts_trash_detail_secure_note"
        }
        (DetailShortcutContext::SecureNote, true, false) => {
            "tui.status_bar.shortcuts_trash_detail_secure_note_ascii"
        }
    };
    t!(key).to_string()
}

/// Return the visual mode shortcut text.
fn visual_shortcuts_text(unicode: bool, is_trash: bool) -> String {
    if is_trash {
        if unicode {
            t!("tui.status_bar.shortcuts_visual_trash").to_string()
        } else {
            t!("tui.status_bar.shortcuts_visual_trash_ascii").to_string()
        }
    } else if unicode {
        t!("tui.status_bar.shortcuts_visual").to_string()
    } else {
        t!("tui.status_bar.shortcuts_visual_ascii").to_string()
    }
}

/// Return the sync indicator display string.
fn sync_indicator_text(sync: &SyncIndicator, unicode: bool) -> String {
    match sync {
        SyncIndicator::Configured => {
            let text = t!("tui.status_bar.sync_configured");
            if unicode {
                format!("{} {}", theme::ICON_SUCCESS, text)
            } else {
                format!("+ {}", text)
            }
        }
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
        SyncIndicator::Configured => theme::NL_SUCCESS,
        SyncIndicator::Synced => theme::NL_SUCCESS,
        SyncIndicator::Syncing => theme::NL_CYAN,
        SyncIndicator::Failed => theme::NL_DANGER,
        SyncIndicator::Offline => theme::NL_HOT,
        SyncIndicator::NotConfigured => theme::NL_TEXT_MUTED,
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
        let text = shortcuts_text(PanelId::Sidebar, true, false, DetailShortcutContext::Login);
        assert!(text.contains("Ctrl+K"));
    }

    #[test]
    fn shortcuts_list_same_as_sidebar() {
        assert_eq!(
            shortcuts_text(PanelId::Sidebar, true, false, DetailShortcutContext::Login),
            shortcuts_text(PanelId::List, true, false, DetailShortcutContext::Login)
        );
    }

    #[test]
    fn shortcuts_detail_unicode() {
        let text = shortcuts_text(PanelId::Detail, true, false, DetailShortcutContext::Login);
        assert!(text.contains('c'));
    }

    #[test]
    fn shortcuts_sidebar_ascii() {
        let text = shortcuts_text(PanelId::Sidebar, false, false, DetailShortcutContext::Login);
        assert!(text.contains("Search"));
    }

    #[test]
    fn shortcuts_detail_ascii() {
        let text = shortcuts_text(PanelId::Detail, false, false, DetailShortcutContext::Login);
        assert!(text.contains("CopyPwd") || text.contains("复制密码"));
    }

    #[test]
    fn secure_note_detail_shortcuts_omit_copy_and_toggle_actions() {
        let text = shortcuts_text(
            PanelId::Detail,
            true,
            false,
            DetailShortcutContext::SecureNote,
        );
        assert!(!text.contains('\u{590D}'), "should not mention copy");
        assert!(
            !text.contains("\u{663E}\u{793A}"),
            "should not mention show/hide"
        );
        assert!(text.contains('e'));
        assert!(text.contains('d'));
    }

    #[test]
    fn api_detail_shortcuts_use_api_field_names() {
        let text = shortcuts_text(PanelId::Detail, true, false, DetailShortcutContext::Api);
        assert!(text.contains("Secret Key"));
        assert!(text.contains("App ID"));
        assert!(!text.contains("\u{7528}\u{6237}\u{540D}"));
        assert!(!text.contains("\u{5BC6}\u{7801}"));
    }

    #[test]
    fn ssh_detail_shortcuts_use_ssh_field_names() {
        let text = shortcuts_text(PanelId::Detail, true, false, DetailShortcutContext::Ssh);
        assert!(text.contains("\u{79C1}\u{94A5}") || text.contains("Private Key"));
        assert!(text.contains("\u{516C}\u{94A5}") || text.contains("Public Key"));
        assert!(!text.contains("\u{7528}\u{6237}\u{540D}"));
        assert!(!text.contains("\u{5BC6}\u{7801}"));
    }

    #[test]
    fn sync_indicator_synced_unicode() {
        let text = sync_indicator_text(&SyncIndicator::Synced, true);
        assert!(text.starts_with('\u{2713}')); // ✓
    }

    #[test]
    fn sync_indicator_configured_unicode() {
        let text = sync_indicator_text(&SyncIndicator::Configured, true);
        assert!(text.contains(t!("tui.status_bar.sync_configured").as_ref()));
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
            sync_indicator_text(&SyncIndicator::Configured, false),
            "+ Configured"
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
        assert_eq!(sync_color(&SyncIndicator::Configured), theme::NL_SUCCESS);
        assert_eq!(sync_color(&SyncIndicator::Synced), theme::NL_SUCCESS);
        assert_eq!(sync_color(&SyncIndicator::Syncing), theme::NL_CYAN);
        assert_eq!(sync_color(&SyncIndicator::Failed), theme::NL_DANGER);
        assert_eq!(sync_color(&SyncIndicator::Offline), theme::NL_HOT);
        assert_eq!(
            sync_color(&SyncIndicator::NotConfigured),
            theme::NL_TEXT_MUTED
        );
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
        assert_eq!(VERSION, concat!("v", env!("CARGO_PKG_VERSION")));
    }

    // ── Trash-aware status bar tests ─────────────────────────────────────────

    #[test]
    fn trash_list_shortcuts_unicode() {
        let text = shortcuts_text(PanelId::List, true, true, DetailShortcutContext::Login);
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
        let text = shortcuts_text(PanelId::Detail, true, true, DetailShortcutContext::Login);
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
        let text = shortcuts_text(PanelId::List, true, false, DetailShortcutContext::Login);
        assert!(
            !text.contains("\u{6E05}\u{7A7A}\u{56DE}\u{6536}\u{7AD9}"),
            "normal list should not contain '清空回收站'"
        );
    }

    #[test]
    fn normal_detail_shortcuts_no_trash() {
        let text = shortcuts_text(PanelId::Detail, true, false, DetailShortcutContext::Login);
        assert!(
            !text.contains("\u{6062}\u{590D}"),
            "normal detail should not contain '恢复'"
        );
    }

    #[test]
    fn visual_mode_shortcuts_shown_when_visual_active() {
        let text = visual_shortcuts_text(true, false);
        assert!(
            text.contains("Space") || text.contains("Space\u{9009}\u{62E9}"),
            "visual shortcuts should mention Space"
        );
    }

    #[test]
    fn trash_visual_shortcuts_show_restore_and_hard_delete() {
        let text = visual_shortcuts_text(true, true);
        assert!(
            !text.contains("BatchDel") && !text.contains("BatchTag"),
            "trash visual shortcuts should not show BatchDel/BatchTag"
        );
    }

    #[test]
    fn normal_shortcuts_shown_when_not_visual() {
        let text = shortcuts_text(PanelId::List, true, false, DetailShortcutContext::Login);
        assert!(
            text.is_empty() || !text.contains("Space\u{9009}\u{62E9}"),
            "non-visual should not show Space shortcut"
        );
    }
}
