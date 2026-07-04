use crate::crypto::CryptoManager;
use crate::types::credential::{CredentialType, EncryptedPayload};
use crate::types::sensitive::SecureStr;

/// Private DTO for plaintext serialization at the crypto boundary.
/// This type is never exposed outside the crypto layer.
#[derive(serde::Serialize, serde::Deserialize)]
enum PayloadPlaintextDto {
    Login {
        name: String,
        username: String,
        password: String,
        url: Option<String>,
        notes: Option<String>,
        #[serde(default)]
        totp: Option<String>,
    },
    Api {
        name: String,
        app_id: String,
        secret_key: String,
        url: Option<String>,
        notes: Option<String>,
    },
    Ssh {
        name: String,
        public_key: String,
        private_key: Option<String>,
        passphrase: Option<String>,
        notes: Option<String>,
    },
    SecureNote {
        name: String,
        notes: Option<String>,
    },
}

impl PayloadPlaintextDto {
    /// Convert from EncryptedPayload, exposing secrets via SecureStr::expose().
    /// This is safe because the resulting plaintext DTO is immediately encrypted.
    fn from_payload(payload: &EncryptedPayload) -> Self {
        match payload {
            EncryptedPayload::Login {
                name,
                username,
                password,
                url,
                notes,
                totp,
            } => PayloadPlaintextDto::Login {
                name: name.clone(),
                username: username.clone(),
                password: password.expose().to_string(),
                url: url.clone(),
                notes: notes.clone(),
                totp: totp.as_ref().map(|secret| secret.expose().to_string()),
            },
            EncryptedPayload::Api {
                name,
                app_id,
                secret_key,
                url,
                notes,
            } => PayloadPlaintextDto::Api {
                name: name.clone(),
                app_id: app_id.clone(),
                secret_key: secret_key.expose().to_string(),
                url: url.clone(),
                notes: notes.clone(),
            },
            EncryptedPayload::Ssh {
                name,
                public_key,
                private_key,
                passphrase,
                notes,
            } => PayloadPlaintextDto::Ssh {
                name: name.clone(),
                public_key: public_key.clone(),
                private_key: private_key.as_ref().map(|pk| pk.expose().to_string()),
                passphrase: passphrase.as_ref().map(|pp| pp.expose().to_string()),
                notes: notes.clone(),
            },
            EncryptedPayload::SecureNote { name, notes } => PayloadPlaintextDto::SecureNote {
                name: name.clone(),
                notes: notes.clone(),
            },
        }
    }

    /// Convert into EncryptedPayload, wrapping secrets in SecureStr.
    /// This is safe because the plaintext DTO is only created from decrypted data.
    fn into_payload(self) -> EncryptedPayload {
        match self {
            PayloadPlaintextDto::Login {
                name,
                username,
                password,
                url,
                notes,
                totp,
            } => EncryptedPayload::Login {
                name,
                username,
                password: SecureStr::new(password),
                url,
                notes,
                totp: totp.map(SecureStr::new),
            },
            PayloadPlaintextDto::Api {
                name,
                app_id,
                secret_key,
                url,
                notes,
            } => EncryptedPayload::Api {
                name,
                app_id,
                secret_key: SecureStr::new(secret_key),
                url,
                notes,
            },
            PayloadPlaintextDto::Ssh {
                name,
                public_key,
                private_key,
                passphrase,
                notes,
            } => EncryptedPayload::Ssh {
                name,
                public_key,
                private_key: private_key.map(SecureStr::new),
                passphrase: passphrase.map(SecureStr::new),
                notes,
            },
            PayloadPlaintextDto::SecureNote { name, notes } => {
                EncryptedPayload::SecureNote { name, notes }
            }
        }
    }
}

pub fn encrypt_payload(
    crypto: &CryptoManager,
    payload: &EncryptedPayload,
    aad: &[u8],
) -> Result<(Vec<u8>, [u8; 24]), String> {
    let dto = PayloadPlaintextDto::from_payload(payload);
    let json = serde_json::to_string(&dto).map_err(|e| e.to_string())?;
    crypto.encrypt(json.as_bytes(), aad)
}

