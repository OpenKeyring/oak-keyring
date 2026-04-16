use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, layout::Rect};

use crate::config::HealthCheckFrequency;
use crate::tui::state::config_state::SecurityConfigForm;

pub fn render(frame: &mut Frame, area: Rect, form: &SecurityConfigForm) {
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

    let title = Paragraph::new("安全").style(Style::default().fg(Color::Rgb(86, 95, 137)).bold());
    frame.render_widget(title, chunks[0]);

    let health = format!(
        "健康检查            [ {} ]",
        if form.health_check_enabled {
            "\u{2713} 已开启"
        } else {
            "\u{2717} 已关闭"
        }
    );
    frame.render_widget(
        Paragraph::new(health).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[1],
    );

    let freq_label = match form.health_check_frequency {
        HealthCheckFrequency::OnStartup => "启动时",
        HealthCheckFrequency::Daily => "每天",
        HealthCheckFrequency::Weekly => "每周",
    };
    let freq = format!("检查频率            [ {} \u{25bc} ]", freq_label);
    frame.render_widget(
        Paragraph::new(freq).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[2],
    );

    let pwd = "主密码              [\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}]        [ 修改 ]";
    frame.render_widget(
        Paragraph::new(pwd).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[3],
    );

    let audit = format!(
        "操作审计            [ {} ]  [ 查看记录 ]",
        if form.audit_enabled {
            "\u{2713} 已开启"
        } else {
            "\u{2717} 已关闭"
        }
    );
    frame.render_widget(
        Paragraph::new(audit).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[4],
    );

    let retention = format!(
        "审计保留时长        [ {}天 \u{25bc} ]",
        form.audit_retention_days
    );
    frame.render_widget(
        Paragraph::new(retention).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[5],
    );
}
