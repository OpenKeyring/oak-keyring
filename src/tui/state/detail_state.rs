//! Password detail panel state: record display data, field navigation, password visibility.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::commands::types::HealthIssue;
use crate::types::credential::CredentialType as CrateCredentialType;

// ── Field Value ────────────────────────────────────

/// Display value for a detail field (supports mask/reveal for secrets).
#[derive(Debug, Clone)]
pub enum FieldValue {
    /// Plain visible text (username, URL, etc.)
    Plain(String),
    /// Masked secret (shows ••••••)
    Masked,
    /// Revealed secret (temporarily visible)
    Revealed(String),
}

// ── Detail Field ───────────────────────────────────

/// A single displayable field in the detail panel.
#[derive(Debug, Clone)]
pub struct DetailField {
    /// Display label (e.g., "用户名", "密码")
    pub label: String,
    /// Current display value
    pub value: FieldValue,
    /// Whether this field can be copied (Enter or shortcut)
    pub copyable: bool,
    /// Whether this field supports show/hide toggle (passwords)
    pub toggleable: bool,
    /// The kind of field (for copy shortcut resolution)
    pub kind: DetailFieldKind,
}

/// Identifies which field this is (for keyboard shortcut mapping).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailFieldKind {
    Username,
    Password,
    Url,
    AppId,
    SecretKey,
    PublicKey,
    PrivateKey,
    Passphrase,
    Notes,
}

impl DetailField {
    /// Whether this field is interactive (navigable with up/down).
    pub fn is_interactive(&self) -> bool {
        self.copyable || self.toggleable
    }

    /// Get the display string for the value.
    pub fn display_value(&self) -> String {
        match &self.value {
            FieldValue::Plain(s) => s.clone(),
            FieldValue::Masked => "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}".to_string(),
            FieldValue::Revealed(s) => s.clone(),
        }
    }
}

// ── Expiry Status ──────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpiryStatus {
    None,
    Valid,
    ExpiringSoon,
    Expired,
}

impl ExpiryStatus {
    pub fn from_date(expires_at: Option<DateTime<Utc>>) -> Self {
        match expires_at {
            None => Self::None,
            Some(dt) => {
                let now = Utc::now();
                if dt < now {
                    Self::Expired
                } else if dt < now + chrono::Duration::days(30) {
                    Self::ExpiringSoon
                } else {
                    Self::Valid
                }
            }
        }
    }
}

// ── Password Strength ──────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordStrength {
    VeryWeak,
    Weak,
    Fair,
    Strong,
    VeryStrong,
}

impl PasswordStrength {
    pub fn label(&self) -> &'static str {
        match self {
            Self::VeryWeak => "极弱",
            Self::Weak => "弱",
            Self::Fair => "中等",
            Self::Strong => "强",
            Self::VeryStrong => "极强",
        }
    }

    pub fn color(&self) -> ratatui::style::Color {
        use crate::tui::theme;
        match self {
            Self::VeryWeak => theme::ERROR,
            Self::Weak => theme::WARNING,
            Self::Fair => theme::BRAND,
            Self::Strong => theme::PRIMARY,
            Self::VeryStrong => theme::SUCCESS,
        }
    }

    pub fn fraction(&self) -> f32 {
        match self {
            Self::VeryWeak => 0.1,
            Self::Weak => 0.3,
            Self::Fair => 0.5,
            Self::Strong => 0.75,
            Self::VeryStrong => 1.0,
        }
    }
}

// ── Detail View Data ───────────────────────────────

#[derive(Debug, Clone)]
pub struct DetailViewData {
    pub id: Uuid,
    pub name: String,
    pub subtitle: String,
    pub credential_type: CrateCredentialType,
    pub is_favorite: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub expiry_status: ExpiryStatus,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub fields: Vec<DetailField>,
    pub password_strength: Option<PasswordStrength>,
}

// ── Detail Panel State ─────────────────────────────

#[derive(Debug, Default)]
pub struct DetailPanelState {
    pub record: Option<DetailViewData>,
    pub focused_field: usize,
    pub password_visible: bool,
    pub health_issue: Option<HealthIssue>,
}

impl DetailPanelState {
    pub fn with_record(record: DetailViewData) -> Self {
        Self {
            record: Some(record),
            focused_field: 0,
            password_visible: false,
            health_issue: None,
        }
    }

    pub fn clear(&mut self) {
        self.record = None;
        self.focused_field = 0;
        self.password_visible = false;
        self.health_issue = None;
    }

