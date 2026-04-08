use crate::crypto::CryptoManager;
use crate::types::credential::{CredentialType, EncryptedPayload};

pub fn encrypt_payload(
    crypto: &CryptoManager,
    payload: &EncryptedPayload,
    aad: &[u8],
) -> Result<(Vec<u8>, [u8; 24]), String> {
    let json = serde_json::to_string(payload).map_err(|e| e.to_string())?;
    crypto.encrypt(json.as_bytes(), aad)
}

pub fn decrypt_payload(
    crypto: &CryptoManager,
    ciphertext: &[u8],
    nonce: &[u8; 24],
    aad: &[u8],
    _credential_type: CredentialType,
) -> Result<EncryptedPayload, String> {
    let plaintext = crypto.decrypt(ciphertext, nonce, aad, crypto.current_dek_version())?;
    let json = String::from_utf8(plaintext).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

pub fn decrypt_name_only(
    crypto: &CryptoManager,
    ciphertext: &[u8],
    nonce: &[u8; 24],
    aad: &[u8],
) -> Result<String, String> {
    let plaintext = crypto.decrypt(ciphertext, nonce, aad, crypto.current_dek_version())?;
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
) -> Result<String, String> {
    let plaintext = crypto.decrypt(ciphertext, nonce, aad, crypto.current_dek_version())?;
    let json = String::from_utf8(plaintext).map_err(|e| e.to_string())?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let inner = unwrap_enum_variant(&value)?;

    let field = match credential_type {
        CredentialType::Login => "username",
        CredentialType::Api => "app_id",
        CredentialType::Ssh => "public_key",
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
