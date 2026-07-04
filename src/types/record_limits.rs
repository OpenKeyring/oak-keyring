use crate::types::credential::{DataError, EncryptedPayload};

pub const MAX_RECORD_NAME_CHARS: usize = 120;
pub const MAX_LOGIN_USERNAME_CHARS: usize = 320;
pub const MAX_LOGIN_PASSWORD_CHARS: usize = 1024;
pub const MAX_URL_CHARS: usize = 2048;
pub const MAX_API_APP_ID_CHARS: usize = 512;
pub const MAX_API_SECRET_CHARS: usize = 8192;
pub const MAX_SSH_PUBLIC_KEY_CHARS: usize = 16_384;
pub const MAX_SSH_PRIVATE_KEY_CHARS: usize = 65_536;
pub const MAX_SSH_PASSPHRASE_CHARS: usize = 1024;
pub const MAX_NOTES_CHARS: usize = 16_384;
pub const MAX_SECURE_NOTE_CHARS: usize = 65_536;
pub const MAX_TAG_CHARS: usize = 50;
pub const MAX_TAGS_PER_RECORD: usize = 10;

pub fn char_count(value: &str) -> usize {
    value.chars().count()
}

pub fn validate_payload(payload: &EncryptedPayload) -> Result<(), DataError> {
    match payload {
        EncryptedPayload::Login {
            name,
            username,
            password,
            url,
            notes,
            totp,
        } => {
            validate_len("name", name, MAX_RECORD_NAME_CHARS)?;
            validate_len("username", username, MAX_LOGIN_USERNAME_CHARS)?;
            validate_len("password", password.expose(), MAX_LOGIN_PASSWORD_CHARS)?;
            validate_optional_len("url", url.as_deref(), MAX_URL_CHARS)?;
            validate_optional_len("notes", notes.as_deref(), MAX_NOTES_CHARS)?;
            validate_optional_len(
                "totp",
                totp.as_ref().map(|secret| secret.expose()),
                MAX_URL_CHARS,
            )
        }
        EncryptedPayload::Api {
            name,
            app_id,
            secret_key,
            url,
            notes,
        } => {
            validate_len("name", name, MAX_RECORD_NAME_CHARS)?;
            validate_len("app_id", app_id, MAX_API_APP_ID_CHARS)?;
            validate_len("secret_key", secret_key.expose(), MAX_API_SECRET_CHARS)?;
            validate_optional_len("url", url.as_deref(), MAX_URL_CHARS)?;
            validate_optional_len("notes", notes.as_deref(), MAX_NOTES_CHARS)
        }
        EncryptedPayload::Ssh {
            name,
            public_key,
            private_key,
            passphrase,
            notes,
        } => {
            validate_len("name", name, MAX_RECORD_NAME_CHARS)?;
            validate_len("public_key", public_key, MAX_SSH_PUBLIC_KEY_CHARS)?;
            if let Some(private_key) = private_key {
                validate_len(
                    "private_key",
                    private_key.expose(),
                    MAX_SSH_PRIVATE_KEY_CHARS,
                )?;
            }
            if let Some(passphrase) = passphrase {
                validate_len("passphrase", passphrase.expose(), MAX_SSH_PASSPHRASE_CHARS)?;
            }
            validate_optional_len("notes", notes.as_deref(), MAX_NOTES_CHARS)
        }
        EncryptedPayload::SecureNote { name, notes } => {
            validate_len("name", name, MAX_RECORD_NAME_CHARS)?;
            validate_optional_len("notes", notes.as_deref(), MAX_SECURE_NOTE_CHARS)
        }
    }
}

pub fn validate_tags(tags: &[String]) -> Result<(), DataError> {
    if tags.len() > MAX_TAGS_PER_RECORD {
        return Err(DataError::FieldTooLong {
            field: "tags",
            max: MAX_TAGS_PER_RECORD,
            actual: tags.len(),
        });
    }
    for tag in tags {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return Err(DataError::EmptyField("tag"));
        }
        validate_len("tag", trimmed, MAX_TAG_CHARS)?;
    }
    Ok(())
}

pub fn validate_len(field: &'static str, value: &str, max: usize) -> Result<(), DataError> {
    let actual = char_count(value);
    if actual > max {
        Err(DataError::FieldTooLong { field, max, actual })
    } else {
        Ok(())
    }
}

pub fn validate_optional_len(
    field: &'static str,
    value: Option<&str>,
    max: usize,
) -> Result<(), DataError> {
    if let Some(value) = value {
        validate_len(field, value, max)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::sensitive::SecureStr;

    #[test]
    fn validate_tags_rejects_more_than_ten_tags() {
        let tags: Vec<String> = (0..11).map(|n| format!("tag-{n}")).collect();
        assert!(matches!(
            validate_tags(&tags),
            Err(DataError::FieldTooLong { field: "tags", .. })
        ));
    }

    #[test]
    fn validate_payload_allows_secure_note_up_to_larger_note_limit() {
        let payload = EncryptedPayload::SecureNote {
            name: "note".to_string(),
            notes: Some("a".repeat(MAX_NOTES_CHARS + 1)),
        };
        assert!(validate_payload(&payload).is_ok());
    }

    #[test]
    fn validate_payload_rejects_overlong_login_password() {
        let payload = EncryptedPayload::Login {
            name: "login".to_string(),
            username: "alice".to_string(),
            password: SecureStr::new("a".repeat(MAX_LOGIN_PASSWORD_CHARS + 1)),
            url: None,
            notes: None,
            totp: None,
        };
        assert!(matches!(
            validate_payload(&payload),
            Err(DataError::FieldTooLong {
                field: "password",
                ..
            })
        ));
    }
}