pub fn decrypt_payload(
    crypto: &CryptoManager,
    ciphertext: &[u8],
    nonce: &[u8; 24],
    aad: &[u8],
    _credential_type: CredentialType,
    dek_version: u32,
) -> Result<EncryptedPayload, String> {
    let plaintext = crypto.decrypt(ciphertext, nonce, aad, dek_version)?;
    let json = String::from_utf8(plaintext).map_err(|e| e.to_string())?;
    let dto: PayloadPlaintextDto = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    Ok(dto.into_payload())
}

pub fn decrypt_name_only(
    crypto: &CryptoManager,
    ciphertext: &[u8],
    nonce: &[u8; 24],
    aad: &[u8],
    dek_version: u32,
) -> Result<String, String> {
    let plaintext = crypto.decrypt(ciphertext, nonce, aad, dek_version)?;
    let json = String::from_utf8(plaintext).map_err(|e| e.to_string())?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let inner = unwrap_enum_variant(&value)?;
    inner["name"]
        .as_str()
        .map(String::from)
        .ok_or("name field not found".into())
}

pub fn decrypt_subtitle(
    crypto: &CryptoManager,
    ciphertext: &[u8],
    nonce: &[u8; 24],
    aad: &[u8],
    credential_type: CredentialType,
    dek_version: u32,
) -> Result<String, String> {
    let plaintext = crypto.decrypt(ciphertext, nonce, aad, dek_version)?;
    let json = String::from_utf8(plaintext).map_err(|e| e.to_string())?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let inner = unwrap_enum_variant(&value)?;

    let field = match credential_type {
        CredentialType::Login => "username",
        CredentialType::Api => "app_id",
        CredentialType::Ssh => "public_key",
        CredentialType::SecureNote => {
            // SecureNote has no subtitle field, return empty string
            return Ok(String::new());
        }
    };

    let s = inner[field].as_str().unwrap_or("").to_string();
    if credential_type == CredentialType::Ssh && s.chars().count() > 32 {
        Ok(format!("{}...", s.chars().take(32).collect::<String>()))
    } else {
        Ok(s)
    }
}

/// Externally-tagged enums serialize as `{"VariantName":{...fields...}}`.
/// This helper extracts the inner fields object.
fn unwrap_enum_variant(value: &serde_json::Value) -> Result<&serde_json::Value, String> {
    if let Some(obj) = value.as_object() {
        if let Some((_, v)) = obj.iter().next() {
            return Ok(v);
        }
    }
    Err("expected externally-tagged enum JSON".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::credential::EncryptedPayload;

    #[test]
    fn payload_roundtrip_preserves_secret_fields_without_securestr_serde() {
        let payload = EncryptedPayload::Login {
            name: "example".to_string(),
            username: "alice".to_string(),
            password: SecureStr::new("correct horse".to_string()),
            url: Some("https://example.test".to_string()),
            notes: None,
            totp: None,
        };

        let dto = PayloadPlaintextDto::from_payload(&payload);
        let restored = dto.into_payload();

        match restored {
            EncryptedPayload::Login { password, .. } => {
                assert_eq!(password.expose(), "correct horse");
            }
            _ => panic!("expected login payload"),
        }
    }

    #[test]
    fn payload_roundtrip_preserves_login_totp_secret() {
        let payload = EncryptedPayload::Login {
            name: "example".to_string(),
            username: "alice".to_string(),
            password: SecureStr::new("correct horse".to_string()),
            url: Some("https://example.test".to_string()),
            notes: None,
            totp: Some(SecureStr::new(
                "otpauth://totp/Example:alice?secret=JBSWY3DPEHPK3PXP&issuer=Example".to_string(),
            )),
        };

        let dto = PayloadPlaintextDto::from_payload(&payload);
        let restored = dto.into_payload();

        match restored {
            EncryptedPayload::Login {
                totp: Some(totp), ..
            } => {
                assert_eq!(
                    totp.expose(),
                    "otpauth://totp/Example:alice?secret=JBSWY3DPEHPK3PXP&issuer=Example"
                );
            }
            _ => panic!("expected login payload with totp"),
        }
    }

    #[test]
    fn legacy_login_payload_without_totp_deserializes_with_none() {
        let json = r#"{"Login":{"name":"example","username":"alice","password":"correct horse","url":null,"notes":null}}"#;
        let dto: PayloadPlaintextDto = serde_json::from_str(json).expect("legacy login payload");
        let restored = dto.into_payload();

        match restored {
            EncryptedPayload::Login { totp: None, .. } => {}
            _ => panic!("expected legacy login payload without totp"),
        }
    }
}
