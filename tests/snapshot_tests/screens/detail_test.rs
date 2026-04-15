use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::tui::screens::main::detail::DetailPanel;
use oak_keyring::tui::state::detail_state::DetailPanelState;

#[test]
fn detail_panel_empty() {
    let backend = TestBackend::new(50, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let state = DetailPanelState::default();
    let panel = DetailPanel;

    terminal
        .draw(|frame| {
            panel.view(frame, frame.area(), &state, false, true);
        })
        .unwrap();

    insta::assert_snapshot!("detail_empty", terminal.backend());
}

#[test]
fn detail_login_record() {
    use oak_keyring::tui::state::detail_state::*;
    use uuid::Uuid;

    let backend = TestBackend::new(60, 25);
    let mut terminal = Terminal::new(backend).unwrap();

    let data = DetailViewData {
        id: Uuid::new_v4(),
        name: "GitHub".into(),
        subtitle: "github.com".into(),
        credential_type: oak_keyring::types::credential::CredentialType::Login,
        is_favorite: true,
        expires_at: None,
        expiry_status: ExpiryStatus::None,
        tags: vec!["work".into(), "dev".into()],
        notes: Some("Personal account".into()),
        created_at: chrono::DateTime::parse_from_rfc3339("2026-04-15T12:00:00Z").unwrap().to_utc(),
        updated_at: chrono::DateTime::parse_from_rfc3339("2026-04-15T12:00:00Z").unwrap().to_utc(),
        fields: vec![
            DetailField {
                label: "用户名".into(),
                value: FieldValue::Plain("octocat".into()),
                copyable: true, toggleable: false,
                kind: DetailFieldKind::Username,
            },
            DetailField {
                label: "密码".into(),
                value: FieldValue::Masked,
                copyable: true, toggleable: true,
                kind: DetailFieldKind::Password,
            },
            DetailField {
                label: "网址".into(),
                value: FieldValue::Plain("https://github.com".into()),
                copyable: true, toggleable: false,
                kind: DetailFieldKind::Url,
            },
            DetailField {
                label: "备注".into(),
                value: FieldValue::Plain("Personal account".into()),
                copyable: true, toggleable: false,
                kind: DetailFieldKind::Notes,
            },
        ],
        password_strength: Some(PasswordStrength::Strong),
    };
    let state = DetailPanelState::with_record(data);
    let panel = DetailPanel;

    terminal.draw(|frame| {
        panel.view(frame, frame.area(), &state, true, true);
    }).unwrap();

    insta::assert_snapshot!("detail_login_record", terminal.backend());
}

#[test]
fn detail_api_record() {
    use oak_keyring::tui::state::detail_state::*;
    use uuid::Uuid;

    let backend = TestBackend::new(60, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    let data = DetailViewData {
        id: Uuid::new_v4(),
        name: "腾讯云 API".into(),
        subtitle: "cloud.tencent.com".into(),
        credential_type: oak_keyring::types::credential::CredentialType::Api,
        is_favorite: false,
        expires_at: None,
        expiry_status: ExpiryStatus::None,
        tags: vec!["cloud".into()],
        notes: None,
        created_at: chrono::DateTime::parse_from_rfc3339("2026-04-15T12:00:00Z").unwrap().to_utc(),
        updated_at: chrono::DateTime::parse_from_rfc3339("2026-04-15T12:00:00Z").unwrap().to_utc(),
        fields: vec![
            DetailField {
                label: "AppID".into(),
                value: FieldValue::Plain("app_1234567890".into()),
                copyable: true, toggleable: false,
                kind: DetailFieldKind::AppId,
            },
            DetailField {
                label: "SecretKey".into(),
                value: FieldValue::Masked,
                copyable: true, toggleable: true,
                kind: DetailFieldKind::SecretKey,
            },
        ],
        password_strength: None,
    };
    let state = DetailPanelState::with_record(data);
    let panel = DetailPanel;

    terminal.draw(|frame| {
        panel.view(frame, frame.area(), &state, true, true);
    }).unwrap();

    insta::assert_snapshot!("detail_api_record", terminal.backend());
}

#[test]
fn detail_ssh_record() {
    use oak_keyring::tui::state::detail_state::*;
    use uuid::Uuid;

    let backend = TestBackend::new(60, 25);
    let mut terminal = Terminal::new(backend).unwrap();

    let data = DetailViewData {
        id: Uuid::new_v4(),
        name: "Server SSH Key".into(),
        subtitle: String::new(),
        credential_type: oak_keyring::types::credential::CredentialType::Ssh,
        is_favorite: false,
        expires_at: None,
        expiry_status: ExpiryStatus::None,
        tags: vec!["servers".into()],
        notes: None,
        created_at: chrono::DateTime::parse_from_rfc3339("2026-04-15T12:00:00Z").unwrap().to_utc(),
        updated_at: chrono::DateTime::parse_from_rfc3339("2026-04-15T12:00:00Z").unwrap().to_utc(),
        fields: vec![
            DetailField {
                label: "公钥".into(),
                value: FieldValue::Plain("ssh-rsa AAAA...".into()),
                copyable: true, toggleable: false,
                kind: DetailFieldKind::PublicKey,
            },
            DetailField {
                label: "私钥".into(),
                value: FieldValue::Masked,
                copyable: true, toggleable: true,
                kind: DetailFieldKind::PrivateKey,
            },
            DetailField {
                label: "Passphrase".into(),
                value: FieldValue::Masked,
                copyable: true, toggleable: true,
                kind: DetailFieldKind::Passphrase,
            },
        ],
        password_strength: None,
    };
    let state = DetailPanelState::with_record(data);
    let panel = DetailPanel;

    terminal.draw(|frame| {
        panel.view(frame, frame.area(), &state, true, true);
    }).unwrap();

    insta::assert_snapshot!("detail_ssh_record", terminal.backend());
}
