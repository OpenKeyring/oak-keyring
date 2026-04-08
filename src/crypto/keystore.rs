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
    pub fn initialize(
        path: &Path,
        sk_bytes: [u8; 32],
        cmk: &str,
        params: &Argon2Params,
    ) -> Result<Self, String> {
        let sk = SecretKey(sk_bytes);
        let kek = KeyEncryptionKey(hkdf::derive_kek(sk.as_bytes())?);

        let salt = argon2::generate_salt();
        let wk = WrappingKey(
            argon2::derive_key_with_params(cmk, &salt, params)?
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
                "time_cost": params.t_cost,
                "memory_cost": params.m_cost,
                "parallelism": params.p_cost,
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

        let time_cost = data["kdf"]["time_cost"]
            .as_u64()
            .ok_or("missing time_cost")? as u32;
        let memory_cost = data["kdf"]["memory_cost"]
            .as_u64()
            .ok_or("missing memory_cost")? as u32;
        let parallelism = data["kdf"]["parallelism"]
            .as_u64()
            .ok_or("missing parallelism")? as u32;
        let kdf_params = Argon2Params {
            m_cost: memory_cost,
            t_cost: time_cost,
            p_cost: parallelism,
        };

        let wk = WrappingKey(
            argon2::derive_key_with_params(cmk, &salt_arr, &kdf_params)?
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

    pub fn change_cmk(path: &Path, old_cmk: &str, new_cmk: &str) -> Result<(), String> {
        let file_path = path.join("wrapped_secret_key.json");
        let content = std::fs::read_to_string(&file_path).map_err(|e| e.to_string())?;
        let data: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;

        let salt_str = data["kdf"]["salt"].as_str().ok_or("missing salt")?;
        let salt = base64_decode(salt_str)?;
        let mut salt_arr = [0u8; 16];
        salt_arr.copy_from_slice(&salt);

        let time_cost = data["kdf"]["time_cost"]
            .as_u64()
            .ok_or("missing time_cost")? as u32;
        let memory_cost = data["kdf"]["memory_cost"]
            .as_u64()
            .ok_or("missing memory_cost")? as u32;
        let parallelism = data["kdf"]["parallelism"]
            .as_u64()
            .ok_or("missing parallelism")? as u32;
        let kdf_params = Argon2Params {
            m_cost: memory_cost,
            t_cost: time_cost,
            p_cost: parallelism,
        };

        let old_wk = WrappingKey(
            argon2::derive_key_with_params(old_cmk, &salt_arr, &kdf_params)?
                .try_into()
                .map_err(|_| "WK derivation failed".to_string())?,
        );

        let wrapped = base64_decode(data["wrapped_sk"].as_str().ok_or("missing wrapped_sk")?)?;
        let nonce_bytes = base64_decode(data["nonce"].as_str().ok_or("missing nonce")?)?;
        let mut nonce_arr = [0u8; 24];
        nonce_arr.copy_from_slice(&nonce_bytes);

        let sk_bytes = unwrap_key(&wrapped, &nonce_arr, old_wk.as_bytes())?;

        let new_salt = argon2::generate_salt();
        let new_params = Argon2Params::medium();
        let mut new_wk = WrappingKey(
            argon2::derive_key_with_params(new_cmk, &new_salt, &new_params)?
                .try_into()
                .map_err(|_| "WK derivation failed".to_string())?,
        );

        let (new_wrapped, new_nonce) = wrap_key(&sk_bytes, new_wk.as_bytes())?;

        let new_json = serde_json::json!({
            "version": data["version"].as_u64().unwrap_or(1),
            "algorithm": data["algorithm"].as_str().unwrap_or("xchacha20-poly1305"),
            "wrapped_sk": base64_encode(&new_wrapped),
            "nonce": base64_encode(&new_nonce),
            "kdf": {
                "algorithm": "argon2id",
                "salt": base64_encode(&new_salt),
                "time_cost": new_params.t_cost,
                "memory_cost": new_params.m_cost,
                "parallelism": new_params.p_cost,
                "output_len": 32
            },
            "created_at": chrono::Utc::now().to_rfc3339(),
        });

        let new_content = serde_json::to_string_pretty(&new_json).map_err(|e| e.to_string())?;

        let temp_path = file_path.with_extension("json.tmp");
        {
            let mut f = std::fs::File::create(&temp_path).map_err(|e| e.to_string())?;
            use std::io::Write;
            f.write_all(new_content.as_bytes())
                .map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
        }

        std::fs::rename(&temp_path, &file_path).map_err(|e| e.to_string())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&file_path)
                .map_err(|e| e.to_string())?
                .permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&file_path, perms).map_err(|e| e.to_string())?;
        }

        new_wk.zeroize();

        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: initialize a vault with custom KDF params written to the JSON.
    /// This simulates a vault created with non-default (e.g. High) params.
    fn init_with_params(path: &Path, sk_bytes: [u8; 32], cmk: &str, params: &Argon2Params) {
        let salt = argon2::generate_salt();
        let wk = WrappingKey(
            argon2::derive_key_with_params(cmk, &salt, params)
                .unwrap()
                .try_into()
                .unwrap(),
        );
        let (wrapped, nonce) = wrap_key(&sk_bytes, wk.as_bytes()).unwrap();

        let json = serde_json::json!({
            "version": 1,
            "algorithm": "xchacha20-poly1305",
            "wrapped_sk": base64_encode(&wrapped),
            "nonce": base64_encode(&nonce),
            "kdf": {
                "algorithm": "argon2id",
                "salt": base64_encode(&salt),
                "time_cost": params.t_cost,
                "memory_cost": params.m_cost,
                "parallelism": params.p_cost,
                "output_len": 32
            },
            "created_at": chrono::Utc::now().to_rfc3339(),
        });

        std::fs::create_dir_all(path).unwrap();
        std::fs::write(
            path.join("wrapped_secret_key.json"),
            serde_json::to_string_pretty(&json).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn test_unlock_reads_kdf_params() {
        // B1 regression: vault created with High params must be unlockable.
        // Before the fix, unlock() hardcoded Argon2Params::medium() and would
        // derive a wrong WK, causing unwrap to fail.
        let dir = TempDir::new().unwrap();
        let sk_bytes = [0xABu8; 32];
        let cmk = "correct-master-key";
        let high_params = Argon2Params::high();

        init_with_params(dir.path(), sk_bytes, cmk, &high_params);

        let store = KeyStore::unlock(dir.path(), cmk).unwrap();
        assert!(
            store.sk.is_some(),
            "unlock must succeed when reading KDF params from JSON"
        );
        assert_eq!(store.sk.as_ref().unwrap().as_bytes(), &sk_bytes);
    }

    #[test]
    fn test_initialize_unlock_roundtrip() {
        // initialize → unlock must recover the same SK.
        let dir = TempDir::new().unwrap();
        let sk_bytes = [0xCDu8; 32];
        let cmk = "roundtrip-password";

        let created =
            KeyStore::initialize(dir.path(), sk_bytes, cmk, &Argon2Params::medium()).unwrap();
        let opened = KeyStore::unlock(dir.path(), cmk).unwrap();

        assert_eq!(
            created.sk.as_ref().unwrap().as_bytes(),
            opened.sk.as_ref().unwrap().as_bytes(),
            "SK after unlock must match original SK"
        );
    }

    #[test]
    fn test_unlock_wrong_cmk_fails() {
        let dir = TempDir::new().unwrap();
        let sk_bytes = [0xEFu8; 32];

        KeyStore::initialize(
            dir.path(),
            sk_bytes,
            "right-password",
            &Argon2Params::medium(),
        )
        .unwrap();

        let result = KeyStore::unlock(dir.path(), "wrong-password");
        assert!(result.is_err(), "unlock with wrong CMK must fail");
    }

    #[test]
    fn test_change_cmk_roundtrip() {
        let dir = TempDir::new().unwrap();
        let sk_bytes = [0x11u8; 32];
        let old_cmk = "old-master-password";
        let new_cmk = "new-master-password";

        KeyStore::initialize(dir.path(), sk_bytes, old_cmk, &Argon2Params::medium()).unwrap();

        KeyStore::change_cmk(dir.path(), old_cmk, new_cmk).unwrap();

        let result = KeyStore::unlock(dir.path(), new_cmk);
        assert!(
            result.is_ok(),
            "unlock with new CMK must succeed after change_cmk"
        );
        assert_eq!(
            result.unwrap().sk.as_ref().unwrap().as_bytes(),
            &sk_bytes,
            "SK must be preserved after CMK change"
        );

        let old_result = KeyStore::unlock(dir.path(), old_cmk);
        assert!(
            old_result.is_err(),
            "unlock with old CMK must fail after change_cmk"
        );
    }

    #[test]
    fn test_change_cmk_wrong_old_fails() {
        let dir = TempDir::new().unwrap();
        let sk_bytes = [0x22u8; 32];

        KeyStore::initialize(dir.path(), sk_bytes, "correct-old", &Argon2Params::medium()).unwrap();

        let result = KeyStore::change_cmk(dir.path(), "wrong-old", "any-new");
        assert!(result.is_err(), "change_cmk with wrong old CMK must fail");

        let unlock = KeyStore::unlock(dir.path(), "correct-old");
        assert!(
            unlock.is_ok(),
            "original CMK must still work after failed change_cmk"
        );
    }

    #[test]
    fn test_wrap_unwrap_roundtrip() {
        let key = [0x42u8; 32];
        let wrapping = [0xABu8; 32];
        let (wrapped, nonce) = wrap_key(&key, &wrapping).unwrap();
        let recovered = unwrap_key(&wrapped, &nonce, &wrapping).unwrap();
        assert_eq!(recovered, key, "unwrapped key must match original");
    }

    #[test]
    fn test_unwrap_wrong_key_fails() {
        let key = [0x42u8; 32];
        let wrapping = [0xABu8; 32];
        let (wrapped, nonce) = wrap_key(&key, &wrapping).unwrap();
        let wrong_wrapping = [0xCDu8; 32];
        let result = unwrap_key(&wrapped, &nonce, &wrong_wrapping);
        assert!(result.is_err(), "unwrap with wrong wrapping key must fail");
    }

    #[test]
    fn test_key_newtype_zeroize() {
        let key = SecretKey::new([0xFFu8; 32]);
        assert_eq!(key.as_bytes(), &[0xFFu8; 32]);
        drop(key);
    }

    #[test]
    fn test_initialize_creates_file() {
        let dir = TempDir::new().unwrap();
        let sk_bytes = [0x77u8; 32];
        KeyStore::initialize(
            dir.path(),
            sk_bytes,
            "file-test-cmk",
            &Argon2Params::medium(),
        )
        .unwrap();
        let file_path = dir.path().join("wrapped_secret_key.json");
        assert!(
            file_path.exists(),
            "wrapped_secret_key.json must be created"
        );
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(!content.is_empty(), "file must not be empty");
    }

    #[test]
    fn test_json_format_matches_spec() {
        let dir = TempDir::new().unwrap();
        let sk_bytes = [0x88u8; 32];
        KeyStore::initialize(
            dir.path(),
            sk_bytes,
            "json-test-cmk",
            &Argon2Params::medium(),
        )
        .unwrap();
        let file_path = dir.path().join("wrapped_secret_key.json");
        let content = std::fs::read_to_string(&file_path).unwrap();
        let data: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert!(data["version"].is_number(), "version must exist");
        assert_eq!(data["version"].as_u64(), Some(1));
        assert!(data["algorithm"].is_string(), "algorithm must exist");
        assert_eq!(data["algorithm"].as_str(), Some("xchacha20-poly1305"));
        assert!(data["wrapped_sk"].is_string(), "wrapped_sk must exist");
        assert!(data["nonce"].is_string(), "nonce must exist");
        assert!(data["created_at"].is_string(), "created_at must exist");

        let kdf = &data["kdf"];
        assert!(kdf["algorithm"].is_string(), "kdf.algorithm must exist");
        assert_eq!(kdf["algorithm"].as_str(), Some("argon2id"));
        assert!(kdf["salt"].is_string(), "kdf.salt must exist");
        assert!(kdf["time_cost"].is_number(), "kdf.time_cost must exist");
        assert!(kdf["memory_cost"].is_number(), "kdf.memory_cost must exist");
        assert!(kdf["parallelism"].is_number(), "kdf.parallelism must exist");
        assert!(kdf["output_len"].is_number(), "kdf.output_len must exist");
        assert_eq!(kdf["output_len"].as_u64(), Some(32));
    }

    #[test]
    fn test_file_permissions_600() {
        let dir = TempDir::new().unwrap();
        let sk_bytes = [0x99u8; 32];
        KeyStore::initialize(
            dir.path(),
            sk_bytes,
            "perm-test-cmk",
            &Argon2Params::medium(),
        )
        .unwrap();
        let file_path = dir.path().join("wrapped_secret_key.json");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&file_path).unwrap().permissions().mode();
            assert_eq!(
                mode & 0o777,
                0o600,
                "file permissions must be 0o600, got {:o}",
                mode & 0o777
            );
        }

        #[cfg(not(unix))]
        {
            assert!(file_path.exists());
        }
    }
}
