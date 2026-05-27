use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use chrono::{DateTime, Utc};

use crate::config::{AliyunDriveType, ProviderConfig, SftpHostCheck, SyncMode, SyncProvider};
use crate::t;
use crate::tui::state::config_state::{GDriveAuthStatus, SyncConfigForm, SyncConnectionStatus};
use crate::tui::theme;

// ── Color palette (for provider-specific field area, not focusable) ────────

const LABEL: ratatui::style::Color = theme::NL_TEXT_MUTED;
const VALUE: ratatui::style::Color = theme::NL_TEXT;

// ── Masking helpers ────────────────────────────────────────────────────────

fn mask(value: &str) -> String {
    if value.is_empty() {
        t!("tui.config.not_set").to_string()
    } else {
        theme::ICON_PASSWORD_MASK.repeat(8)
    }
}

fn mask_opt(value: &Option<String>) -> String {
    match value {
        Some(v) if !v.is_empty() => theme::ICON_PASSWORD_MASK.repeat(8),
        _ => t!("tui.config.not_set").to_string(),
    }
}

// ── Main render ────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut Frame,
    area: Rect,
    form: &SyncConfigForm,
    status: SyncConnectionStatus,
    gdrive_auth_status: GDriveAuthStatus,
    last_sync: Option<DateTime<Utc>>,
    sync_error_message: Option<&str>,
    focused: usize,
) {
    let field_count = provider_field_count(&form.provider_config, &gdrive_auth_status);
    let mut constraints: Vec<Constraint> = Vec::with_capacity(6 + field_count as usize);
    constraints.push(Constraint::Length(2)); // Title
    constraints.push(Constraint::Length(1)); // Provider
    constraints.push(Constraint::Length(1)); // Sync mode
    constraints.push(Constraint::Length(1)); // Auto interval

    if field_count > 0 {
        constraints.push(Constraint::Length(1)); // Divider
        constraints.push(Constraint::Length(field_count)); // Provider fields
    }

    constraints.push(Constraint::Length(1)); // Test button + status
    constraints.push(Constraint::Min(0)); // Remainder

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut row = 0;

    super::render::render_section_title(frame, chunks[row], t!("tui.config.tab_sync").as_ref());
    row += 1;

    let provider_name = match form.provider {
        SyncProvider::Disabled => t!("tui.config.sync_disabled").to_string(),
        SyncProvider::ICloud => t!("tui.config.sync_icloud").to_string(),
        SyncProvider::GoogleDrive => t!("tui.config.sync_google_drive").to_string(),
        SyncProvider::Dropbox => t!("tui.config.sync_dropbox").to_string(),
        SyncProvider::OneDrive => t!("tui.config.sync_onedrive").to_string(),
        SyncProvider::WebDav => t!("tui.config.sync_webdav").to_string(),
        SyncProvider::Sftp => t!("tui.config.sync_sftp").to_string(),
        SyncProvider::S3 => t!("tui.config.sync_s3").to_string(),
        SyncProvider::AliyunDrive => t!("tui.config.sync_aliyun_drive").to_string(),
        SyncProvider::AliyunOss => t!("tui.config.sync_aliyun_oss").to_string(),
        SyncProvider::TencentCos => t!("tui.config.sync_tencent_cos").to_string(),
        SyncProvider::HuaweiObs => t!("tui.config.sync_huawei_obs").to_string(),
        SyncProvider::Upyun => t!("tui.config.sync_upyun").to_string(),
    };
    super::render::render_setting_row(
        frame,
        chunks[row],
        theme::NF_SYNC,
        t!("tui.config.sync_provider").as_ref(),
        &super::render::dropdown_control(&provider_name),
        focused == 0,
        true,
    );
    row += 1;

    let mode_label = match form.sync_mode {
        SyncMode::Auto => t!("tui.config.sync_mode_auto").to_string(),
        SyncMode::Manual => t!("tui.config.sync_mode_manual").to_string(),
    };
    super::render::render_setting_row(
        frame,
        chunks[row],
        theme::NF_SLIDERS,
        t!("tui.config.sync_mode").as_ref(),
        &super::render::dropdown_control(&mode_label),
        focused == 1,
        true,
    );
    row += 1;

    super::render::render_setting_row(
        frame,
        chunks[row],
        theme::NF_CLOCK,
        t!("tui.config.sync_interval").as_ref(),
        &super::render::dropdown_control(&t!("tui.config.seconds", n = form.auto_interval_seconds)),
        focused == 2,
        form.sync_mode != SyncMode::Manual,
    );
    row += 1;

    if field_count > 0 {
        let divider_text: String = "┄".repeat(area.width as usize);
        frame.render_widget(
            Paragraph::new(divider_text).style(Style::default().fg(theme::NL_LINE)),
            chunks[row],
        );
        row += 1;

        let field_area = chunks[row];
        let field_constraints = (0..field_count)
            .map(|_| Constraint::Length(1))
            .collect::<Vec<_>>();
        let field_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(field_constraints)
            .split(field_area);

        let mut fi = 0u16;
        render_provider_fields(
            &form.provider_config,
            gdrive_auth_status,
            last_sync,
            &field_chunks,
            &mut fi,
            frame,
        );
        row += 1;
    }

    let (status_text, enabled) = match status {
        SyncConnectionStatus::Connected => (
            format!(
                "{} {}",
                theme::ICON_SUCCESS,
                t!("tui.config.sync_connected")
            ),
            true,
        ),
        SyncConnectionStatus::Disconnected => {
            let base = format!(
                "{} {}",
                theme::ICON_ERROR,
                t!("tui.config.sync_disconnected")
            );
            let text = match sync_error_message {
                Some(err) if !err.is_empty() => format!("{}: {}", base, err),
                _ => base,
            };
            (text, true)
        }
        SyncConnectionStatus::NotConfigured => (
            format!(
                "{} {}",
                theme::ICON_NOT_CONFIGURED,
                t!("tui.config.sync_not_configured")
            ),
            false,
        ),
        SyncConnectionStatus::Testing => (
            format!(
                "{} {}",
                theme::ICON_SYNC_SYNCING,
                t!("tui.config.sync_testing")
            ),
            true,
        ),
    };

    super::render::render_setting_row(
        frame,
        chunks[row],
        theme::NF_CHECK_CIRCLE,
        t!("tui.config.sync_test_button").as_ref(),
        &super::render::plain_control(&format!(
            "{}  {}",
            t!("tui.config.sync_status"),
            status_text
        )),
        focused == 3,
        enabled,
    );
}

