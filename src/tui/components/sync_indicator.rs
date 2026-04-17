//! Sync status indicator widget for the status bar (U10).

use chrono::Utc;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::state::sync_ui_state::{SyncDisplayStatus, SyncIndicatorState};

/// A reusable sync-status indicator that can render into a ratatui frame.
pub struct SyncIndicator<'a> {
    state: &'a SyncIndicatorState,
}

impl<'a> SyncIndicator<'a> {
    pub fn new(state: &'a SyncIndicatorState) -> Self {
        Self { state }
    }

    /// Render just the icon glyph (for compact status bar display).
    pub fn render_icon(self, f: &mut Frame, area: Rect) {
        let icon = self.current_icon();
        let color = self.status_color();
        let para = Paragraph::new(icon).style(Style::default().fg(color));
        f.render_widget(para, area);
    }

    /// Render icon followed by detail text (for expanded status bar area).
    pub fn render_with_detail(self, f: &mut Frame, area: Rect) {
        let icon = self.current_icon();
        let color = self.status_color();
        let detail = self.detail_text();

        let text = format!("{} {}", icon, detail);
        let para = Paragraph::new(text).style(Style::default().fg(color));
        f.render_widget(para, area);
    }

    /// Return the current icon glyph, accounting for animation frames during
    /// active sync.
    pub fn current_icon(&self) -> &'static str {
        match self.state.status {
            SyncDisplayStatus::Syncing => {
                if self.state.animation_frame.is_multiple_of(2) {
                    "\u{27F3}"
                } else {
                    "\u{27F2}"
                }
            }
            _ => self.state.status.icon(),
        }
    }

    fn status_color(&self) -> Color {
        match self.state.status {
            SyncDisplayStatus::Synced => Color::Green,
            SyncDisplayStatus::Syncing => Color::Blue,
            SyncDisplayStatus::Failed => Color::Red,
            SyncDisplayStatus::NotConfigured => Color::DarkGray,
            SyncDisplayStatus::Offline => Color::Yellow,
            SyncDisplayStatus::Rotating => Color::Magenta,
        }
    }

    fn detail_text(&self) -> String {
        match self.state.status {
            SyncDisplayStatus::Synced => {
                if let Some(last_sync) = self.state.last_sync {
                    format_relative_time(last_sync)
                } else {
                    "已同步".to_string()
                }
            }
            SyncDisplayStatus::Syncing => {
                if let Some(ref p) = self.state.progress {
                    format!("同步中 {}/{}", p.current, p.total)
                } else {
                    "同步中...".to_string()
                }
            }
            SyncDisplayStatus::Failed => {
                format!(
                    "同步失败: {}",
                    self.state.error_message.as_deref().unwrap_or("未知错误")
                )
            }
            SyncDisplayStatus::Offline => "网络不可达".to_string(),
            SyncDisplayStatus::NotConfigured => "未配置同步".to_string(),
            SyncDisplayStatus::Rotating => {
                if let Some(ref p) = self.state.progress {
                    format!("密钥轮换中 {}/{}", p.current, p.total)
                } else {
                    "密钥轮换中...".to_string()
                }
            }
        }
    }
}

