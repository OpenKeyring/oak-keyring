use super::*;
use crate::commands::types::HealthIssue;
use crate::t;
use crate::types::credential::CredentialType as CrateCredentialType;
use chrono::Utc;
use uuid::Uuid;

fn make_login_data() -> DetailViewData {
    DetailViewData {
        id: Uuid::new_v4(),
        name: "Test Login".into(),
        subtitle: "https://example.com".into(),
        credential_type: CrateCredentialType::Login,
        is_favorite: false,
        expires_at: None,
        expiry_status: ExpiryStatus::None,
        tags: vec![],
        notes: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        fields: vec![
            DetailField {
                label: t!("tui.entry.username_label").to_string(),
                value: FieldValue::Plain("alice".into()),
                copyable: true,
                toggleable: false,
                kind: DetailFieldKind::Username,
            },
            DetailField {
                label: t!("tui.entry.password_label").to_string(),
                value: FieldValue::Masked,
                copyable: true,
                toggleable: true,
                kind: DetailFieldKind::Password,
            },
            DetailField {
                label: t!("tui.entry.url_label").to_string(),
                value: FieldValue::Plain("https://example.com".into()),
                copyable: true,
                toggleable: false,
                kind: DetailFieldKind::Url,
            },
        ],
        password_strength: None,
        deleted_at: None,
    }
}

#[test]
fn detail_state_default_is_empty() {
    let state = DetailPanelState::default();
    assert!(state.record.is_none());
    assert_eq!(state.focused_field, 0);
    assert!(!state.password_visible);
    assert!(state.health_issue.is_none());
}

#[test]
fn detail_state_clear() {
    let mut state = DetailPanelState::with_record(make_login_data());
    assert!(state.record.is_some());
    state.focused_field = 2;
    state.password_visible = true;
    state.health_issue = Some(HealthIssue::Weak);

    state.clear();

    assert!(state.record.is_none());
    assert_eq!(state.focused_field, 0);
    assert!(!state.password_visible);
    assert!(state.health_issue.is_none());
}

#[test]
fn field_navigation_skips_non_interactive() {
    let _guard = crate::tui::i18n::LocaleGuard::en();
    let data = DetailViewData {
        fields: vec![
            DetailField {
                label: "Header".into(),
                value: FieldValue::Plain("non-interactive".into()),
                copyable: false,
                toggleable: false,
                kind: DetailFieldKind::Notes,
            },
            DetailField {
                label: t!("tui.entry.username_label").to_string(),
                value: FieldValue::Plain("alice".into()),
                copyable: true,
                toggleable: false,
                kind: DetailFieldKind::Username,
            },
            DetailField {
                label: t!("tui.entry.password_label").to_string(),
                value: FieldValue::Masked,
                copyable: true,
                toggleable: true,
                kind: DetailFieldKind::Password,
            },
        ],
        ..make_login_data()
    };
    let mut state = DetailPanelState::with_record(data);
    state.focused_field = 0;

    // Should skip index 0 (non-interactive) and land on 1
    assert!(state.move_field_down());
    assert_eq!(state.focused_field, 1);
}

#[test]
fn field_navigation_up() {
    let mut state = DetailPanelState::with_record(make_login_data());
    state.focused_field = 2;

    assert!(state.move_field_up());
    assert_eq!(state.focused_field, 1);

    assert!(state.move_field_up());
    assert_eq!(state.focused_field, 0);

    // Already at top, should return false
    assert!(!state.move_field_up());
}

#[test]
fn password_toggle_masks_revealed() {
    let mut state = DetailPanelState::with_record(make_login_data());

    // Toggle on: password_visible becomes true
    assert!(state.toggle_password());
    assert!(state.password_visible);

    // Toggle off: revealed fields should be masked again
    assert!(!state.toggle_password());
    assert!(!state.password_visible);

    // Verify the password field is masked
    let password_field = state
        .record
        .as_ref()
        .unwrap()
        .fields
        .iter()
        .find(|f| f.kind == DetailFieldKind::Password)
        .unwrap();
    assert!(matches!(password_field.value, FieldValue::Masked));
}

