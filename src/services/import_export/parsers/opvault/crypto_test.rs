//! Test helpers for creating valid encrypted .opvault test data.
//!
//! This module provides encryption functions (the reverse of crypto.rs) to create
//! test fixtures for unit testing the OpVault parser.
//!
//! # Encryption Chain (reverse of decryption)
//!
//! 1. Generate random salt and master/overview key materials (32 bytes each)
//! 2. PBKDF2-HMAC-SHA512(password, salt, iterations=1) → 64 bytes → derivedKey(32) + derivedMAC(32)
//! 3. SHA-512(master_material) → masterKey(32) + masterMAC(32) → encrypt with derived keys → profile.masterKey
//! 4. SHA-512(overview_material) → overviewKey(32) + overviewMAC(32) → encrypt with derived keys → profile.overviewKey
//! 5. For each item: generate item_key_material (64 bytes) → SHA-512 → item_enc(32) + item_mac(32)
//! 6. Encrypt item_key_material with masterKey/masterMAC → item.k
//! 7. Encrypt overview JSON with overviewKey/overviewMAC → item.o
//! 8. Encrypt details JSON with itemKey/itemMAC → item.d

use aes::Aes256;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use cbc::cipher::generic_array::GenericArray;
use cbc::cipher::{BlockEncryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac;
use serde_json::json;
use sha2::{Digest, Sha256, Sha512};

use super::types::{KeyPair, Profile};

type Aes256CbcEnc = cbc::Encryptor<Aes256>;
type HmacSha256 = Hmac<Sha256>;

/// OpVault test fixture builder with known password "freddy" and iterations=1.
pub struct OpVaultFixture {
    pub password: String,
    pub iterations: u32,
    pub salt: Vec<u8>,
    pub master_material: Vec<u8>,
    pub overview_material: Vec<u8>,
    pub derived_keys: KeyPair,
    pub master_keys: KeyPair,
    pub overview_keys: KeyPair,
}

impl Default for OpVaultFixture {
    fn default() -> Self {
        Self::new()
    }
}

impl OpVaultFixture {
    /// Create a new fixture with default test password "freddy" and iterations=1.
    pub fn new() -> Self {
        let password = "freddy";
        let iterations = 1;
        let salt = Self::generate_salt();
        let master_material = Self::generate_key_material();
        let overview_material = Self::generate_key_material();

        // Derive keys from password
        let derived_keys = Self::derive_keys(password.as_bytes(), &salt, iterations);

        // Derive master and overview key pairs from materials
        let master_keys = Self::composite_key_from_material(&master_material);
        let overview_keys = Self::composite_key_from_material(&overview_material);

        Self {
            password: password.to_string(),
            iterations,
            salt,
            master_material,
            overview_material,
            derived_keys,
            master_keys,
            overview_keys,
        }
    }

    /// Generate a random 32-byte salt.
    fn generate_salt() -> Vec<u8> {
        rand::random::<[u8; 32]>().to_vec()
    }

    /// Generate random 32-byte key material.
    fn generate_key_material() -> Vec<u8> {
        rand::random::<[u8; 32]>().to_vec()
    }

    /// Derive encryption and HMAC keys from master password using PBKDF2-HMAC-SHA512.
    fn derive_keys(password: &[u8], salt: &[u8], iterations: u32) -> KeyPair {
        let mut out = [0u8; 64];
        pbkdf2_hmac::<Sha512>(password, salt, iterations, &mut out);

        let mut enc = [0u8; 32];
        let mut mac = [0u8; 32];
        enc.copy_from_slice(&out[..32]);
        mac.copy_from_slice(&out[32..]);

        KeyPair { enc, mac }
    }

    /// Derive a composite key pair from key material via SHA-512.
    fn composite_key_from_material(material: &[u8]) -> KeyPair {
        let digest = Sha512::digest(material);

        let mut enc = [0u8; 32];
        let mut mac = [0u8; 32];
        enc.copy_from_slice(&digest[..32]);
        mac.copy_from_slice(&digest[32..64]);

        KeyPair { enc, mac }
    }

    /// Encrypt data in opdata01 format: [8B "opdata01"][8B plaintext_len LE][16B IV][ciphertext][32B HMAC-SHA256]
    /// With front random padding: plaintext is prefixed with random bytes before encryption.
    fn encrypt_opdata01(plaintext: &[u8], key: &[u8; 32], mac_key: &[u8; 32]) -> Vec<u8> {
        let plain_len = plaintext.len() as u64;

        // Generate random IV
        let iv = rand::random::<[u8; 16]>();

        // Add front random padding: make total size a multiple of 16 bytes
        // Minimum padding is 16 bytes to ensure there's at least some random data
        let padded_len = (plaintext.len() + 16).div_ceil(16) * 16; // Round up to next 16-byte block
        let padding_len = padded_len - plaintext.len();

        let mut padded_plaintext = vec![0u8; padded_len];
        // Fill padding with random data using rand::random
        for byte in padded_plaintext.iter_mut().take(padding_len) {
            *byte = rand::random::<u8>();
        }
        padded_plaintext[padding_len..].copy_from_slice(plaintext);

        // Encrypt with AES-256-CBC
        let mut ciphertext = padded_plaintext;
        let mut encryptor =
            Aes256CbcEnc::new_from_slices(key, &iv).expect("invalid key or IV length");

        for chunk in ciphertext.chunks_exact_mut(16) {
            encryptor.encrypt_block_mut(GenericArray::from_mut_slice(chunk));
        }

        // Build opdata01 structure: header + len + iv + ciphertext
        let header = b"opdata01";
        let mut result = Vec::with_capacity(8 + 8 + 16 + ciphertext.len() + 32);
        result.extend_from_slice(header);
        result.extend_from_slice(&plain_len.to_le_bytes());
        result.extend_from_slice(&iv);
        result.extend_from_slice(&ciphertext);

        // Calculate HMAC over everything before the HMAC itself
        let mut mac = HmacSha256::new_from_slice(mac_key).expect("invalid MAC key length");
        mac.update(&result);
        result.extend_from_slice(&mac.finalize().into_bytes());

        result
    }

    /// Encrypt data in opdata01 format and return base64-encoded string.
    fn encrypt_opdata01_b64(plaintext: &[u8], key: &[u8; 32], mac_key: &[u8; 32]) -> String {
        let encrypted = Self::encrypt_opdata01(plaintext, key, mac_key);
        STANDARD.encode(encrypted)
    }

    /// Encrypt item key material: [16B IV][64B AES-256-CBC encrypted][32B HMAC-SHA256]
    /// Input: 64 bytes of key material → splits into item_enc_key(32) + item_mac_key(32)
    /// The item_key_material is NOT hashed first — it's encrypted directly.
    fn encrypt_item_key(item_key_material: &[u8; 64], master: &KeyPair) -> Vec<u8> {
        // Generate random IV
        let iv = rand::random::<[u8; 16]>();

        // Encrypt the 64 bytes of key material
        let mut buf = item_key_material.to_vec();
        let mut encryptor =
            Aes256CbcEnc::new_from_slices(&master.enc, &iv).expect("invalid key or IV length");

        for chunk in buf.chunks_exact_mut(16) {
            encryptor.encrypt_block_mut(GenericArray::from_mut_slice(chunk));
        }

        // Build structure: IV + encrypted keys
        let mut result = Vec::with_capacity(16 + 64 + 32);
        result.extend_from_slice(&iv);
        result.extend_from_slice(&buf);

        // Calculate HMAC over IV + encrypted keys
        let mut mac = HmacSha256::new_from_slice(&master.mac).expect("invalid MAC key length");
        mac.update(&result);
        result.extend_from_slice(&mac.finalize().into_bytes());

        result
    }

    /// Encrypt item key material and return base64-encoded string.
    fn encrypt_item_key_b64(item_key_material: &[u8; 64], master: &KeyPair) -> String {
        let encrypted = Self::encrypt_item_key(item_key_material, master);
        STANDARD.encode(encrypted)
    }

    /// Generate encrypted profile.js content.
    pub fn create_profile_js(&self, uuid: &str) -> String {
        let master_key_b64 = Self::encrypt_opdata01_b64(
            &self.master_material,
            &self.derived_keys.enc,
            &self.derived_keys.mac,
        );
        let overview_key_b64 = Self::encrypt_opdata01_b64(
            &self.overview_material,
            &self.derived_keys.enc,
            &self.derived_keys.mac,
        );
        let salt_b64 = STANDARD.encode(&self.salt);

        let profile = json!({
            "uuid": uuid,
            "lock": {
                "iterations": self.iterations,
                "salt": salt_b64,
                "masterKey": master_key_b64,
                "overviewKey": overview_key_b64
            }
        });

        // OpVault profile.js format: var profile={...};
        format!("var profile={};", profile)
    }

    /// Generate a test Login item with encrypted overview and details.
    pub fn create_login_item(
        &self,
        uuid: &str,
        title: &str,
        username: &str,
        password: &str,
        url: Option<&str>,
    ) -> (String, String, String, String) {
        // Create overview JSON
        let overview_json = if let Some(url) = url {
            json!({
                "title": title,
                "url": url,
                "URLs": [{"u": url}],
                "tags": [],
                "ainfo": null
            })
        } else {
            json!({
                "title": title,
                "tags": [],
                "ainfo": null
            })
        };

        let overview_bytes = serde_json::to_vec(&overview_json).unwrap();
        let overview_b64 = Self::encrypt_opdata01_b64(
            &overview_bytes,
            &self.overview_keys.enc,
            &self.overview_keys.mac,
        );

        // Create details JSON with username and password fields
        let details_json = json!({
            "fields": [
                {
                    "designation": "username",
                    "name": "username",
                    "value": username,
                    "type": "T"
                },
                {
                    "designation": "password",
                    "name": "password",
                    "value": password,
                    "type": "P"
                }
            ],
            "sections": []
        });

        let details_bytes = serde_json::to_vec(&details_json).unwrap();

        // Generate item key material (64 random bytes: enc_key(32) + mac_key(32))
        // The parser uses these directly without hashing.
        let item_key_material: [u8; 64] = rand::random();
        let mut item_enc = [0u8; 32];
        let mut item_mac = [0u8; 32];
        item_enc.copy_from_slice(&item_key_material[..32]);
        item_mac.copy_from_slice(&item_key_material[32..64]);
        let item_keys = KeyPair {
            enc: item_enc,
            mac: item_mac,
        };

        // Encrypt details with item keys
        let details_b64 =
            Self::encrypt_opdata01_b64(&details_bytes, &item_keys.enc, &item_keys.mac);

        // Encrypt item key with master keys
        let key_b64 = Self::encrypt_item_key_b64(&item_key_material, &self.master_keys);

        (uuid.to_string(), overview_b64, key_b64, details_b64)
    }

    /// Generate encrypted band_0.js content with one Login item.
    #[allow(dead_code)]
    pub fn create_band_0_js(
        &self,
        uuid: &str,
        title: &str,
        username: &str,
        password: &str,
        url: Option<&str>,
    ) -> String {
        let (item_uuid, overview, key, details) =
            self.create_login_item(uuid, title, username, password, url);

        let item = json!({
            "uuid": item_uuid,
            "category": "001", // Login category
            "trashed": false,
            "o": overview,
            "k": key,
            "d": details,
            "folder": "",
            "findex": "",
            "created": 0,
            "updated": 0
        });

        // OpVault band format: ld({...});
        format!("ld({});", item)
    }

    /// Create a complete minimal .opvault directory in the given path.
    pub fn create_opvault_dir(&self, base_path: &std::path::Path) -> std::io::Result<()> {
        use std::collections::BTreeMap;
        use std::fs;

        // Create directory structure: .opvault/default/
        let default_dir = base_path.join("default");
        fs::create_dir_all(&default_dir)?;

        // Create profile.js with a fixed UUID
        let profile_js = self.create_profile_js("00000000000000000000000000000000");
        fs::write(default_dir.join("profile.js"), profile_js)?;

        // Create band_0.js with one test item in proper HashMap format
        let (item_uuid, overview, key, details) = self.create_login_item(
            "11111111111111111111111111111111",
            "Test Login",
            "test@example.com",
            "secret123",
            Some("https://example.com"),
        );

        let mut band_map = BTreeMap::new();
        band_map.insert(
            item_uuid.clone(),
            json!({
                "uuid": item_uuid,
                "category": "001",
                "trashed": false,
                "o": overview,
                "k": key,
                "d": details,
                "folder": "",
                "findex": "",
                "created": 0,
                "updated": 0
            }),
        );

        let band_js = format!("ld({})", serde_json::to_string(&band_map).unwrap());
        fs::write(default_dir.join("band_0.js"), band_js)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::import_export::parsers::opvault::crypto::{
        decrypt_keys_from_profile, decrypt_opdata01_b64,
    };

    #[test]
    fn test_fixture_roundtrip() {
        let fixture = OpVaultFixture::new();

        // Verify derived keys match what crypto.rs would produce
        let expected_derived = super::OpVaultFixture::derive_keys(
            fixture.password.as_bytes(),
            &fixture.salt,
            fixture.iterations,
        );
        assert_eq!(fixture.derived_keys.enc, expected_derived.enc);
        assert_eq!(fixture.derived_keys.mac, expected_derived.mac);
    }

    #[test]
    fn test_opdata01_roundtrip() {
        let fixture = OpVaultFixture::new();
        let plaintext = b"Hello, OpVault!";

        // Encrypt
        let encrypted_b64 = OpVaultFixture::encrypt_opdata01_b64(
            plaintext,
            &fixture.master_keys.enc,
            &fixture.master_keys.mac,
        );

        // Decrypt
        let decrypted = decrypt_opdata01_b64(
            &encrypted_b64,
            &fixture.master_keys.enc,
            &fixture.master_keys.mac,
        )
        .unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_profile_decryption() {
        let fixture = OpVaultFixture::new();

        // Create a mock profile
        let master_key_b64 = OpVaultFixture::encrypt_opdata01_b64(
            &fixture.master_material,
            &fixture.derived_keys.enc,
            &fixture.derived_keys.mac,
        );
        let overview_key_b64 = OpVaultFixture::encrypt_opdata01_b64(
            &fixture.overview_material,
            &fixture.derived_keys.enc,
            &fixture.derived_keys.mac,
        );
        let salt_b64 = STANDARD.encode(&fixture.salt);

        let profile = Profile {
            uuid: "00000000000000000000000000000000".to_string(),
            lock: Some(super::super::types::ProfileLock {
                iterations: fixture.iterations,
                salt: salt_b64,
                master_key: master_key_b64,
                overview_key: overview_key_b64,
            }),
            iterations: None,
            salt: None,
            master_key: None,
            overview_key: None,
        };

        // Decrypt keys from profile
        let decrypted_keys =
            decrypt_keys_from_profile(&profile, fixture.password.as_bytes()).unwrap();

        // Verify the keys match our fixture
        assert_eq!(decrypted_keys.master.enc, fixture.master_keys.enc);
        assert_eq!(decrypted_keys.master.mac, fixture.master_keys.mac);
        assert_eq!(decrypted_keys.overview.enc, fixture.overview_keys.enc);
        assert_eq!(decrypted_keys.overview.mac, fixture.overview_keys.mac);
    }

    #[test]
    fn test_create_minimal_opvault() {
        use tempfile::TempDir;

        let fixture = OpVaultFixture::new();
        let temp_dir = TempDir::new().unwrap();

        fixture.create_opvault_dir(temp_dir.path()).unwrap();

        // Verify files exist
        let profile_path = temp_dir.path().join("default/profile.js");
        let band_path = temp_dir.path().join("default/band_0.js");

        assert!(profile_path.exists());
        assert!(band_path.exists());

        // Verify profile.js has correct format
        let profile_content = std::fs::read_to_string(&profile_path).unwrap();
        assert!(profile_content.starts_with("var profile="));
        assert!(profile_content.ends_with(";"));

        // Verify band_0.js has correct format
        let band_content = std::fs::read_to_string(&band_path).unwrap();
        assert!(band_content.starts_with("ld("));
        assert!(band_content.ends_with(")"));
    }
}