/// Format a `DateTime<Utc>` as a human-readable relative-time string (Chinese).
fn format_relative_time(dt: chrono::DateTime<Utc>) -> String {
    let now = Utc::now();
    let dur = now.signed_duration_since(dt);
    if dur.num_seconds() < 60 {
        "刚刚".to_string()
    } else if dur.num_minutes() < 60 {
        format!("{}分钟前", dur.num_minutes())
    } else if dur.num_hours() < 24 {
        format!("{}小时前", dur.num_hours())
    } else {
        format!("{}天前", dur.num_days())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::sync_ui_state::{SyncDisplayStatus, SyncProgress};

    #[test]
    fn indicator_status_color_synced_is_green() {
        let state = SyncIndicatorState {
            status: SyncDisplayStatus::Synced,
            ..Default::default()
        };
        let indicator = SyncIndicator::new(&state);
        assert_eq!(indicator.status_color(), Color::Green);
    }

    #[test]
    fn indicator_status_color_syncing_is_blue() {
        let state = SyncIndicatorState {
            status: SyncDisplayStatus::Syncing,
            ..Default::default()
        };
        let indicator = SyncIndicator::new(&state);
        assert_eq!(indicator.status_color(), Color::Blue);
    }

    #[test]
    fn indicator_status_color_failed_is_red() {
        let state = SyncIndicatorState {
            status: SyncDisplayStatus::Failed,
            ..Default::default()
        };
        let indicator = SyncIndicator::new(&state);
        assert_eq!(indicator.status_color(), Color::Red);
    }

    #[test]
    fn indicator_status_color_offline_is_yellow() {
        let state = SyncIndicatorState {
            status: SyncDisplayStatus::Offline,
            ..Default::default()
        };
        let indicator = SyncIndicator::new(&state);
        assert_eq!(indicator.status_color(), Color::Yellow);
    }

    #[test]
    fn indicator_status_color_rotating_is_magenta() {
        let state = SyncIndicatorState {
            status: SyncDisplayStatus::Rotating,
            ..Default::default()
        };
        let indicator = SyncIndicator::new(&state);
        assert_eq!(indicator.status_color(), Color::Magenta);
    }

    #[test]
    fn indicator_status_color_not_configured_is_dark_gray() {
        let state = SyncIndicatorState {
            status: SyncDisplayStatus::NotConfigured,
            ..Default::default()
        };
        let indicator = SyncIndicator::new(&state);
        assert_eq!(indicator.status_color(), Color::DarkGray);
    }

    #[test]
    fn indicator_current_icon_animates_during_sync() {
        let state = SyncIndicatorState {
            status: SyncDisplayStatus::Syncing,
            animation_frame: 0,
            ..Default::default()
        };
        let indicator = SyncIndicator::new(&state);
        assert_eq!(indicator.current_icon(), "\u{27F3}");

        let state = SyncIndicatorState {
            status: SyncDisplayStatus::Syncing,
            animation_frame: 1,
            ..Default::default()
        };
        let indicator = SyncIndicator::new(&state);
        assert_eq!(indicator.current_icon(), "\u{27F2}");
    }

    #[test]
    fn indicator_current_icon_static_for_other_statuses() {
        for (status, expected) in [
            (SyncDisplayStatus::Synced, "\u{2713}"),
            (SyncDisplayStatus::Failed, "\u{2717}"),
            (SyncDisplayStatus::NotConfigured, "\u{2014}"),
            (SyncDisplayStatus::Offline, "\u{25D0}"),
            (SyncDisplayStatus::Rotating, "\u{27F2}"),
        ] {
            let state = SyncIndicatorState {
                status,
                ..Default::default()
            };
            let indicator = SyncIndicator::new(&state);
            assert_eq!(indicator.current_icon(), expected);
        }
    }

    #[test]
    fn indicator_detail_text_synced_without_last_sync() {
        let state = SyncIndicatorState {
            status: SyncDisplayStatus::Synced,
            last_sync: None,
            ..Default::default()
        };
        let indicator = SyncIndicator::new(&state);
        assert_eq!(indicator.detail_text(), "已同步");
    }

    #[test]
    fn indicator_detail_text_syncing_with_progress() {
        let state = SyncIndicatorState {
            status: SyncDisplayStatus::Syncing,
            progress: Some(SyncProgress { current: 3, total: 10 }),
            ..Default::default()
        };
        let indicator = SyncIndicator::new(&state);
        assert_eq!(indicator.detail_text(), "同步中 3/10");
    }

    #[test]
    fn indicator_detail_text_syncing_without_progress() {
        let state = SyncIndicatorState {
            status: SyncDisplayStatus::Syncing,
            progress: None,
            ..Default::default()
        };
        let indicator = SyncIndicator::new(&state);
        assert_eq!(indicator.detail_text(), "同步中...");
    }

    #[test]
    fn indicator_detail_text_failed_with_error() {
        let state = SyncIndicatorState {
            status: SyncDisplayStatus::Failed,
            error_message: Some("connection refused".to_string()),
            ..Default::default()
        };
        let indicator = SyncIndicator::new(&state);
        assert_eq!(indicator.detail_text(), "同步失败: connection refused");
    }

    #[test]
    fn indicator_detail_text_failed_without_error() {
        let state = SyncIndicatorState {
            status: SyncDisplayStatus::Failed,
            error_message: None,
            ..Default::default()
        };
        let indicator = SyncIndicator::new(&state);
        assert_eq!(indicator.detail_text(), "同步失败: 未知错误");
    }

    #[test]
    fn indicator_detail_text_offline() {
        let state = SyncIndicatorState {
            status: SyncDisplayStatus::Offline,
            ..Default::default()
        };
        let indicator = SyncIndicator::new(&state);
        assert_eq!(indicator.detail_text(), "网络不可达");
    }

    #[test]
    fn indicator_detail_text_not_configured() {
        let state = SyncIndicatorState {
            status: SyncDisplayStatus::NotConfigured,
            ..Default::default()
        };
        let indicator = SyncIndicator::new(&state);
        assert_eq!(indicator.detail_text(), "未配置同步");
    }

    #[test]
    fn indicator_detail_text_rotating_with_progress() {
        let state = SyncIndicatorState {
            status: SyncDisplayStatus::Rotating,
            progress: Some(SyncProgress { current: 50, total: 100 }),
            ..Default::default()
        };
        let indicator = SyncIndicator::new(&state);
        assert_eq!(indicator.detail_text(), "密钥轮换中 50/100");
    }

    #[test]
    fn indicator_detail_text_rotating_without_progress() {
        let state = SyncIndicatorState {
            status: SyncDisplayStatus::Rotating,
            progress: None,
            ..Default::default()
        };
        let indicator = SyncIndicator::new(&state);
        assert_eq!(indicator.detail_text(), "密钥轮换中...");
    }

    #[test]
    fn format_relative_time_just_now() {
        let now = Utc::now();
        let result = format_relative_time(now);
        assert_eq!(result, "刚刚");
    }

    #[test]
    fn format_relative_time_minutes_ago() {
        let dt = Utc::now() - chrono::Duration::minutes(5);
        let result = format_relative_time(dt);
        assert_eq!(result, "5分钟前");
    }

    #[test]
    fn format_relative_time_hours_ago() {
        let dt = Utc::now() - chrono::Duration::hours(3);
        let result = format_relative_time(dt);
        assert_eq!(result, "3小时前");
    }

    #[test]
    fn format_relative_time_days_ago() {
        let dt = Utc::now() - chrono::Duration::days(7);
        let result = format_relative_time(dt);
        assert_eq!(result, "7天前");
    }
}