#[test]
fn expiry_status_from_date() {
    assert_eq!(ExpiryStatus::from_date(None), ExpiryStatus::None);
    assert_eq!(
        ExpiryStatus::from_date(Some(Utc::now() - chrono::Duration::days(1))),
        ExpiryStatus::Expired
    );
    assert_eq!(
        ExpiryStatus::from_date(Some(Utc::now() + chrono::Duration::days(10))),
        ExpiryStatus::ExpiringSoon
    );
    assert_eq!(
        ExpiryStatus::from_date(Some(Utc::now() + chrono::Duration::days(100))),
        ExpiryStatus::Valid
    );
}

#[test]
fn password_strength_colors() {
    use crate::tui::theme;
    assert_eq!(PasswordStrength::VeryWeak.color(), theme::ERROR);
    assert_eq!(PasswordStrength::Weak.color(), theme::WARNING);
    assert_eq!(PasswordStrength::Fair.color(), theme::BRAND);
    assert_eq!(PasswordStrength::Strong.color(), theme::PRIMARY);
    assert_eq!(PasswordStrength::VeryStrong.color(), theme::SUCCESS);
}

#[test]
fn field_display_value() {
    let _guard = crate::tui::i18n::LocaleGuard::en();
    let plain = DetailField {
        label: t!("tui.entry.username_label").to_string(),
        value: FieldValue::Plain("alice".into()),
        copyable: false,
        toggleable: false,
        kind: DetailFieldKind::Username,
    };
    assert_eq!(plain.display_value(), "alice");

    let masked = DetailField {
        label: t!("tui.entry.password_label").to_string(),
        value: FieldValue::Masked,
        copyable: false,
        toggleable: false,
        kind: DetailFieldKind::Password,
    };
    assert_eq!(
        masked.display_value(),
        "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"
    );

    let revealed = DetailField {
        label: t!("tui.entry.password_label").to_string(),
        value: FieldValue::Revealed("secret123".into()),
        copyable: false,
        toggleable: false,
        kind: DetailFieldKind::Password,
    };
    assert_eq!(revealed.display_value(), "secret123");
}

#[test]
fn username_field_per_type() {
    // Login type should return Username field
    let login_state = DetailPanelState::with_record(make_login_data());
    let uf = login_state.username_field().unwrap();
    assert_eq!(uf.kind, DetailFieldKind::Username);

    // Empty state should return None
    let empty_state = DetailPanelState::default();
    assert!(empty_state.username_field().is_none());
}

#[test]
fn build_from_login_record() {
    let _guard = crate::tui::i18n::LocaleGuard::en();
    use crate::types::record::DecryptedRecord;
    use crate::types::sensitive::SecureStr;

    let record = DecryptedRecord::Login {
        id: Uuid::new_v4(),
        name: "GitHub".into(),
        username: "octocat".into(),
        password: SecureStr::new("secret123".into()),
        url: Some("https://github.com".into()),
        notes: Some("My account".into()),
        totp: None,
        is_favorite: true,
        expires_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: 1,
        deleted: false,
        deleted_at: None,
        tags: vec!["dev".into()],
    };

    let data = DetailPanelState::build_from_record(&record, None);
    assert_eq!(data.name, "GitHub");
    assert!(data.is_favorite);
    assert_eq!(data.fields.len(), 4); // username, password, url, notes
    assert_eq!(
        data.fields[0].label,
        t!("tui.password_detail.username_label")
    );
    assert!(matches!(data.fields[0].value, FieldValue::Plain(ref s) if s == "octocat"));
    assert_eq!(
        data.fields[1].label,
        t!("tui.password_detail.password_label")
    );
    assert!(matches!(data.fields[1].value, FieldValue::Masked));
    assert_eq!(data.fields[2].label, t!("tui.password_detail.url_label"));
    assert_eq!(data.tags, vec!["dev"]);
}

#[test]
fn build_from_api_record() {
    use crate::types::record::DecryptedRecord;
    use crate::types::sensitive::SecureStr;

    let record = DecryptedRecord::Api {
        id: Uuid::new_v4(),
        name: "Cloud API".into(),
        app_id: "app_123".into(),
        secret_key: SecureStr::new("secret".into()),
        url: None,
        notes: None,
        is_favorite: false,
        expires_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: 1,
        deleted: false,
        deleted_at: None,
        tags: vec![],
    };

    let data = DetailPanelState::build_from_record(&record, None);
    assert_eq!(data.name, "Cloud API");
    assert_eq!(data.fields.len(), 2); // AppID, SecretKey
    assert_eq!(data.fields[0].kind, DetailFieldKind::AppId);
    assert_eq!(data.fields[1].kind, DetailFieldKind::SecretKey);
}

