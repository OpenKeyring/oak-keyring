use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, layout::Rect};

use crate::config::HealthCheckFrequency;
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
    let title = Paragraph::new("安全").style(dim_style);
    frame.render_widget(title, chunks[0]);

    // Row index 0: Health check (focused == 0)
    let health = format!(
        "健康检查            [ {} ]",
        if form.health_check_enabled {
            "\u{2713} 已开启"
        } else {
            "\u{2717} 已关闭"
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
        HealthCheckFrequency::OnStartup => "启动时",
        HealthCheckFrequency::Daily => "每天",
        HealthCheckFrequency::Weekly => "每周",
    };
    let freq = format!("检查频率            [ {} \u{25bc} ]", freq_label);
    frame.render_widget(
        Paragraph::new(freq).style(if focused == 1 {
            focused_style
        } else {
            normal_style
        }),
        chunks[2],
    );

    // Row index 2: Master password (focused == 2)
    let pwd = "主密码              [\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}]        [ 修改 ]";
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
        "操作审计            [ {} ]  [ 查看记录 ]",
        if form.audit_enabled {
            "\u{2713} 已开启"
        } else {
            "\u{2717} 已关闭"
        }
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
        "审计保留时长        [ {}天 \u{25bc} ]",
        form.audit_retention_days
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
