use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, layout::Rect};

use crate::config::{SyncMode, SyncProvider};
use crate::tui::state::config_state::{SyncConfigForm, SyncConnectionStatus};

pub fn render(frame: &mut Frame, area: Rect, form: &SyncConfigForm, status: SyncConnectionStatus) {
    let constraints = vec![
        Constraint::Length(1), // Title
        Constraint::Length(1), // Provider
        Constraint::Length(1), // Sync mode
        Constraint::Length(1), // Auto interval
        Constraint::Min(3),    // Provider info/config area
        Constraint::Length(1), // Status
    ];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let title = Paragraph::new("同步").style(Style::default().fg(Color::Rgb(86, 95, 137)).bold());
    frame.render_widget(title, chunks[0]);

    let provider_name = match form.provider {
        SyncProvider::Disabled => "禁用",
        SyncProvider::ICloud => "iCloud Drive",
        SyncProvider::GoogleDrive => "Google Drive",
        SyncProvider::Dropbox => "Dropbox",
        SyncProvider::OneDrive => "OneDrive",
        SyncProvider::WebDav => "WebDAV",
        SyncProvider::Sftp => "SFTP",
        SyncProvider::S3 => "S3 兼容",
        SyncProvider::AliyunDrive => "阿里云盘",
        SyncProvider::AliyunOss => "阿里云 OSS",
        SyncProvider::TencentCos => "腾讯云 COS",
        SyncProvider::HuaweiObs => "华为云 OBS",
        SyncProvider::Upyun => "又拍云",
    };
    let provider = format!("云同步 Provider     [ {} \u{25bc} ]", provider_name);
    frame.render_widget(
        Paragraph::new(provider).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[1],
    );

    let mode_label = match form.sync_mode {
        SyncMode::Auto => "自动",
        SyncMode::Manual => "手动",
    };
    let mode = format!("同步方式            [ {} \u{25bc} ]", mode_label);
    frame.render_widget(
        Paragraph::new(mode).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[2],
    );

    let interval = format!(
        "自动同步间隔        [ {}秒 \u{25bc} ]",
        form.auto_interval_seconds
    );
    frame.render_widget(
        Paragraph::new(interval).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[3],
    );

    // Provider info area
    if form.provider == SyncProvider::Disabled {
        let info = Paragraph::new("未配置云同步。选择 Provider 以开始。")
            .style(Style::default().fg(Color::Rgb(86, 95, 137)));
        frame.render_widget(info, chunks[4]);
    } else if form.provider == SyncProvider::ICloud {
        let info = Paragraph::new("iCloud Drive 无需额外配置，同步目录自动管理。")
            .style(Style::default().fg(Color::Rgb(86, 95, 137)));
        frame.render_widget(info, chunks[4]);
    }
    // For other providers, provider_config details would be rendered here (Task 4 will expand this)

    // Status
    let (status_text, status_color) = match status {
        SyncConnectionStatus::Connected => ("\u{2713} 已连接", Color::Rgb(158, 206, 106)),
        SyncConnectionStatus::Disconnected => ("\u{2717} 未连接", Color::Rgb(247, 118, 142)),
        SyncConnectionStatus::NotConfigured => ("\u{2014} 未配置", Color::Rgb(59, 66, 97)),
        SyncConnectionStatus::Testing => ("\u{27f3} 测试连接中...", Color::Rgb(122, 162, 247)),
    };
    let status_widget = Paragraph::new(format!("同步状态            {}", status_text))
        .style(Style::default().fg(status_color));
    frame.render_widget(status_widget, chunks[5]);
}