    pub fn interactive_fields(&self) -> Vec<(usize, &DetailField)> {
        self.record
            .as_ref()
            .map(|r| {
                r.fields
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| f.is_interactive())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn current_field(&self) -> Option<&DetailField> {
        self.record
            .as_ref()
            .and_then(|r| r.fields.get(self.focused_field))
    }

    pub fn move_field_down(&mut self) -> bool {
        let record = match &self.record {
            Some(r) => r,
            None => return false,
        };
        let fields = &record.fields;
        let start = self.focused_field + 1;
        for i in start..fields.len() {
            if fields[i].is_interactive() {
                self.focused_field = i;
                return true;
            }
        }
        false
    }

    pub fn move_field_up(&mut self) -> bool {
        let record = match &self.record {
            Some(r) => r,
            None => return false,
        };
        let fields = &record.fields;
        if self.focused_field == 0 {
            return false;
        }
        for i in (0..self.focused_field).rev() {
            if fields[i].is_interactive() {
                self.focused_field = i;
                return true;
            }
        }
        false
    }

    pub fn toggle_password(&mut self) -> bool {
        if self.password_visible {
            if let Some(ref mut record) = self.record {
                for field in &mut record.fields {
                    if field.toggleable {
                        if let FieldValue::Revealed(_) = field.value {
                            field.value = FieldValue::Masked;
                        }
                    }
                }
            }
            self.password_visible = false;
            false
        } else {
            self.password_visible = true;
            true
        }
    }

    pub fn password_field(&self) -> Option<&DetailField> {
        self.record.as_ref().and_then(|r| {
            r.fields.iter().find(|f| f.kind == DetailFieldKind::Password
                || f.kind == DetailFieldKind::SecretKey
                || f.kind == DetailFieldKind::PrivateKey)
        })
    }

    pub fn username_field(&self) -> Option<&DetailField> {
        self.record.as_ref().and_then(|r| {
            r.fields.iter().find(|f| {
                matches!(f.kind, DetailFieldKind::Username
                    | DetailFieldKind::AppId
                    | DetailFieldKind::PublicKey)
            })
        })
    }

    pub fn build_from_record(record: &crate::types::record::DecryptedRecord) -> DetailViewData {
        match record {
            crate::types::record::DecryptedRecord::Login {
                id,
                name,
                username,
                password: _,
                url,
                notes,
                is_favorite,
                expires_at,
                created_at,
                updated_at,
                tags,
                ..
            } => {
                let expiry_status = ExpiryStatus::from_date(*expires_at);
                let subtitle = url.clone().unwrap_or_default();
                let fields = vec![
                    DetailField {
                        label: "用户名".into(),
                        value: FieldValue::Plain(username.clone()),
                        copyable: true,
                        toggleable: false,
                        kind: DetailFieldKind::Username,
                    },
                    DetailField {
                        label: "密码".into(),
                        value: FieldValue::Masked,
                        copyable: true,
                        toggleable: true,
                        kind: DetailFieldKind::Password,
                    },
                ];
                let mut all_fields = fields;
                if let Some(ref u) = url {
                    all_fields.push(DetailField {
                        label: "网址".into(),
                        value: FieldValue::Plain(u.clone()),
                        copyable: true,
                        toggleable: false,
                        kind: DetailFieldKind::Url,
                    });
                }
                if let Some(ref n) = notes {
                    all_fields.push(DetailField {
                        label: "备注".into(),
                        value: FieldValue::Plain(n.clone()),
                        copyable: true,
                        toggleable: false,
                        kind: DetailFieldKind::Notes,
                    });
                }
                DetailViewData {
                    id: *id,
                    name: name.clone(),
                    subtitle,
                    credential_type: CrateCredentialType::Login,
                    is_favorite: *is_favorite,
                    expires_at: *expires_at,
                    expiry_status,
                    tags: tags.clone(),
                    notes: notes.clone(),
                    created_at: *created_at,
                    updated_at: *updated_at,
                    fields: all_fields,
                    password_strength: None,
                }
            }
            crate::types::record::DecryptedRecord::Api {
                id,
                name,
                app_id,
                secret_key: _,
                url,
                notes,
                is_favorite,
                expires_at,
                created_at,
                updated_at,
                tags,
                ..
            } => {
                let expiry_status = ExpiryStatus::from_date(*expires_at);
                let subtitle = url.clone().unwrap_or_default();
                let mut fields = vec![
                    DetailField {
                        label: "AppID".into(),
                        value: FieldValue::Plain(app_id.clone()),
                        copyable: true,
                        toggleable: false,
                        kind: DetailFieldKind::AppId,
                    },
                    DetailField {
                        label: "SecretKey".into(),
                        value: FieldValue::Masked,
                        copyable: true,
                        toggleable: true,
                        kind: DetailFieldKind::SecretKey,
                    },
                ];
                if let Some(ref u) = url {
                    fields.push(DetailField {
                        label: "网址".into(),
                        value: FieldValue::Plain(u.clone()),
                        copyable: true,
                        toggleable: false,
                        kind: DetailFieldKind::Url,
                    });
                }
                if let Some(ref n) = notes {
                    fields.push(DetailField {
                        label: "备注".into(),
                        value: FieldValue::Plain(n.clone()),
                        copyable: true,
                        toggleable: false,
                        kind: DetailFieldKind::Notes,
                    });
                }
                DetailViewData {
                    id: *id,
                    name: name.clone(),
                    subtitle,
                    credential_type: CrateCredentialType::Api,
                    is_favorite: *is_favorite,
                    expires_at: *expires_at,
                    expiry_status,
                    tags: tags.clone(),
                    notes: notes.clone(),
                    created_at: *created_at,
                    updated_at: *updated_at,
                    fields,
                    password_strength: None,
                }
            }
            crate::types::record::DecryptedRecord::Ssh {
                id,
                name,
                public_key,
                private_key: _,
                passphrase: _,
                notes,
                is_favorite,
                expires_at,
                created_at,
                updated_at,
                tags,
                ..
            } => {
                let expiry_status = ExpiryStatus::from_date(*expires_at);
                let mut fields = vec![
                    DetailField {
                        label: "公钥".into(),
                        value: FieldValue::Plain(public_key.clone()),
                        copyable: true,
                        toggleable: false,
                        kind: DetailFieldKind::PublicKey,
                    },
                    DetailField {
                        label: "私钥".into(),
                        value: FieldValue::Masked,
                        copyable: true,
                        toggleable: true,
                        kind: DetailFieldKind::PrivateKey,
                    },
                ];
                fields.push(DetailField {
                    label: "Passphrase".into(),
                    value: FieldValue::Masked,
                    copyable: true,
                    toggleable: true,
                    kind: DetailFieldKind::Passphrase,
                });
                if let Some(ref n) = notes {
                    fields.push(DetailField {
                        label: "备注".into(),
                        value: FieldValue::Plain(n.clone()),
                        copyable: true,
                        toggleable: false,
                        kind: DetailFieldKind::Notes,
                    });
                }
                DetailViewData {
                    id: *id,
                    name: name.clone(),
                    subtitle: String::new(),
                    credential_type: CrateCredentialType::Ssh,
                    is_favorite: *is_favorite,
                    expires_at: *expires_at,
                    expiry_status,
                    tags: tags.clone(),
                    notes: notes.clone(),
                    created_at: *created_at,
                    updated_at: *updated_at,
                    fields,
                    password_strength: None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    label: "用户名".into(),
                    value: FieldValue::Plain("alice".into()),
                    copyable: true,
                    toggleable: false,
                    kind: DetailFieldKind::Username,
                },
                DetailField {
                    label: "密码".into(),
                    value: FieldValue::Masked,
                    copyable: true,
                    toggleable: true,
                    kind: DetailFieldKind::Password,
                },
                DetailField {
                    label: "网址".into(),
                    value: FieldValue::Plain("https://example.com".into()),
                    copyable: true,
                    toggleable: false,
                    kind: DetailFieldKind::Url,
                },
            ],
            password_strength: None,
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
                    label: "用户名".into(),
                    value: FieldValue::Plain("alice".into()),
                    copyable: true,
                    toggleable: false,
                    kind: DetailFieldKind::Username,
                },
                DetailField {
                    label: "密码".into(),
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
        let plain = DetailField {
            label: "用户名".into(),
            value: FieldValue::Plain("alice".into()),
            copyable: false,
            toggleable: false,
            kind: DetailFieldKind::Username,
        };
        assert_eq!(plain.display_value(), "alice");

        let masked = DetailField {
            label: "密码".into(),
            value: FieldValue::Masked,
            copyable: false,
            toggleable: false,
            kind: DetailFieldKind::Password,
        };
        assert_eq!(masked.display_value(), "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");

        let revealed = DetailField {
            label: "密码".into(),
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
}