// ── Field count per provider ───────────────────────────────────────────────

fn provider_field_count(pc: &Option<ProviderConfig>, gdrive_auth_status: &GDriveAuthStatus) -> u16 {
    match pc {
        None => 0,                         // Disabled or not configured yet
        Some(ProviderConfig::ICloud) => 1, // just a hint line
        Some(ProviderConfig::GoogleDrive(_)) => {
            if matches!(gdrive_auth_status, GDriveAuthStatus::Authorized) {
                4 // auth status, last sync, auth action, root_path
            } else {
                3 // auth status, auth action, root_path
            }
        }
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
    gdrive_auth_status: GDriveAuthStatus,
    last_sync: Option<DateTime<Utc>>,
    chunks: &[Rect],
    fi: &mut u16,
    frame: &mut Frame,
) {
    match pc {
        None => {
            let hint = t!("tui.config.not_configured_hint").to_string();
            render_label_value(chunks, fi, frame, &hint, "", LABEL);
        }
        Some(ProviderConfig::ICloud) => {
            let hint = t!("tui.config.icloud_hint").to_string();
            render_label_value(chunks, fi, frame, &hint, "", LABEL);
        }
        Some(ProviderConfig::GoogleDrive(cfg)) => {
            let status_text = match &gdrive_auth_status {
                GDriveAuthStatus::NotAuthorized => {
                    t!("tui.config.gdrive_not_authorized").to_string()
                }
                GDriveAuthStatus::Authorizing => t!("tui.config.gdrive_authorizing").to_string(),
                GDriveAuthStatus::Authorized => t!("tui.config.gdrive_authorized").to_string(),
                GDriveAuthStatus::Failed { ref reason } => {
                    format!("{}: {}", t!("tui.config.gdrive_auth_failed"), reason)
                }
            };
            render_label_value(
                chunks,
                fi,
                frame,
                &t!("tui.config.sync_auth_status"),
                &status_text,
                LABEL,
            );

            // Show last sync time only when authorized
            if matches!(gdrive_auth_status, GDriveAuthStatus::Authorized) {
                let last_sync_text = last_sync
                    .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| t!("tui.config.sync_never").to_string());
                render_label_value(
                    chunks,
                    fi,
                    frame,
                    &t!("tui.config.sync_last_sync"),
                    &last_sync_text,
                    LABEL,
                );
            }

            let button_text = match &gdrive_auth_status {
                GDriveAuthStatus::NotAuthorized | GDriveAuthStatus::Failed { .. } => {
                    &t!("tui.config.sync_start_auth")
                }
                GDriveAuthStatus::Authorizing => &t!("tui.config.sync_authorizing"),
                GDriveAuthStatus::Authorized => &t!("tui.config.sync_authorized"),
            };
            render_label_value(
                chunks,
                fi,
                frame,
                &t!("tui.config.sync_action"),
                button_text,
                LABEL,
            );

            let l = t!("tui.config.field_work_dir");
            render_field(chunks, fi, frame, &l, &cfg.root_path, false);
        }
        Some(ProviderConfig::Dropbox(cfg)) => {
            let l = t!("tui.config.field_client_id");
            render_field(chunks, fi, frame, &l, &cfg.client_id, false);
            let l = t!("tui.config.field_client_secret");
            render_field(chunks, fi, frame, &l, &cfg.client_secret, true);
            let l = t!("tui.config.field_refresh_token");
            render_field(chunks, fi, frame, &l, &cfg.refresh_token, true);
            let l = t!("tui.config.field_work_dir");
            render_field(chunks, fi, frame, &l, &cfg.root_path, false);
        }
        Some(ProviderConfig::OneDrive(cfg)) => {
            let l = t!("tui.config.field_client_id");
            render_field(chunks, fi, frame, &l, &cfg.client_id, false);
            let l = t!("tui.config.field_client_secret");
            render_field(chunks, fi, frame, &l, &cfg.client_secret, true);
            let l = t!("tui.config.field_refresh_token");
            render_field(chunks, fi, frame, &l, &cfg.refresh_token, true);
            let l = t!("tui.config.field_work_dir");
            render_field(chunks, fi, frame, &l, &cfg.root_path, false);
        }
        Some(ProviderConfig::WebDav(cfg)) => {
            let l = t!("tui.config.field_endpoint");
            render_field(chunks, fi, frame, &l, &cfg.endpoint, false);
            let l = t!("tui.config.field_work_dir");
            render_field(chunks, fi, frame, &l, &cfg.root_path, false);
            let l = t!("tui.config.field_username");
            render_field_opt(chunks, fi, frame, &l, &cfg.username, false);
            let l = t!("tui.config.field_password");
            render_field_opt(chunks, fi, frame, &l, &cfg.password, true);
            let l = t!("tui.config.field_bearer_token");
            render_field_opt(chunks, fi, frame, &l, &cfg.bearer_token, true);
        }
        Some(ProviderConfig::Sftp(cfg)) => {
            let l = t!("tui.config.field_server");
            render_field(chunks, fi, frame, &l, &cfg.server, false);
            let l = t!("tui.config.field_work_dir");
            render_field(chunks, fi, frame, &l, &cfg.root_path, false);
            let l = t!("tui.config.field_ssh_key_path");
            render_field(chunks, fi, frame, &l, &cfg.ssh_key_path, false);
            let host_check_str = match cfg.host_check {
                SftpHostCheck::Strict => t!("tui.config.host_check_strict").to_string(),
                SftpHostCheck::Accept => t!("tui.config.host_check_accept").to_string(),
                SftpHostCheck::Add => t!("tui.config.host_check_add").to_string(),
            };
            let l = t!("tui.config.field_host_check");
            render_label_value(chunks, fi, frame, &l, &host_check_str, LABEL);
        }
        Some(ProviderConfig::S3(cfg)) => {
            let l = t!("tui.config.field_endpoint");
            render_field_opt(chunks, fi, frame, &l, &cfg.endpoint, false);
            let l = t!("tui.config.field_bucket");
            render_field(chunks, fi, frame, &l, &cfg.bucket, false);
            let l = t!("tui.config.field_region");
            render_field_opt(chunks, fi, frame, &l, &cfg.region, false);
            let l = t!("tui.config.field_access_key_id");
            render_field(chunks, fi, frame, &l, &cfg.access_key_id, false);
            let l = t!("tui.config.field_access_key_secret");
            render_field(chunks, fi, frame, &l, &cfg.secret_access_key, true);
            let l = t!("tui.config.field_work_dir");
            render_field(chunks, fi, frame, &l, &cfg.root_path, false);
        }
        Some(ProviderConfig::AliyunDrive(cfg)) => {
            let l = t!("tui.config.field_client_id");
            render_field(chunks, fi, frame, &l, &cfg.client_id, false);
            let l = t!("tui.config.field_client_secret");
            render_field(chunks, fi, frame, &l, &cfg.client_secret, true);
            let l = t!("tui.config.field_refresh_token");
            render_field(chunks, fi, frame, &l, &cfg.refresh_token, true);
            let drive_type_str = match cfg.drive_type {
                AliyunDriveType::Default => t!("tui.config.drive_type_default").to_string(),
                AliyunDriveType::Backup => t!("tui.config.drive_type_backup").to_string(),
                AliyunDriveType::Resource => t!("tui.config.drive_type_resource").to_string(),
            };
            let l = t!("tui.config.field_drive_type");
            render_label_value(chunks, fi, frame, &l, &drive_type_str, LABEL);
            let l = t!("tui.config.field_work_dir");
            render_field(chunks, fi, frame, &l, &cfg.root_path, false);
        }
        Some(ProviderConfig::AliyunOss(cfg)) => {
            let l = t!("tui.config.field_endpoint");
            render_field(chunks, fi, frame, &l, &cfg.endpoint, false);
            let l = t!("tui.config.field_bucket");
            render_field(chunks, fi, frame, &l, &cfg.bucket, false);
            let l = t!("tui.config.field_access_key_id");
            render_field(chunks, fi, frame, &l, &cfg.access_key_id, false);
            let l = t!("tui.config.field_access_key_secret");
            render_field(chunks, fi, frame, &l, &cfg.access_key_secret, true);
            let l = t!("tui.config.field_work_dir");
            render_field(chunks, fi, frame, &l, &cfg.root_path, false);
        }
        Some(ProviderConfig::TencentCos(cfg)) => {
            let l = t!("tui.config.field_endpoint");
            render_field(chunks, fi, frame, &l, &cfg.endpoint, false);
            let l = t!("tui.config.field_bucket");
            render_field(chunks, fi, frame, &l, &cfg.bucket, false);
            let l = t!("tui.config.field_secret_id");
            render_field(chunks, fi, frame, &l, &cfg.secret_id, false);
            let l = t!("tui.config.field_secret_key");
            render_field(chunks, fi, frame, &l, &cfg.secret_key, true);
            let l = t!("tui.config.field_work_dir");
            render_field(chunks, fi, frame, &l, &cfg.root_path, false);
        }
        Some(ProviderConfig::HuaweiObs(cfg)) => {
            let l = t!("tui.config.field_endpoint");
            render_field(chunks, fi, frame, &l, &cfg.endpoint, false);
            let l = t!("tui.config.field_bucket");
            render_field(chunks, fi, frame, &l, &cfg.bucket, false);
            let l = t!("tui.config.field_access_key_id");
            render_field(chunks, fi, frame, &l, &cfg.access_key_id, false);
            let l = t!("tui.config.field_access_key_secret");
            render_field(chunks, fi, frame, &l, &cfg.secret_access_key, true);
            let l = t!("tui.config.field_work_dir");
            render_field(chunks, fi, frame, &l, &cfg.root_path, false);
        }
        Some(ProviderConfig::Upyun(cfg)) => {
            let l = t!("tui.config.field_bucket");
            render_field(chunks, fi, frame, &l, &cfg.bucket, false);
            let l = t!("tui.config.field_operator");
            render_field(chunks, fi, frame, &l, &cfg.operator, false);
            let l = t!("tui.config.field_operator_password");
            render_field(chunks, fi, frame, &l, &cfg.operator_password, true);
            let l = t!("tui.config.field_work_dir");
            render_field(chunks, fi, frame, &l, &cfg.root_path, false);
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
    let display = if sensitive {
        mask(value)
    } else {
        value.to_string()
    };
    render_label_value(chunks, fi, frame, label, &display, LABEL);
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
            Some(v) if !v.is_empty() => v.clone(),
            _ => t!("tui.config.not_set").to_string(),
        }
    };
    render_label_value(chunks, fi, frame, label, &display, LABEL);
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
            .constraints([Constraint::Length(22), Constraint::Min(0)])
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
