use std::path::Path;
use zeroize::Zeroize;

use crate::crypto::argon2::{self, Argon2Params};
use crate::crypto::hkdf;
use crate::crypto::xchacha20;

#[derive(Zeroize)]
#[zeroize(drop)]
pub struct SecretKey([u8; 32]);

#[derive(Zeroize)]
#[zeroize(drop)]
pub struct WrappingKey([u8; 32]);

#[derive(Zeroize)]
#[zeroize(drop)]
pub struct KeyEncryptionKey([u8; 32]);

#[derive(Zeroize)]
#[zeroize(drop)]
pub struct DataEncryptionKey([u8; 32]);

#[derive(Zeroize)]
#[zeroize(drop)]
pub struct DeviceKey([u8; 32]);

impl SecretKey {
    pub(crate) fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl WrappingKey {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl KeyEncryptionKey {
    pub(crate) fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl DataEncryptionKey {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl DeviceKey {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

pub fn wrap_key(
    key_to_wrap: &[u8; 32],
    wrapping_key: &[u8; 32],
) -> Result<(Vec<u8>, [u8; 24]), String> {
    xchacha20::encrypt(key_to_wrap, wrapping_key)
}

pub fn unwrap_key(
    wrapped: &[u8],
    nonce: &[u8; 24],
    wrapping_key: &[u8; 32],
) -> Result<[u8; 32], String> {
    let plaintext = xchacha20::decrypt(wrapped, nonce, wrapping_key)?;
    let mut key = [0u8; 32];
    key.copy_from_slice(&plaintext);
    Ok(key)
}

#[derive(Zeroize)]
#[zeroize(drop)]
pub struct KeyStore {
    pub(crate) sk: Option<SecretKey>,
    pub(crate) kek: Option<KeyEncryptionKey>,
    pub(crate) current_dek_version: u32,
    pub(crate) device_id: String,
}

impl KeyStore {
    pub fn initialize(path: &Path, sk_bytes: [u8; 32], cmk: &str) -> Result<Self, String> {
        let sk = SecretKey(sk_bytes);
        let kek = KeyEncryptionKey(hkdf::derive_kek(sk.as_bytes())?);

        let salt = argon2::generate_salt();
        let wk = WrappingKey(
            argon2::derive_key_with_params(cmk, &salt, &Argon2Params::medium())?
                .try_into()
                .map_err(|_| "WK derivation failed".to_string())?,
        );

        let (wrapped, nonce) = wrap_key(sk.as_bytes(), wk.as_bytes())?;

        let wrapped_sk = serde_json::json!({
            "version": 1,
            "algorithm": "xchacha20-poly1305",
            "wrapped_sk": base64_encode(&wrapped),
            "nonce": base64_encode(&nonce),
            "kdf": {
                "algorithm": "argon2id",
                "salt": base64_encode(&salt),
                "time_cost": 2,
                "memory_cost": 49152,
                "parallelism": 2,
                "output_len": 32
            },
            "created_at": chrono::Utc::now().to_rfc3339(),
        });

        let content = serde_json::to_string_pretty(&wrapped_sk).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
        let file_path = path.join("wrapped_secret_key.json");
        std::fs::write(&file_path, content).map_err(|e| e.to_string())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&file_path)
                .map_err(|e| e.to_string())?
                .permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&file_path, perms).map_err(|e| e.to_string())?;
        }

        Ok(Self {
            sk: Some(sk),
            kek: Some(kek),
            current_dek_version: 1,
            device_id: uuid::Uuid::new_v4().to_string(),
        })
    }

    pub fn unlock(path: &Path, cmk: &str) -> Result<Self, String> {
        let file_path = path.join("wrapped_secret_key.json");
        let content = std::fs::read_to_string(&file_path).map_err(|e| e.to_string())?;
        let data: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;

        let salt_str = data["kdf"]["salt"].as_str().ok_or("missing salt")?;
        let salt = base64_decode(salt_str)?;
        let mut salt_arr = [0u8; 16];
        salt_arr.copy_from_slice(&salt);

        let wk = WrappingKey(
            argon2::derive_key_with_params(cmk, &salt_arr, &Argon2Params::medium())?
                .try_into()
                .map_err(|_| "WK derivation failed".to_string())?,
        );

        let wrapped = base64_decode(data["wrapped_sk"].as_str().ok_or("missing wrapped_sk")?)?;
        let nonce = base64_decode(data["nonce"].as_str().ok_or("missing nonce")?)?;
        let mut nonce_arr = [0u8; 24];
        nonce_arr.copy_from_slice(&nonce);

        let sk_bytes = unwrap_key(&wrapped, &nonce_arr, wk.as_bytes())?;
        let sk = SecretKey(sk_bytes);
        let kek = KeyEncryptionKey(hkdf::derive_kek(sk.as_bytes())?);

        Ok(Self {
            sk: Some(sk),
            kek: Some(kek),
            current_dek_version: 1,
            device_id: uuid::Uuid::new_v4().to_string(),
        })
    }

    pub fn get_dek(&self, version: u32) -> Result<DataEncryptionKey, String> {
        let kek = self.kek.as_ref().ok_or("KeyStore not unlocked")?;
        Ok(DataEncryptionKey(hkdf::derive_dek(
            kek.as_bytes(),
            version,
        )?))
    }

    pub fn current_dek_version(&self) -> u32 {
        self.current_dek_version
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }
}

fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| e.to_string())
}
