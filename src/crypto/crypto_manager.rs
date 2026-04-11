use std::path::Path;

use crate::crypto::bip39::{MnemonicLanguage, Passkey};
use crate::crypto::hkdf;
use crate::crypto::keystore::{DataEncryptionKey, KeyEncryptionKey, KeyStore, SecretKey};
use crate::crypto::xchacha20;

pub struct CryptoManager {
    keystore: Option<KeyStore>,
    current_dek_version: u32,
}

impl CryptoManager {
    pub fn new() -> Self {
        Self {
            keystore: None,
            current_dek_version: 1,
        }
    }

    pub fn unlock(&mut self, path: &Path, cmk: &str) -> Result<(), String> {
        let ks = KeyStore::unlock(path, cmk)?;
        self.current_dek_version = ks.current_dek_version();
        self.keystore = Some(ks);
        Ok(())
    }

    pub fn unlock_with_mnemonic(&mut self, mnemonic: &Passkey) -> Result<(), String> {
        let seed = mnemonic.to_seed(None)?;
        let sk_bytes = seed.to_secret_key();
        let kek_bytes = hkdf::derive_kek(&sk_bytes)?;

        let ks = KeyStore {
            sk: Some(SecretKey::new(sk_bytes)),
            kek: Some(KeyEncryptionKey::new(kek_bytes)),
            current_dek_version: 1,
            device_id: uuid::Uuid::new_v4().to_string(),
            mnemonic_language: MnemonicLanguage::English,
        };

        self.keystore = Some(ks);
        self.current_dek_version = 1;
        Ok(())
    }

    pub fn lock(&mut self) {
        self.keystore = None;
    }

    pub fn encrypt(&self, plaintext: &[u8], aad: &[u8]) -> Result<(Vec<u8>, [u8; 24]), String> {
        let dek = self.get_current_dek()?;
        xchacha20::encrypt_with_aad(plaintext, aad, dek.as_bytes())
            .map(|data| (data.ciphertext, data.nonce))
            .map_err(|e| e.to_string())
    }

    pub fn decrypt(
        &self,
        ciphertext: &[u8],
        nonce: &[u8; 24],
        aad: &[u8],
        dek_version: u32,
    ) -> Result<Vec<u8>, String> {
        let dek = self.get_dek(dek_version)?;
        xchacha20::decrypt_with_aad(
            &xchacha20::EncryptedData {
                ciphertext: ciphertext.to_vec(),
                nonce: *nonce,
            },
            aad,
            dek.as_bytes(),
        )
        .map_err(|e| e.to_string())
    }

    pub fn is_unlocked(&self) -> bool {
        self.keystore.is_some()
    }

    pub fn current_dek_version(&self) -> u32 {
        self.current_dek_version
    }

    pub fn get_dek(&self, version: u32) -> Result<DataEncryptionKey, String> {
        let ks = self.keystore.as_ref().ok_or("CryptoManager not unlocked")?;
        ks.get_dek(version)
    }

    fn get_current_dek(&self) -> Result<DataEncryptionKey, String> {
        self.get_dek(self.current_dek_version)
    }
}

impl Default for CryptoManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CryptoManager {
    fn drop(&mut self) {
        self.lock();
    }
}
