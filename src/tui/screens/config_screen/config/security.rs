use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Paragraph;
use ratatui::{layout::Rect, Frame};

use crate::config::HealthCheckFrequency;
use crate::t;
use crate::tui::state::config_state::SecurityConfigForm;
use crate::tui::theme;

pub fn render(frame: &mut Frame, area: Rect, form: &SecurityConfigForm, focused: usize) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Title
            Constraint::Length(1), // Health check
            Constraint::Length(1), // Check frequency
            Constraint::Length(1), // Master password
            Constraint::Length(1), // Audit
            Constraint::Length(1), // Audit retention
        ])
        .split(area);

    let dim_style = Style::default().fg(theme::TEXT_SECONDARY).bold();
    let normal_style = Style::default().fg(theme::TEXT);
    let focused_style = Style::default()
        .fg(theme::TEXT)
        .add_modifier(Modifier::BOLD)
        .bg(theme::BG_SURFACE);

    // Title row is NOT focusable
    let title = Paragraph::new(t!("tui.config.tab_security").to_string()).style(dim_style);
    frame.render_widget(title, chunks[0]);

    // Row index 0: Health check (focused == 0)
    let health = format!(
        "{}            [ {} ]",
        t!("tui.config.health_check"),
        if form.health_check_enabled {
            format!("\u{2713} {}", t!("tui.config.enabled"))
        } else {
            format!("\u{2717} {}", t!("tui.config.disabled"))
        }
    );
    frame.render_widget(
        Paragraph::new(health).style(if focused == 0 {
            focused_style
        } else {
            normal_style
        }),
        chunks[1],
    );

    // Row index 1: Check frequency (focused == 1)
    let freq_label = match form.health_check_frequency {
        HealthCheckFrequency::OnStartup => t!("tui.config.frequency_on_startup").to_string(),
        HealthCheckFrequency::Daily => t!("tui.config.frequency_daily").to_string(),
        HealthCheckFrequency::Weekly => t!("tui.config.frequency_weekly").to_string(),
    };
    let freq = format!(
        "{}            [ {} \u{25bc} ]",
        t!("tui.config.check_frequency"),
        freq_label
    );
    frame.render_widget(
        Paragraph::new(freq).style(if focused == 1 {
            focused_style
        } else {
            normal_style
        }),
        chunks[2],
    );

    // Row index 2: Master password (focused == 2)
    let pwd = format!(
        "{}              [{}]        [ {} ]",
        t!("tui.config.master_password"),
        t!("tui.config.master_password_masked"),
        t!("tui.config.change_password")
    );
    frame.render_widget(
        Paragraph::new(pwd).style(if focused == 2 {
            focused_style
        } else {
            normal_style
        }),
        chunks[3],
    );

    // Row index 3: Audit (focused == 3)
    let audit = format!(
        "{}            [ {} ]  [ {} ]",
        t!("tui.config.audit"),
        if form.audit_enabled {
            format!("\u{2713} {}", t!("tui.config.enabled"))
        } else {
            format!("\u{2717} {}", t!("tui.config.disabled"))
        },
        t!("tui.config.view_audit_log")
    );
    frame.render_widget(
        Paragraph::new(audit).style(if focused == 3 {
            focused_style
        } else {
            normal_style
        }),
        chunks[4],
    );

    // Row index 4: Audit retention (focused == 4)
    let retention = format!(
        "{}        [ {} \u{25bc} ]",
        t!("tui.config.audit_retention"),
        t!("tui.config.days", n = form.audit_retention_days)
    );
    frame.render_widget(
        Paragraph::new(retention).style(if focused == 4 {
            focused_style
        } else {
            normal_style
        }),
        chunks[5],
    );
}
