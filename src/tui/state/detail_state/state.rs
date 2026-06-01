//! Password detail panel state: record display data, field navigation, password visibility.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::commands::types::HealthIssue;
use crate::t;
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

/// Action buttons rendered inside the detail panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailActionKind {
    Copy,
    ToggleSecret,
}

/// Keyboard/mouse focus target for a detail-panel action button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetailActionFocus {
    pub field_index: usize,
    pub kind: DetailActionKind,
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
            FieldValue::Masked => {
                "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}".to_string()
            }
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
    pub fn label(&self) -> String {
        match self {
            Self::VeryWeak => crate::t!("tui.generator.strength_too_weak").to_string(),
            Self::Weak => crate::t!("tui.generator.strength_weak").to_string(),
            Self::Fair => crate::t!("tui.generator.strength_fair").to_string(),
            Self::Strong => crate::t!("tui.generator.strength_strong").to_string(),
            Self::VeryStrong => crate::t!("tui.generator.strength_very_strong").to_string(),
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
    /// Deletion timestamp (set when record is in trash).
    pub deleted_at: Option<DateTime<Utc>>,
}

// ── Detail Panel State ─────────────────────────────

#[derive(Debug)]
pub struct DetailPanelState {
    pub record: Option<DetailViewData>,
    pub focused_field: usize,
    /// Focused action button inside the current field row, if any.
    pub focused_action: Option<DetailActionFocus>,
    pub password_visible: bool,
    pub health_issue: Option<HealthIssue>,
    /// Whether the current record is in the trash (deleted).
    pub is_trash: bool,
    /// Trash retention days from config (0 = never auto-delete).
    pub trash_retention_days: u32,
}

impl Default for DetailPanelState {
    fn default() -> Self {
        Self {
            record: None,
            focused_field: 0,
            focused_action: None,
            password_visible: false,
            health_issue: None,
            is_trash: false,
            trash_retention_days: 30,
        }
    }
}

impl DetailPanelState {
    pub fn with_record(record: DetailViewData) -> Self {
        Self {
            record: Some(record),
            focused_field: 0,
            focused_action: None,
            password_visible: false,
            health_issue: None,
            is_trash: false,
            trash_retention_days: 30,
        }
    }

    pub fn clear(&mut self) {
        self.record = None;
        self.focused_field = 0;
        self.focused_action = None;
        self.password_visible = false;
        self.health_issue = None;
        self.is_trash = false;
        self.trash_retention_days = 30;
    }

    /// Set the trash context for the current detail view.
    pub fn set_trash_context(&mut self, is_trash: bool, retention_days: u32) {
        self.is_trash = is_trash;
        self.trash_retention_days = retention_days;
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

    pub fn available_actions(&self) -> Vec<DetailActionFocus> {
        self.record
            .as_ref()
            .map(|record| {
                record
                    .fields
                    .iter()
                    .enumerate()
                    .flat_map(|(field_index, field)| {
                        let mut actions = Vec::new();
                        if field.toggleable {
                            actions.push(DetailActionFocus {
                                field_index,
                                kind: DetailActionKind::ToggleSecret,
                            });
                        }
                        if field.copyable {
                            actions.push(DetailActionFocus {
                                field_index,
                                kind: DetailActionKind::Copy,
                            });
                        }
                        actions
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn actions_for_field(&self, field_index: usize) -> Vec<DetailActionFocus> {
        self.record
            .as_ref()
            .and_then(|record| record.fields.get(field_index))
            .map(|field| {
                let mut actions = Vec::new();
                if field.toggleable {
                    actions.push(DetailActionFocus {
                        field_index,
                        kind: DetailActionKind::ToggleSecret,
                    });
                }
                if field.copyable {
                    actions.push(DetailActionFocus {
                        field_index,
                        kind: DetailActionKind::Copy,
                    });
                }
                actions
            })
            .unwrap_or_default()
    }

    pub fn focus_first_action(&mut self) -> bool {
        match self.available_actions().into_iter().next() {
            Some(action) => {
                self.focused_field = action.field_index;
                self.focused_action = Some(action);
                true
            }
            None => false,
        }
    }

    pub fn clear_action_focus(&mut self) {
        self.focused_action = None;
    }

    pub fn move_action_down(&mut self) -> bool {
        self.move_action_vertical(1)
    }

    pub fn move_action_up(&mut self) -> bool {
        self.move_action_vertical(-1)
    }

    fn move_action_vertical(&mut self, delta: isize) -> bool {
        let Some(current) = self.focused_action else {
            return self.focus_first_action();
        };
        let Some(record) = self.record.as_ref() else {
            return false;
        };
        let mut next_index = current.field_index as isize + delta;
        while next_index >= 0 && (next_index as usize) < record.fields.len() {
            let actions = self.actions_for_field(next_index as usize);
            if !actions.is_empty() {
                let action = actions
                    .iter()
                    .copied()
                    .find(|action| action.kind == current.kind)
                    .unwrap_or(actions[0]);
                self.focused_field = action.field_index;
                self.focused_action = Some(action);
                return true;
            }
            next_index += delta;
        }
        false
    }

    pub fn move_action_right(&mut self) -> bool {
        self.move_action_horizontal(1)
    }

    pub fn move_action_left(&mut self) -> bool {
        self.move_action_horizontal(-1)
    }

    fn move_action_horizontal(&mut self, delta: isize) -> bool {
        let Some(current) = self.focused_action else {
            return self.focus_first_action();
        };
        let actions = self.actions_for_field(current.field_index);
        let Some(current_index) = actions.iter().position(|action| *action == current) else {
            return false;
        };
        let next_index = current_index as isize + delta;
        if next_index < 0 {
            self.clear_action_focus();
            return true;
        }
        let Some(action) = actions.get(next_index as usize).copied() else {
            return false;
        };
        self.focused_field = action.field_index;
        self.focused_action = Some(action);
        true
    }

    pub fn set_action_focus(&mut self, action: DetailActionFocus) -> bool {
        if self.available_actions().contains(&action) {
            self.focused_field = action.field_index;
            self.focused_action = Some(action);
            true
        } else {
            false
        }
    }

    pub fn move_field_down(&mut self) -> bool {
        let record = match &self.record {
            Some(r) => r,
            None => return false,
        };
        let fields = &record.fields;
        let start = self.focused_field + 1;
        for (i, field) in fields.iter().enumerate().skip(start) {
            if field.is_interactive() {
                self.focused_field = i;
                self.focused_action = None;
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
                self.focused_action = None;
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
            r.fields.iter().find(|f| {
                f.kind == DetailFieldKind::Password
                    || f.kind == DetailFieldKind::SecretKey
                    || f.kind == DetailFieldKind::PrivateKey
            })
        })
    }

    /// Returns the currently focused field if it is toggleable (supports
    /// show/hide), otherwise falls back to the first primary password-like
    /// field found by [`Self::password_field`].
    ///
    /// This is used by the `p` key handler so that pressing `p` on a focused
    /// passphrase field decrypts the passphrase rather than jumping to the
    /// private key / password field.
    pub fn current_toggleable_field(&self) -> Option<&DetailField> {
        self.record.as_ref().and_then(|r| {
            let focused_idx = self.focused_field;
            r.fields.get(focused_idx).and_then(|f| {
                if f.toggleable {
                    Some(f)
                } else {
                    // Fallback: find the primary password-like field
                    self.password_field()
                }
            })
        })
    }

    pub fn username_field(&self) -> Option<&DetailField> {
        self.record.as_ref().and_then(|r| {
            r.fields.iter().find(|f| {
                matches!(
                    f.kind,
                    DetailFieldKind::Username | DetailFieldKind::AppId | DetailFieldKind::PublicKey
                )
            })
        })
    }

    pub fn build_from_record(
        record: &crate::types::record::DecryptedRecord,
        strength: Option<crate::crypto::strength::PasswordStrength>,
    ) -> DetailViewData {
        let mapped_strength = strength.map(|s| match s.level {
            crate::crypto::strength::StrengthLevel::VeryWeak => PasswordStrength::VeryWeak,
            crate::crypto::strength::StrengthLevel::Weak => PasswordStrength::Weak,
            crate::crypto::strength::StrengthLevel::Fair => PasswordStrength::Fair,
            crate::crypto::strength::StrengthLevel::Strong => PasswordStrength::Strong,
            crate::crypto::strength::StrengthLevel::VeryStrong => PasswordStrength::VeryStrong,
        });
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
                        label: t!("tui.password_detail.username_label").to_string(),
                        value: FieldValue::Plain(username.clone()),
                        copyable: true,
                        toggleable: false,
                        kind: DetailFieldKind::Username,
                    },
                    DetailField {
                        label: t!("tui.password_detail.password_label").to_string(),
                        value: FieldValue::Masked,
                        copyable: true,
                        toggleable: true,
                        kind: DetailFieldKind::Password,
                    },
                ];
                let mut all_fields = fields;
                if let Some(ref u) = url {
                    all_fields.push(DetailField {
                        label: t!("tui.password_detail.url_label").to_string(),
                        value: FieldValue::Plain(u.clone()),
                        copyable: true,
                        toggleable: false,
                        kind: DetailFieldKind::Url,
                    });
                }
                if let Some(ref n) = notes {
                    all_fields.push(DetailField {
                        label: t!("tui.password_detail.notes_label").to_string(),
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
                    password_strength: mapped_strength,
                    deleted_at: None,
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
                        label: t!("tui.password_detail.app_id_label").to_string(),
                        value: FieldValue::Plain(app_id.clone()),
                        copyable: true,
                        toggleable: false,
                        kind: DetailFieldKind::AppId,
                    },
                    DetailField {
                        label: t!("tui.password_detail.secret_key_label").to_string(),
                        value: FieldValue::Masked,
                        copyable: true,
                        toggleable: true,
                        kind: DetailFieldKind::SecretKey,
                    },
                ];
                if let Some(ref u) = url {
                    fields.push(DetailField {
                        label: t!("tui.password_detail.url_label").to_string(),
                        value: FieldValue::Plain(u.clone()),
                        copyable: true,
                        toggleable: false,
                        kind: DetailFieldKind::Url,
                    });
                }
                if let Some(ref n) = notes {
                    fields.push(DetailField {
                        label: t!("tui.password_detail.notes_label").to_string(),
                        value: FieldValue::Plain(n.clone()),
                        copyable: false,
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
                    password_strength: mapped_strength,
                    deleted_at: None,
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
                        label: t!("tui.password_detail.public_key_label").to_string(),
                        value: FieldValue::Plain(public_key.clone()),
                        copyable: true,
                        toggleable: false,
                        kind: DetailFieldKind::PublicKey,
                    },
                    DetailField {
                        label: t!("tui.password_detail.private_key_label").to_string(),
                        value: FieldValue::Masked,
                        copyable: true,
                        toggleable: true,
                        kind: DetailFieldKind::PrivateKey,
                    },
                ];
                fields.push(DetailField {
                    label: t!("tui.password_detail.passphrase_label").to_string(),
                    value: FieldValue::Masked,
                    copyable: true,
                    toggleable: true,
                    kind: DetailFieldKind::Passphrase,
                });
                if let Some(ref n) = notes {
                    fields.push(DetailField {
                        label: t!("tui.password_detail.notes_label").to_string(),
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
                    password_strength: mapped_strength,
                    deleted_at: None,
                }
            }
            crate::types::record::DecryptedRecord::SecureNote {
                id,
                name,
                notes,
                is_favorite,
                expires_at,
                created_at,
                updated_at,
                tags,
                ..
            } => {
                let expiry_status = ExpiryStatus::from_date(*expires_at);
                let mut fields = Vec::new();
                if let Some(ref n) = notes {
                    fields.push(DetailField {
                        label: t!("tui.password_detail.notes_label").to_string(),
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
                    credential_type: CrateCredentialType::SecureNote,
                    is_favorite: *is_favorite,
                    expires_at: *expires_at,
                    expiry_status,
                    tags: tags.clone(),
                    notes: notes.clone(),
                    created_at: *created_at,
                    updated_at: *updated_at,
                    fields,
                    password_strength: None, // No password for SecureNote
                    deleted_at: None,
                }
            }
        }
    }
}
