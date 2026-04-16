use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::config::{AliyunDriveType, ProviderConfig, SftpHostCheck, SyncMode, SyncProvider};
use crate::tui::state::config_state::{SyncConfigForm, SyncConnectionStatus};
use crate::tui::theme;

// ── Color palette (for provider-specific field area, not focusable) ────────

const LABEL: ratatui::style::Color = ratatui::style::Color::Rgb(86, 95, 137);
const VALUE: ratatui::style::Color = ratatui::style::Color::Rgb(192, 202, 245);
const DIVIDER: ratatui::style::Color = ratatui::style::Color::Rgb(41, 46, 66);

// ── Masking helpers ────────────────────────────────────────────────────────

fn mask(value: &str) -> &'static str {
    if value.is_empty() {
        "(未设置)"
    } else {
        "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"
    }
}

fn mask_opt(value: &Option<String>) -> &'static str {
    match value {
        Some(v) if !v.is_empty() => {
            "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"
        }
        _ => "(未设置)",
    }
}

// ── Main render ────────────────────────────────────────────────────────────

pub fn render(
    frame: &mut Frame,
    area: Rect,
    form: &SyncConfigForm,
    status: SyncConnectionStatus,
    focused: usize,
) {
    let dim_style = Style::default().fg(theme::TEXT_SECONDARY).bold();
    let normal_style = Style::default().fg(theme::TEXT);
    let focused_style = Style::default()
        .fg(theme::TEXT)
        .add_modifier(Modifier::BOLD)
        .bg(theme::BG_SURFACE);

    // Count provider-specific field rows so we can size the layout dynamically.
    let field_count = provider_field_count(&form.provider_config);

    // Build constraints: title, provider, mode, interval, divider, N fields, button+status
    let mut constraints: Vec<Constraint> = Vec::with_capacity(6 + field_count as usize);
    constraints.push(Constraint::Length(1)); // Title
    constraints.push(Constraint::Length(1)); // Provider
    constraints.push(Constraint::Length(1)); // Sync mode
    constraints.push(Constraint::Length(1)); // Auto interval
    constraints.push(Constraint::Length(1)); // Divider

    if field_count > 0 {
        constraints.push(Constraint::Length(field_count)); // Provider fields
    }

    constraints.push(Constraint::Length(1)); // Test button + status
    constraints.push(Constraint::Min(0)); // Remainder

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut row = 0;

    // ── Title (not focusable) ─────────────────────────────────────────────
    frame.render_widget(
        Paragraph::new("同步").style(dim_style),
        chunks[row],
    );
    row += 1;

    // ── Provider dropdown (focus index 0) ─────────────────────────────────
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
    frame.render_widget(
        Paragraph::new(format!(
            "云同步 Provider     [ {} \u{25bc} ]",
            provider_name
        ))
        .style(if focused == 0 {
            focused_style
        } else {
            normal_style
        }),
        chunks[row],
    );
    row += 1;

    // ── Sync mode (focus index 1) ─────────────────────────────────────────
    let mode_label = match form.sync_mode {
        SyncMode::Auto => "自动",
        SyncMode::Manual => "手动",
    };
    frame.render_widget(
        Paragraph::new(format!(
            "同步方式            [ {} \u{25bc} ]",
            mode_label
        ))
        .style(if focused == 1 {
            focused_style
        } else {
            normal_style
        }),
        chunks[row],
    );
    row += 1;

    // ── Auto interval (focus index 2) ─────────────────────────────────────
    let interval_style = if form.sync_mode == SyncMode::Manual {
        Style::default()
            .fg(theme::TEXT_MUTED)
            .add_modifier(Modifier::DIM)
    } else if focused == 2 {
        focused_style
    } else {
        normal_style
    };
    frame.render_widget(
        Paragraph::new(format!(
            "自动同步间隔        [ {}秒 \u{25bc} ]",
            form.auto_interval_seconds
        ))
        .style(interval_style),
        chunks[row],
    );
    row += 1;

    // ── Divider (not focusable) ───────────────────────────────────────────
    let divider_width = area.width as usize;
    let divider_text: String = "\u{2500}".repeat(divider_width);
    frame.render_widget(
        Paragraph::new(divider_text).style(Style::default().fg(DIVIDER)),
        chunks[row],
    );
    row += 1;

    // ── Provider-specific fields (not focusable) ──────────────────────────
    if field_count > 0 {
        let field_area = chunks[row];
        let field_constraints = (0..field_count)
            .map(|_| Constraint::Length(1))
            .collect::<Vec<_>>();
        let field_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(field_constraints)
            .split(field_area);

        let mut fi = 0u16;
        render_provider_fields(&form.provider_config, &field_chunks, &mut fi, frame);
        row += 1;
    }

    // ── Test button + status (focus index 3) ──────────────────────────────
    let (status_text, status_color) = match status {
        SyncConnectionStatus::Connected => ("\u{2713} 已连接", ratatui::style::Color::Rgb(158, 206, 106)),
        SyncConnectionStatus::Disconnected => ("\u{2717} 未连接", ratatui::style::Color::Rgb(247, 118, 142)),
        SyncConnectionStatus::NotConfigured => ("\u{2014} 未配置", ratatui::style::Color::Rgb(59, 66, 97)),
        SyncConnectionStatus::Testing => ("\u{27f3} 测试连接中...", theme::PRIMARY),
    };

    let status_line = format!(
        "[ 测试连接 ]   同步状态  {}",
        status_text
    );
    frame.render_widget(
        Paragraph::new(status_line).style(if focused == 3 {
            focused_style
        } else {
            Style::default().fg(status_color)
        }),
        chunks[row],
    );
}