#[test]
fn build_from_ssh_record() {
    let _guard = crate::tui::i18n::LocaleGuard::en();
    use crate::types::record::DecryptedRecord;
    use crate::types::sensitive::SecureStr;

    let record = DecryptedRecord::Ssh {
        id: Uuid::new_v4(),
        name: "Server Key".into(),
        public_key: "ssh-rsa AAAA...".into(),
        private_key: Some(SecureStr::new("private".into())),
        passphrase: Some(SecureStr::new("pass".into())),
        notes: None,
        is_favorite: false,
        expires_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: 1,
        deleted: false,
        deleted_at: None,
        tags: vec![],
    };

    let data = DetailPanelState::build_from_record(&record, None);
    assert_eq!(data.fields.len(), 3); // PublicKey, PrivateKey, Passphrase
    assert_eq!(data.fields[0].kind, DetailFieldKind::PublicKey);
    assert!(matches!(data.fields[0].value, FieldValue::Plain(_)));
    assert_eq!(data.fields[1].kind, DetailFieldKind::PrivateKey);
    assert!(matches!(data.fields[1].value, FieldValue::Masked));
}

// ── API data helpers ────────────────────────────────────────────────────

fn make_api_data() -> DetailViewData {
    DetailViewData {
        id: Uuid::new_v4(),
        name: "Cloud API Key".into(),
        subtitle: "api.cloud.example.com".into(),
        credential_type: CrateCredentialType::Api,
        is_favorite: false,
        expires_at: None,
        expiry_status: ExpiryStatus::None,
        tags: vec![],
        notes: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        fields: vec![
            DetailField {
                label: t!("tui.entry.app_id_label").to_string(),
                value: FieldValue::Plain("app_1234567890".into()),
                copyable: true,
                toggleable: false,
                kind: DetailFieldKind::AppId,
            },
            DetailField {
                label: t!("tui.entry.secret_key_label").to_string(),
                value: FieldValue::Masked,
                copyable: true,
                toggleable: true,
                kind: DetailFieldKind::SecretKey,
            },
        ],
        password_strength: None,
        deleted_at: None,
    }
}

fn make_ssh_data() -> DetailViewData {
    DetailViewData {
        id: Uuid::new_v4(),
        name: "Production SSH Key".into(),
        subtitle: String::new(),
        credential_type: CrateCredentialType::Ssh,
        is_favorite: false,
        expires_at: None,
        expiry_status: ExpiryStatus::None,
        tags: vec![],
        notes: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        fields: vec![
            DetailField {
                label: t!("tui.entry.public_key_label").to_string(),
                value: FieldValue::Plain("ssh-rsa AAAA...".into()),
                copyable: true,
                toggleable: false,
                kind: DetailFieldKind::PublicKey,
            },
            DetailField {
                label: t!("tui.entry.private_key_label").to_string(),
                value: FieldValue::Masked,
                copyable: true,
                toggleable: true,
                kind: DetailFieldKind::PrivateKey,
            },
            DetailField {
                label: t!("tui.password_detail.passphrase_label").to_string(),
                value: FieldValue::Masked,
                copyable: true,
                toggleable: true,
                kind: DetailFieldKind::Passphrase,
            },
        ],
        password_strength: None,
        deleted_at: None,
    }
}

// ── password_field() accessor tests ─────────────────────────────────────

#[test]
fn password_field_api_returns_secret_key() {
    let state = DetailPanelState::with_record(make_api_data());
    let pf = state.password_field().unwrap();
    assert_eq!(pf.kind, DetailFieldKind::SecretKey);
}

#[test]
fn password_field_ssh_returns_private_key() {
    let state = DetailPanelState::with_record(make_ssh_data());
    let pf = state.password_field().unwrap();
    assert_eq!(pf.kind, DetailFieldKind::PrivateKey);
}

// ── username_field() accessor tests ─────────────────────────────────────

#[test]
fn username_field_api_returns_app_id() {
    let state = DetailPanelState::with_record(make_api_data());
    let uf = state.username_field().unwrap();
    assert_eq!(uf.kind, DetailFieldKind::AppId);
}

#[test]
fn username_field_ssh_returns_public_key() {
    let state = DetailPanelState::with_record(make_ssh_data());
    let uf = state.username_field().unwrap();
    assert_eq!(uf.kind, DetailFieldKind::PublicKey);
}
