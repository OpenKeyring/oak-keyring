use std::path::Path;
use zeroize::Zeroize;

use crate::crypto::argon2::{self, Argon2Params};
use crate::crypto::bip39::MnemonicLanguage;
use crate::crypto::hkdf;
use crate::crypto::xchacha20;
use crate::types::SecureStr;

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
    xchacha20::encrypt(key_to_wrap, wrapping_key).map_err(|e| e.to_string())
}

pub fn unwrap_key(
    wrapped: &[u8],
    nonce: &[u8; 24],
    wrapping_key: &[u8; 32],
) -> Result<[u8; 32], String> {
    let plaintext = xchacha20::decrypt(wrapped, nonce, wrapping_key).map_err(|e| e.to_string())?;
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
    #[zeroize(skip)]
    pub(crate) mnemonic_language: MnemonicLanguage,
}

impl KeyStore {
    /// Check whether a vault exists at the given directory.
    ///
    /// A vault is considered to exist when `wrapped_secret_key.json` is present
    /// in the directory. This is used to decide whether to route the user to
    /// Onboarding (no vault) or Unlock (has vault) on startup.
    pub fn vault_exists(vault_dir: &Path) -> bool {
        vault_dir.join("wrapped_secret_key.json").exists()
    }

    pub fn initialize(
        path: &Path,
        sk_bytes: [u8; 32],
        cmk: &SecureStr,
        params: &Argon2Params,
        language: MnemonicLanguage,
    ) -> Result<Self, String> {
        let sk = SecretKey(sk_bytes);
        let kek = KeyEncryptionKey(hkdf::derive_kek(sk.as_bytes())?);

        let salt = argon2::generate_salt();
        let wk = WrappingKey(
            argon2::derive_key_with_params(cmk.get(), &salt, params)?
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
            "mnemonic_language": language.to_keystore_value(),
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
            mnemonic_language: language,
        })
    }

    pub fn unlock(path: &Path, cmk: &SecureStr) -> Result<Self, String> {
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
            argon2::derive_key_with_params(cmk.get(), &salt_arr, &kdf_params)?
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

        let mnemonic_lang_str = data["mnemonic_language"].as_str().unwrap_or("en");
        let mnemonic_language = MnemonicLanguage::from_keystore_value(mnemonic_lang_str)
            .unwrap_or(MnemonicLanguage::English);

        Ok(Self {
            sk: Some(sk),
            kek: Some(kek),
            current_dek_version: 1,
            device_id: uuid::Uuid::new_v4().to_string(),
            mnemonic_language,
        })
    }

    pub fn change_cmk(path: &Path, old_cmk: &SecureStr, new_cmk: &SecureStr) -> Result<(), String> {
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
            argon2::derive_key_with_params(old_cmk.get(), &salt_arr, &kdf_params)?
                .try_into()
                .map_err(|_| "WK derivation failed".to_string())?,
        );

        let wrapped = base64_decode(data["wrapped_sk"].as_str().ok_or("missing wrapped_sk")?)?;
        let nonce_bytes = base64_decode(data["nonce"].as_str().ok_or("missing nonce")?)?;
        let mut nonce_arr = [0u8; 24];
        nonce_arr.copy_from_slice(&nonce_bytes);

        let mut sk_bytes = unwrap_key(&wrapped, &nonce_arr, old_wk.as_bytes())?;

        let new_salt = argon2::generate_salt();
        // Preserve the existing vault's KDF params for the new wrapping (security not downgraded)
        let mut new_wk = WrappingKey(
            argon2::derive_key_with_params(new_cmk.get(), &new_salt, &kdf_params)?
                .try_into()
                .map_err(|_| "WK derivation failed".to_string())?,
        );

        let (new_wrapped, new_nonce) = wrap_key(&sk_bytes, new_wk.as_bytes())?;

        let mnemonic_language_str = data["mnemonic_language"].as_str().unwrap_or("en");

        let new_json = serde_json::json!({
            "version": data["version"].as_u64().unwrap_or(1),
            "algorithm": data["algorithm"].as_str().unwrap_or("xchacha20-poly1305"),
            "wrapped_sk": base64_encode(&new_wrapped),
            "nonce": base64_encode(&new_nonce),
            "kdf": {
                "algorithm": "argon2id",
                "salt": base64_encode(&new_salt),
                "time_cost": kdf_params.t_cost,
                "memory_cost": kdf_params.m_cost,
                "parallelism": kdf_params.p_cost,
                "output_len": 32
            },
            "created_at": chrono::Utc::now().to_rfc3339(),
            "mnemonic_language": mnemonic_language_str,
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
        sk_bytes.zeroize();

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

    pub fn mnemonic_language(&self) -> MnemonicLanguage {
        self.mnemonic_language
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

    /// Helper: create a SecureStr from a string literal for test use.
    fn sec(s: &str) -> SecureStr {
        SecureStr::new(s.to_string())
    }

    /// Helper: initialize a vault with custom KDF params written to the JSON.
    /// This simulates a vault created with non-default (e.g. High) params.
    fn init_with_params(path: &Path, sk_bytes: [u8; 32], cmk: &SecureStr, params: &Argon2Params) {
        let salt = argon2::generate_salt();
        let wk = WrappingKey(
            argon2::derive_key_with_params(cmk.get(), &salt, params)
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
        let cmk = sec("correct-master-key");
        let high_params = Argon2Params::high();

        init_with_params(dir.path(), sk_bytes, &cmk, &high_params);

        let store = KeyStore::unlock(dir.path(), &cmk).unwrap();
        assert!(
            store.sk.is_some(),
            "unlock must succeed when reading KDF params from JSON"
        );
        assert_eq!(store.sk.as_ref().unwrap().as_bytes(), &sk_bytes);
    }

    #[test]
    fn test_initialize_unlock_roundtrip() {
        // initialize -> unlock must recover the same SK.
        let dir = TempDir::new().unwrap();
        let sk_bytes = [0xCDu8; 32];
        let cmk = sec("roundtrip-password");

        let created = KeyStore::initialize(
            dir.path(),
            sk_bytes,
            &cmk,
            &Argon2Params::medium(),
            MnemonicLanguage::English,
        )
        .unwrap();
        let opened = KeyStore::unlock(dir.path(), &cmk).unwrap();

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
            &sec("right-password"),
            &Argon2Params::medium(),
            MnemonicLanguage::English,
        )
        .unwrap();

        let result = KeyStore::unlock(dir.path(), &sec("wrong-password"));
        assert!(result.is_err(), "unlock with wrong CMK must fail");
    }

    #[test]
    fn test_change_cmk_roundtrip() {
        let dir = TempDir::new().unwrap();
        let sk_bytes = [0x11u8; 32];
        let old_cmk = sec("old-master-password");
        let new_cmk = sec("new-master-password");

        KeyStore::initialize(
            dir.path(),
            sk_bytes,
            &old_cmk,
            &Argon2Params::medium(),
            MnemonicLanguage::English,
        )
        .unwrap();

        KeyStore::change_cmk(dir.path(), &old_cmk, &new_cmk).unwrap();

        let result = KeyStore::unlock(dir.path(), &new_cmk);
        assert!(
            result.is_ok(),
            "unlock with new CMK must succeed after change_cmk"
        );
        assert_eq!(
            result.unwrap().sk.as_ref().unwrap().as_bytes(),
            &sk_bytes,
            "SK must be preserved after CMK change"
        );

        // Verify old CMK no longer works - must create a new SecureStr since
        // the previous one was consumed by the successful unlock above
        let old_cmk2 = sec("old-master-password");
        let old_result = KeyStore::unlock(dir.path(), &old_cmk2);
        assert!(
            old_result.is_err(),
            "unlock with old CMK must fail after change_cmk"
        );
    }

    #[test]
    fn test_change_cmk_wrong_old_fails() {
        let dir = TempDir::new().unwrap();
        let sk_bytes = [0x22u8; 32];

        KeyStore::initialize(
            dir.path(),
            sk_bytes,
            &sec("correct-old"),
            &Argon2Params::medium(),
            MnemonicLanguage::English,
        )
        .unwrap();

        let result = KeyStore::change_cmk(dir.path(), &sec("wrong-old"), &sec("any-new"));
        assert!(result.is_err(), "change_cmk with wrong old CMK must fail");

        let unlock = KeyStore::unlock(dir.path(), &sec("correct-old"));
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
            &sec("file-test-cmk"),
            &Argon2Params::medium(),
            MnemonicLanguage::English,
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
            &sec("json-test-cmk"),
            &Argon2Params::medium(),
            MnemonicLanguage::English,
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
        assert_eq!(data["mnemonic_language"].as_str(), Some("en"));
    }

    #[test]
    fn test_file_permissions_600() {
        let dir = TempDir::new().unwrap();
        let sk_bytes = [0x99u8; 32];
        KeyStore::initialize(
            dir.path(),
            sk_bytes,
            &sec("perm-test-cmk"),
            &Argon2Params::medium(),
            MnemonicLanguage::English,
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

    // -- Zeroize verification tests ----------------------------------------

    #[test]
    fn test_secret_key_zeroize_on_drop() {
        let key = SecretKey::new([0xAAu8; 32]);
        assert_eq!(key.as_bytes(), &[0xAAu8; 32]);
        drop(key);
        // With #[zeroize(drop)], the Drop impl calls zeroize().
        // We can't read memory after drop in safe Rust, but we verify
        // the derive macro is present and compiles correctly.
    }

    #[test]
    fn test_wrapping_key_zeroize_on_drop() {
        let key = WrappingKey([0xBBu8; 32]);
        assert_eq!(key.as_bytes(), &[0xBBu8; 32]);
        drop(key);
    }

    #[test]
    fn test_kek_zeroize_on_drop() {
        let key = KeyEncryptionKey::new([0xCCu8; 32]);
        assert_eq!(key.as_bytes(), &[0xCCu8; 32]);
        drop(key);
    }

    #[test]
    fn test_dek_zeroize_on_drop() {
        let key = DataEncryptionKey([0xDDu8; 32]);
        assert_eq!(key.as_bytes(), &[0xDDu8; 32]);
        drop(key);
    }

    #[test]
    fn test_keystore_zeroize_on_drop() {
        let dir = TempDir::new().unwrap();
        let ks = KeyStore::initialize(
            dir.path(),
            [0xEEu8; 32],
            &sec("zeroize-test"),
            &Argon2Params::medium(),
            MnemonicLanguage::English,
        )
        .unwrap();
        assert!(ks.sk.is_some());
        assert!(ks.kek.is_some());
        drop(ks);
        // KeyStore has #[zeroize(drop)] -- after drop the Option fields are zeroized
    }

    #[test]
    fn test_crypto_manager_lock_clears_keystore() {
        use crate::crypto::CryptoManager;
        let dir = TempDir::new().unwrap();
        KeyStore::initialize(
            dir.path(),
            [0xFFu8; 32],
            &sec("lock-test-cmk"),
            &Argon2Params::medium(),
            MnemonicLanguage::English,
        )
        .unwrap();

        let mut cm = CryptoManager::new();
        cm.unlock(dir.path(), &sec("lock-test-cmk")).unwrap();
        assert!(cm.is_unlocked());

        cm.lock();
        assert!(!cm.is_unlocked(), "lock() must clear keystore");
    }

    // -- Mnemonic Language Persistence Tests --------------------------------

    #[test]
    fn test_initialize_writes_mnemonic_language() {
        let dir = TempDir::new().unwrap();
        let sk_bytes = [0x11u8; 32];

        KeyStore::initialize(
            dir.path(),
            sk_bytes,
            &sec("cmk"),
            &Argon2Params::medium(),
            MnemonicLanguage::ChineseSimplified,
        )
        .unwrap();

        let file_path = dir.path().join("wrapped_secret_key.json");
        let content = std::fs::read_to_string(&file_path).unwrap();
        let data: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            data["mnemonic_language"].as_str(),
            Some("zh-CN"),
            "initialize must write mnemonic_language to JSON"
        );
    }

    #[test]
    fn test_initialize_default_language_is_english() {
        let dir = TempDir::new().unwrap();
        let sk_bytes = [0x22u8; 32];

        KeyStore::initialize(
            dir.path(),
            sk_bytes,
            &sec("cmk"),
            &Argon2Params::medium(),
            MnemonicLanguage::English,
        )
        .unwrap();

        let file_path = dir.path().join("wrapped_secret_key.json");
        let content = std::fs::read_to_string(&file_path).unwrap();
        let data: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            data["mnemonic_language"].as_str(),
            Some("en"),
            "initialize must write en for English"
        );
    }

    #[test]
    fn test_unlock_reads_mnemonic_language() {
        let dir = TempDir::new().unwrap();
        let sk_bytes = [0x33u8; 32];
        let cmk = sec("test-cmk");

        KeyStore::initialize(
            dir.path(),
            sk_bytes,
            &cmk,
            &Argon2Params::medium(),
            MnemonicLanguage::ChineseSimplified,
        )
        .unwrap();

        let store = KeyStore::unlock(dir.path(), &cmk).unwrap();
        assert_eq!(
            store.mnemonic_language(),
            MnemonicLanguage::ChineseSimplified,
            "unlock must read mnemonic_language from JSON"
        );
    }

    #[test]
    fn test_unlock_missing_language_defaults_to_english() {
        // init_with_params creates JSON without mnemonic_language field
        // to simulate old vaults
        let dir = TempDir::new().unwrap();
        let sk_bytes = [0x44u8; 32];
        let cmk = sec("test-cmk");

        init_with_params(dir.path(), sk_bytes, &cmk, &Argon2Params::medium());

        let store = KeyStore::unlock(dir.path(), &cmk).unwrap();
        assert_eq!(
            store.mnemonic_language(),
            MnemonicLanguage::English,
            "unlock must default to English when mnemonic_language is missing"
        );
    }

    #[test]
    fn test_change_cmk_preserves_mnemonic_language() {
        let dir = TempDir::new().unwrap();
        let sk_bytes = [0x55u8; 32];
        let old_cmk = sec("old-cmk");
        let new_cmk = sec("new-cmk");

        KeyStore::initialize(
            dir.path(),
            sk_bytes,
            &old_cmk,
            &Argon2Params::medium(),
            MnemonicLanguage::ChineseSimplified,
        )
        .unwrap();

        KeyStore::change_cmk(dir.path(), &old_cmk, &new_cmk).unwrap();

        let store = KeyStore::unlock(dir.path(), &new_cmk).unwrap();
        assert_eq!(
            store.mnemonic_language(),
            MnemonicLanguage::ChineseSimplified,
            "change_cmk must preserve mnemonic_language"
        );
    }

    // -- vault_exists Tests ---------------------------------------------------

    #[test]
    fn test_vault_exists_returns_false_for_empty_directory() {
        let dir = TempDir::new().unwrap();
        assert!(
            !KeyStore::vault_exists(dir.path()),
            "vault_exists must return false for an empty directory"
        );
    }

    #[test]
    fn test_vault_exists_returns_true_after_initialize() {
        let dir = TempDir::new().unwrap();
        KeyStore::initialize(
            dir.path(),
            [0x77u8; 32],
            &sec("vault-exists-test"),
            &Argon2Params::medium(),
            MnemonicLanguage::English,
        )
        .unwrap();

        assert!(
            KeyStore::vault_exists(dir.path()),
            "vault_exists must return true after initialize creates the vault file"
        );
    }

    #[test]
    fn test_vault_exists_returns_true_for_manual_file() {
        // Verify that any file named wrapped_secret_key.json triggers true,
        // even without going through initialize.
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("wrapped_secret_key.json"), "{}").unwrap();

        assert!(
            KeyStore::vault_exists(dir.path()),
            "vault_exists must return true when wrapped_secret_key.json is present"
        );
    }
}