// ── Field count per provider ───────────────────────────────────────────────

fn provider_field_count(pc: &Option<ProviderConfig>) -> u16 {
    match pc {
        None => 0, // Disabled or not configured yet
        Some(ProviderConfig::ICloud) => 1, // just a hint line
        Some(ProviderConfig::GoogleDrive(_)) => 4,
        Some(ProviderConfig::Dropbox(_)) => 4,
        Some(ProviderConfig::OneDrive(_)) => 4,
        Some(ProviderConfig::WebDav(_)) => 5,
        Some(ProviderConfig::Sftp(_)) => 4,
        Some(ProviderConfig::S3(_)) => 6,
        Some(ProviderConfig::AliyunDrive(_)) => 5,
        Some(ProviderConfig::AliyunOss(_)) => 5,
        Some(ProviderConfig::TencentCos(_)) => 5,
        Some(ProviderConfig::HuaweiObs(_)) => 5,
        Some(ProviderConfig::Upyun(_)) => 4,
    }
}

// ── Provider-specific field rendering ──────────────────────────────────────

fn render_provider_fields(
    pc: &Option<ProviderConfig>,
    chunks: &[Rect],
    fi: &mut u16,
    frame: &mut Frame,
) {
    match pc {
        None => {
            render_label_value(chunks, fi, frame, "未配置云同步。选择 Provider 以开始。", "", LABEL);
        }
        Some(ProviderConfig::ICloud) => {
            render_label_value(chunks, fi, frame, "iCloud Drive 无需额外配置，同步目录自动管理。", "", LABEL);
        }
        Some(ProviderConfig::GoogleDrive(cfg)) => {
            render_field(chunks, fi, frame, "Client ID",        &cfg.client_id,     false);
            render_field(chunks, fi, frame, "Client Secret",    &cfg.client_secret, true);
            render_field(chunks, fi, frame, "Refresh Token",    &cfg.refresh_token, true);
            render_field(chunks, fi, frame, "Root Path",        &cfg.root_path,     false);
        }
        Some(ProviderConfig::Dropbox(cfg)) => {
            render_field(chunks, fi, frame, "Client ID",        &cfg.client_id,     false);
            render_field(chunks, fi, frame, "Client Secret",    &cfg.client_secret, true);
            render_field(chunks, fi, frame, "Refresh Token",    &cfg.refresh_token, true);
            render_field(chunks, fi, frame, "Root Path",        &cfg.root_path,     false);
        }
        Some(ProviderConfig::OneDrive(cfg)) => {
            render_field(chunks, fi, frame, "Client ID",        &cfg.client_id,     false);
            render_field(chunks, fi, frame, "Client Secret",    &cfg.client_secret, true);
            render_field(chunks, fi, frame, "Refresh Token",    &cfg.refresh_token, true);
            render_field(chunks, fi, frame, "Root Path",        &cfg.root_path,     false);
        }
        Some(ProviderConfig::WebDav(cfg)) => {
            render_field(chunks, fi, frame, "Endpoint",         &cfg.endpoint,      false);
            render_field(chunks, fi, frame, "Root Path",        &cfg.root_path,     false);
            render_field_opt(chunks, fi, frame, "Username",     &cfg.username,      false);
            render_field_opt(chunks, fi, frame, "Password",     &cfg.password,      true);
            render_field_opt(chunks, fi, frame, "Bearer Token", &cfg.bearer_token,  true);
        }
        Some(ProviderConfig::Sftp(cfg)) => {
            render_field(chunks, fi, frame, "Server",           &cfg.server,        false);
            render_field(chunks, fi, frame, "Root Path",        &cfg.root_path,     false);
            render_field(chunks, fi, frame, "SSH Key Path",    &cfg.ssh_key_path,  false);
            let host_check_str = match cfg.host_check {
                SftpHostCheck::Strict => "Strict",
                SftpHostCheck::Accept => "Accept",
                SftpHostCheck::Add => "Add",
            };
            render_label_value(chunks, fi, frame, "Host Check", host_check_str, LABEL);
        }
        Some(ProviderConfig::S3(cfg)) => {
            render_field_opt(chunks, fi, frame, "Endpoint",     &cfg.endpoint,      false);
            render_field(chunks, fi, frame, "Bucket",           &cfg.bucket,        false);
            render_field_opt(chunks, fi, frame, "Region",       &cfg.region,        false);
            render_field(chunks, fi, frame, "Access Key ID",    &cfg.access_key_id, false);
            render_field(chunks, fi, frame, "Secret Access Key",&cfg.secret_access_key, true);
            render_field(chunks, fi, frame, "Root Path",        &cfg.root_path,     false);
        }
        Some(ProviderConfig::AliyunDrive(cfg)) => {
            render_field(chunks, fi, frame, "Client ID",        &cfg.client_id,     false);
            render_field(chunks, fi, frame, "Client Secret",    &cfg.client_secret, true);
            render_field(chunks, fi, frame, "Refresh Token",    &cfg.refresh_token, true);
            let drive_type_str = match cfg.drive_type {
                AliyunDriveType::Default => "默认空间",
                AliyunDriveType::Backup => "备份空间",
                AliyunDriveType::Resource => "资源库",
            };
            render_label_value(chunks, fi, frame, "Drive Type", drive_type_str, LABEL);
            render_field(chunks, fi, frame, "Root Path",        &cfg.root_path,     false);
        }
        Some(ProviderConfig::AliyunOss(cfg)) => {
            render_field(chunks, fi, frame, "Endpoint",         &cfg.endpoint,      false);
            render_field(chunks, fi, frame, "Bucket",           &cfg.bucket,        false);
            render_field(chunks, fi, frame, "Access Key ID",    &cfg.access_key_id, false);
            render_field(chunks, fi, frame, "Access Key Secret",&cfg.access_key_secret, true);
            render_field(chunks, fi, frame, "Root Path",        &cfg.root_path,     false);
        }
        Some(ProviderConfig::TencentCos(cfg)) => {
            render_field(chunks, fi, frame, "Endpoint",         &cfg.endpoint,      false);
            render_field(chunks, fi, frame, "Bucket",           &cfg.bucket,        false);
            render_field(chunks, fi, frame, "Secret ID",        &cfg.secret_id,     false);
            render_field(chunks, fi, frame, "Secret Key",       &cfg.secret_key,    true);
            render_field(chunks, fi, frame, "Root Path",        &cfg.root_path,     false);
        }
        Some(ProviderConfig::HuaweiObs(cfg)) => {
            render_field(chunks, fi, frame, "Endpoint",         &cfg.endpoint,      false);
            render_field(chunks, fi, frame, "Bucket",           &cfg.bucket,        false);
            render_field(chunks, fi, frame, "Access Key ID",    &cfg.access_key_id, false);
            render_field(chunks, fi, frame, "Secret Access Key",&cfg.secret_access_key, true);
            render_field(chunks, fi, frame, "Root Path",        &cfg.root_path,     false);
        }
        Some(ProviderConfig::Upyun(cfg)) => {
            render_field(chunks, fi, frame, "Bucket",           &cfg.bucket,             false);
            render_field(chunks, fi, frame, "Operator",         &cfg.operator,           false);
            render_field(chunks, fi, frame, "Operator Password",&cfg.operator_password,  true);
            render_field(chunks, fi, frame, "Root Path",        &cfg.root_path,          false);
        }
    }
}

