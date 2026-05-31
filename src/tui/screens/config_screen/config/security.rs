use ratatui::layout::Constraint;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{layout::Rect, Frame};

use crate::config::HealthCheckFrequency;
use crate::t;
use crate::tui::state::config_state::SecurityConfigForm;
use crate::tui::theme;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    form: &SecurityConfigForm,
    focused: usize,
    sub_item_focus: Option<usize>,
) {
    let chunks = super::render::vertical_chunks(
        area,
        &[
            Constraint::Length(2), // Title
            Constraint::Length(1), // Health check
            Constraint::Length(1), // Check frequency
            Constraint::Length(1), // Master password
            Constraint::Length(1), // Audit
            Constraint::Length(1), // Audit retention
            Constraint::Min(0),
        ],
    );

    super::render::render_section_title(frame, chunks[0], t!("tui.config.tab_security").as_ref());
    super::render::render_setting_row(
        frame,
        chunks[1],
        theme::NF_SECURITY_ISSUES,
        t!("tui.config.health_check").as_ref(),
        &super::render::switch_control(form.health_check_enabled),
        focused == 0,
        true,
    );

    let freq_label = match form.health_check_frequency {
        HealthCheckFrequency::OnStartup => t!("tui.config.frequency_on_startup").to_string(),
        HealthCheckFrequency::Daily => t!("tui.config.frequency_daily").to_string(),
        HealthCheckFrequency::Weekly => t!("tui.config.frequency_weekly").to_string(),
    };
    super::render::render_setting_row(
        frame,
        chunks[2],
        theme::NF_CLOCK,
        t!("tui.config.check_frequency").as_ref(),
        &super::render::dropdown_control(&freq_label),
        focused == 1,
        true,
    );

    super::render::render_setting_row(
        frame,
        chunks[3],
        theme::NF_LOCK,
        t!("tui.config.master_password").as_ref(),
        &super::render::plain_control(&format!(
            "{}   {}",
            t!("tui.config.master_password_masked"),
            t!("tui.config.change_password")
        )),
        focused == 2,
        true,
    );

    render_audit_row(
        frame,
        chunks[4],
        form.audit_enabled,
        focused == 3,
        sub_item_focus,
    );
    super::render::render_setting_row(
        frame,
        chunks[5],
        theme::NF_CLOCK,
        t!("tui.config.audit_retention").as_ref(),
        &super::render::dropdown_control(&t!("tui.config.days", n = form.audit_retention_days)),
        focused == 4,
        true,
    );
}

fn render_audit_row(
    frame: &mut Frame,
    area: Rect,
    enabled: bool,
    focused: bool,
    sub_item_focus: Option<usize>,
) {
    let row_style = super::render::row_style(focused);
    let bg = row_style.bg.unwrap_or(theme::NL_BG);
    let toggle_style = if focused && sub_item_focus.unwrap_or(0) == 0 {
        Style::default()
            .fg(theme::NL_CYAN)
            .bg(bg)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default().fg(theme::NL_TEXT).bg(bg)
    };
    let link_style = if focused && sub_item_focus.unwrap_or(0) == 1 {
        Style::default()
            .fg(theme::NL_CYAN)
            .bg(bg)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default().fg(theme::NL_TEXT).bg(bg)
    };

    let toggle = if enabled {
        format!("{} {}", theme::ICON_SUCCESS, t!("tui.config.enabled"))
    } else {
        format!("{} {}", theme::ICON_ERROR, t!("tui.config.disabled"))
    };
    let line = Line::from(vec![
        Span::styled(
            format!("  {:<8}", theme::NF_NOTE),
            Style::default().fg(theme::NL_CYAN).bg(bg),
        ),
        Span::styled(
            t!("tui.config.audit").to_string(),
            Style::default().fg(theme::NL_TEXT).bg(bg),
        ),
        Span::styled("        ", Style::default().bg(bg)),
        Span::styled(format!("[ {} ]", toggle), toggle_style),
        Span::styled("  ", Style::default().bg(bg)),
        Span::styled(
            format!("[ {} ]", t!("tui.config.view_audit_log")),
            link_style,
        ),
    ]);
    frame.render_widget(Paragraph::new(line).style(row_style), area);
}