// ── Low-level helpers ──────────────────────────────────────────────────────

fn render_field(
    chunks: &[Rect],
    fi: &mut u16,
    frame: &mut Frame,
    label: &str,
    value: &str,
    sensitive: bool,
) {
    let display = if sensitive { mask(value) } else { value };
    render_label_value(chunks, fi, frame, label, display, LABEL);
}

fn render_field_opt(
    chunks: &[Rect],
    fi: &mut u16,
    frame: &mut Frame,
    label: &str,
    value: &Option<String>,
    sensitive: bool,
) {
    let display = if sensitive {
        mask_opt(value)
    } else {
        match value {
            Some(v) if !v.is_empty() => v.as_str(),
            _ => "(未设置)",
        }
    };
    render_label_value(chunks, fi, frame, label, display, LABEL);
}

fn render_label_value(
    chunks: &[Rect],
    fi: &mut u16,
    frame: &mut Frame,
    label: &str,
    value: &str,
    label_color: ratatui::style::Color,
) {
    let idx = *fi as usize;
    if idx < chunks.len() {
        let row_area = chunks[idx];
        // Split row into label (fixed 22 chars) and value
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(22),
                Constraint::Min(0),
            ])
            .split(row_area);

        frame.render_widget(
            Paragraph::new(label).style(Style::default().fg(label_color)),
            h_chunks[0],
        );
        frame.render_widget(
            Paragraph::new(value.to_string()).style(Style::default().fg(VALUE)),
            h_chunks[1],
        );
    }
    *fi += 1;
}
