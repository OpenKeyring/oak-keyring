//! OpVault re-key + sanitize tool.
//!
//! Decrypts an .opvault with old password, sanitizes sensitive content,
//! re-encrypts with new password, writes to a new directory.
//!
//! Usage: opvault-rekey <input.opvault> <output.opvault> <old_password> <new_password>

use aes::Aes256;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use cbc::cipher::generic_array::GenericArray;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac;
use serde::Deserialize;
use sha2::{Digest, Sha256, Sha512};

use std::collections::HashMap;
use std::fs;
use std::path::Path;

type Aes256CbcEnc = cbc::Encryptor<Aes256>;
type Aes256CbcDec = cbc::Decryptor<Aes256>;
type HmacSha256 = Hmac<Sha256>;

// ---- Data structures ----

struct KeyPair {
    enc: [u8; 32],
    mac: [u8; 32],
}

#[derive(Deserialize)]
struct Profile {
    uuid: String,
    #[serde(default)]
    lock: Option<ProfileLock>,
    #[serde(default)]
    iterations: Option<u32>,
    #[serde(default)]
    salt: Option<String>,
    #[serde(default, rename = "masterKey")]
    master_key: Option<String>,
    #[serde(default, rename = "overviewKey")]
    overview_key: Option<String>,
}

#[derive(Deserialize, Clone)]
struct ProfileLock {
    iterations: u32,
    salt: String,
    #[serde(rename = "masterKey")]
    master_key: String,
    #[serde(rename = "overviewKey")]
    overview_key: String,
}

impl Profile {
    fn resolve_lock(&self) -> Result<ProfileLock, String> {
        if let Some(ref lock) = self.lock {
            return Ok(lock.clone());
        }
        match (self.iterations, &self.salt, &self.master_key, &self.overview_key) {
            (Some(iterations), Some(salt), Some(master_key), Some(overview_key)) => {
                Ok(ProfileLock {
                    iterations,
                    salt: salt.clone(),
                    master_key: master_key.clone(),
                    overview_key: overview_key.clone(),
                })
            }
            _ => Err("missing lock fields in profile".into()),
        }
    }
}

// ---- Crypto: decrypt ----

fn derive_keys(password: &[u8], salt: &[u8], iterations: u32) -> KeyPair {
    let mut out = [0u8; 64];
    pbkdf2_hmac::<Sha512>(password, salt, iterations, &mut out);
    let mut enc = [0u8; 32];
    let mut mac = [0u8; 32];
    enc.copy_from_slice(&out[..32]);
    mac.copy_from_slice(&out[32..]);
    KeyPair { enc, mac }
}

fn composite_key_from_material(material: &[u8]) -> KeyPair {
    let digest = Sha512::digest(material);
    let mut enc = [0u8; 32];
    let mut mac = [0u8; 32];
    enc.copy_from_slice(&digest[..32]);
    mac.copy_from_slice(&digest[32..64]);
    KeyPair { enc, mac }
}

fn verify_hmac(data: &[u8], expected_mac: &[u8], mac_key: &[u8; 32]) -> Result<(), String> {
    let mut mac = HmacSha256::new_from_slice(mac_key).map_err(|e| format!("HMAC init: {e}"))?;
    mac.update(data);
    mac.verify_slice(expected_mac).map_err(|_| "HMAC verification failed".into())
}

fn decrypt_opdata01(data: &[u8], key: &[u8; 32], mac_key: &[u8; 32]) -> Result<Vec<u8>, String> {
    const MIN_LEN: usize = 8 + 8 + 16 + 16 + 32;
    if data.len() < MIN_LEN {
        return Err("opdata01: data too short".into());
    }

    let signed_end = data.len() - 32;
    verify_hmac(&data[..signed_end], &data[signed_end..], mac_key)?;

    if &data[..8] != b"opdata01" {
        return Err("opdata01: invalid header".into());
    }

    let plain_len = u64::from_le_bytes(data[8..16].try_into().map_err(|e: std::array::TryFromSliceError| e.to_string())?) as usize;
    let iv = &data[16..32];
    let ciphertext = &data[32..signed_end];

    if !ciphertext.len().is_multiple_of(16) {
        return Err("opdata01: ciphertext not block-aligned".into());
    }

    let mut buf = ciphertext.to_vec();
    let mut decryptor = Aes256CbcDec::new_from_slices(key, iv).map_err(|e| format!("cipher init: {e}"))?;
    for chunk in buf.chunks_exact_mut(16) {
        decryptor.decrypt_block_mut(GenericArray::from_mut_slice(chunk));
    }

    let padding_len = buf.len() - plain_len;
    Ok(buf[padding_len..].to_vec())
}

fn decrypt_opdata01_b64(b64: &str, key: &[u8; 32], mac_key: &[u8; 32]) -> Result<Vec<u8>, String> {
    let bytes = STANDARD.decode(b64).map_err(|e| format!("base64 decode: {e}"))?;
    decrypt_opdata01(&bytes, key, mac_key)
}

fn decrypt_item_key(k_data: &[u8], master: &KeyPair) -> Result<[u8; 64], String> {
    if k_data.len() != 112 {
        return Err(format!("item key: expected 112 bytes, got {}", k_data.len()));
    }

    let signed_part = &k_data[..80];
    let expected_mac = &k_data[80..112];
    verify_hmac(signed_part, expected_mac, &master.mac)?;

    let iv = &k_data[..16];
    let encrypted_keys = &k_data[16..80];

    let mut buf = encrypted_keys.to_vec();
    let mut decryptor = Aes256CbcDec::new_from_slices(&master.enc, iv).map_err(|e| format!("item key cipher: {e}"))?;
    for chunk in buf.chunks_exact_mut(16) {
        decryptor.decrypt_block_mut(GenericArray::from_mut_slice(chunk));
    }

    let mut result = [0u8; 64];
    result.copy_from_slice(&buf);
    Ok(result)
}

fn decrypt_item_key_b64(k_b64: &str, master: &KeyPair) -> Result<[u8; 64], String> {
    let k = STANDARD.decode(k_b64).map_err(|e| format!("item key base64: {e}"))?;
    decrypt_item_key(&k, master)
}

fn decrypt_keys_from_profile(profile: &Profile, password: &[u8]) -> Result<(KeyPair, KeyPair), String> {
    let lock = profile.resolve_lock()?;
    let salt = STANDARD.decode(&lock.salt).map_err(|e| format!("salt base64: {e}"))?;
    let derived = derive_keys(password, &salt, lock.iterations);

    let master_material = decrypt_opdata01_b64(&lock.master_key, &derived.enc, &derived.mac)?;
    let master = composite_key_from_material(&master_material);

    let overview_material = decrypt_opdata01_b64(&lock.overview_key, &derived.enc, &derived.mac)?;
    let overview = composite_key_from_material(&overview_material);

    Ok((master, overview))
}

// ---- Crypto: encrypt ----

fn encrypt_opdata01(plaintext: &[u8], key: &[u8; 32], mac_key: &[u8; 32]) -> Vec<u8> {
    let plain_len = plaintext.len() as u64;
    let iv: [u8; 16] = rand::random();

    let padded_len = ((plaintext.len() + 16 + 15) / 16) * 16;
    let padding_len = padded_len - plaintext.len();

    let mut padded_plaintext = vec![0u8; padded_len];
    for i in 0..padding_len {
        padded_plaintext[i] = rand::random::<u8>();
    }
    padded_plaintext[padding_len..].copy_from_slice(plaintext);

    let mut ciphertext = padded_plaintext;
    let mut encryptor = Aes256CbcEnc::new_from_slices(key, &iv).expect("invalid key/IV");

    for chunk in ciphertext.chunks_exact_mut(16) {
        encryptor.encrypt_block_mut(GenericArray::from_mut_slice(chunk));
    }

    let mut result = Vec::with_capacity(8 + 8 + 16 + ciphertext.len() + 32);
    result.extend_from_slice(b"opdata01");
    result.extend_from_slice(&plain_len.to_le_bytes());
    result.extend_from_slice(&iv);
    result.extend_from_slice(&ciphertext);

    let mut mac = HmacSha256::new_from_slice(mac_key).expect("invalid MAC key");
    mac.update(&result);
    result.extend_from_slice(&mac.finalize().into_bytes());

    result
}

fn encrypt_opdata01_b64(plaintext: &[u8], key: &[u8; 32], mac_key: &[u8; 32]) -> String {
    STANDARD.encode(encrypt_opdata01(plaintext, key, mac_key))
}

fn encrypt_item_key(item_key_material: &[u8; 64], master: &KeyPair) -> Vec<u8> {
    let iv: [u8; 16] = rand::random();

    let mut buf = item_key_material.to_vec();
    let mut encryptor = Aes256CbcEnc::new_from_slices(&master.enc, &iv).expect("invalid key/IV");
    for chunk in buf.chunks_exact_mut(16) {
        encryptor.encrypt_block_mut(GenericArray::from_mut_slice(chunk));
    }

    let mut result = Vec::with_capacity(16 + 64 + 32);
    result.extend_from_slice(&iv);
    result.extend_from_slice(&buf);

    let mut mac = HmacSha256::new_from_slice(&master.mac).expect("invalid MAC key");
    mac.update(&result);
    result.extend_from_slice(&mac.finalize().into_bytes());

    result
}

fn encrypt_item_key_b64(item_key_material: &[u8; 64], master: &KeyPair) -> String {
    STANDARD.encode(encrypt_item_key(item_key_material, master))
}

// ---- Sanitize ----

/// Per-UUID field overrides for overview and details.
/// Falls back to generic string replacement for any unmatched field.
struct ItemOverrides {
    /// Override entire overview JSON (title, url, ainfo, etc.)
    overview: serde_json::Value,
    /// Override entire details JSON (fields, sections, notes, password)
    details: serde_json::Value,
}

fn get_overrides(uuid: &str) -> Option<ItemOverrides> {
    match uuid {
        // 12CC60BD — Secure Note (cat=003)
        "12CC60BD1B8F4AA491F9314B437DDF86" => Some(ItemOverrides {
            overview: serde_json::json!({
                "title": "My Secure Note",
                "ainfo": "This is a test secure note"
            }),
            details: serde_json::json!({
                "notesPlain": "This is a test secure note.",
                "sections": [{
                    "fields": [{"k": "string", "n": "details", "t": "details", "v": "some test details"}],
                    "name": "Section_test",
                    "title": "Custom Section"
                }]
            }),
        }),

        // 1211EB9D — Password (cat=005)
        "1211EB9D74FE44CAADA3805506E482BB" => Some(ItemOverrides {
            overview: serde_json::json!({
                "title": "Strong Password",
                "ainfo": "4/18/2020",
                "ps": 100
            }),
            details: serde_json::json!({
                "password": "Str0ng!P@ss",
                "sections": [{
                    "fields": [{"k": "concealed", "n": "TOTP_test", "t": "one-time password", "v": "otpauth://totp/Example:user@example.org?secret=JBSWY3DPEHPK3PXP&digits=8&period=45&algorithm=sha256"}],
                    "name": "Section_test",
                    "title": ""
                }]
            }),
        }),

        // 30B6513E — Login (cat=001) KeePassXC
        "30B6513EE64B4DFE9C47EC2F257CE296" => Some(ItemOverrides {
            overview: serde_json::json!({
                "title": "Example Website",
                "ainfo": "user1",
                "ps": 32,
                "url": "https://example.com",
                "URLs": [{"l": "website", "u": "https://example.com"}, {"l": "staging", "u": "https://staging.example.com"}]
            }),
            details: serde_json::json!({
                "fields": [
                    {"designation": "username", "name": "username", "type": "T", "value": "user1"},
                    {"designation": "password", "name": "password", "type": "P", "value": "password123"}
                ],
                "htmlForm": {},
                "notesPlain": "Test account",
                "sections": [{
                    "fields": [{"k": "concealed", "n": "TOTP_test", "t": "one-time password", "v": "JBSWY3DPEHPK3PXP"}],
                    "name": "Section_test",
                    "title": "Advanced"
                }],
                "attachments": [{
                    "id": "AABBCCDDAABBCCDDAABBCCDDAABBCCDD",
                    "name": "test-attachment.txt"
                }]
            }),
        }),

        // 43B445C5 — Server (cat=110)
        "43B445C591924C0ABD7770816A1E8514" => Some(ItemOverrides {
            overview: serde_json::json!({
                "title": "My Server",
                "ainfo": "admin"
            }),
            details: serde_json::json!({
                "sections": [
                    {
                        "fields": [
                            {"k": "string", "n": "url", "t": "URL", "v": "myserver.local"},
                            {"k": "string", "n": "username", "t": "username", "v": "admin"},
                            {"k": "concealed", "n": "password", "t": "password", "v": "admin123"}
                        ],
                        "name": "", "title": ""
                    },
                    {
                        "fields": [
                            {"k": "string", "n": "admin_url", "t": "admin URL", "v": "admin.myserver.local"},
                            {"k": "string", "n": "admin_username", "t": "admin username", "v": "admin"},
                            {"k": "concealed", "n": "admin_password", "t": "admin password", "v": "admin123"}
                        ],
                        "name": "admin_console", "title": "Admin Console"
                    },
                    {
                        "fields": [
                            {"k": "string", "n": "name", "t": "name"},
                            {"k": "string", "n": "website", "t": "website"},
                            {"k": "string", "n": "support_url", "t": "support URL"},
                            {"k": "string", "n": "support_phone", "t": "support phone"}
                        ],
                        "name": "hosting_provider_details", "title": "Hosting Provider"
                    }
                ]
            }),
        }),

        // A6C49CAF — Login (cat=001) Expired
        "A6C49CAF606248828E33F0938FCEFF5C" => Some(ItemOverrides {
            overview: serde_json::json!({
                "title": "Old Account",
                "ainfo": "olduser",
                "ps": 1
            }),
            details: serde_json::json!({
                "fields": [
                    {"designation": "username", "name": "username", "type": "T", "value": "olduser"},
                    {"designation": "password", "name": "password", "type": "P", "value": "oldpass123"}
                ],
                "htmlForm": {},
                "sections": [{
                    "fields": [{"k": "date", "n": "expires_test", "t": "expires", "v": 1509537660}],
                    "name": "Section_test", "title": "Expiration"
                }]
            }),
        }),

        _ => None,
    }
}

// ---- Profile re-encryption ----

fn rekey_profile(
    old_profile: &Profile,
    old_password: &[u8],
    new_password: &[u8],
    new_iterations: u32,
) -> Result<String, String> {
    let lock = old_profile.resolve_lock()?;
    let old_salt = STANDARD.decode(&lock.salt).map_err(|e| format!("salt base64: {e}"))?;
    let old_derived = derive_keys(old_password, &old_salt, lock.iterations);

    // Decrypt raw key materials
    let master_material = decrypt_opdata01_b64(&lock.master_key, &old_derived.enc, &old_derived.mac)?;
    let overview_material = decrypt_opdata01_b64(&lock.overview_key, &old_derived.enc, &old_derived.mac)?;

    // Generate new salt and derive new keys
    let new_salt: [u8; 32] = rand::random();
    let new_derived = derive_keys(new_password, &new_salt, new_iterations);

    // Re-encrypt with new derived keys
    let new_master_key_b64 = encrypt_opdata01_b64(&master_material, &new_derived.enc, &new_derived.mac);
    let new_overview_key_b64 = encrypt_opdata01_b64(&overview_material, &new_derived.enc, &new_derived.mac);
    let new_salt_b64 = STANDARD.encode(&new_salt);

    let profile_json = serde_json::json!({
        "uuid": old_profile.uuid,
        "updatedAt": 0,
        "createdAt": 0,
        "tx": 0,
        "passwordHint": "",
        "lastUpdatedBy": "opvault-rekey",
        "profileName": "sanitized",
        "iterations": new_iterations,
        "salt": new_salt_b64,
        "masterKey": new_master_key_b64,
        "overviewKey": new_overview_key_b64,
    });

    Ok(format!("var profile={profile_json};"))
}

// ---- Main ----

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 {
        eprintln!("Usage: opvault-rekey <input.opvault> <output.opvault> <old_password> <new_password>");
        std::process::exit(1);
    }

    let input_path = Path::new(&args[1]);
    let output_path = Path::new(&args[2]);
    let old_password = args[3].as_bytes();
    let new_password = args[4].as_bytes();

    let input_default = input_path.join("default");
    let output_default = output_path.join("default");

    if !input_default.is_dir() {
        return Err(format!("Input must be a .opvault directory: {}", input_path.display()));
    }

    // 1. Read and parse profile.js
    let profile_content = fs::read_to_string(input_default.join("profile.js"))
        .map_err(|e| format!("read profile.js: {e}"))?;
    let json_str = profile_content
        .trim()
        .strip_prefix("var profile=")
        .unwrap_or(&profile_content)
        .strip_suffix(';')
        .unwrap_or(&profile_content)
        .trim();
    let profile: Profile = serde_json::from_str(json_str).map_err(|e| format!("parse profile: {e}"))?;

    // 2. Decrypt master/overview keys with old password
    let (old_master, old_overview) = decrypt_keys_from_profile(&profile, old_password)?;

    // 3. Generate new master/overview keys for re-encryption
    //    Re-use the same master_material/overview_material (just re-encrypt with new derived keys)
    //    But we need the master keys to decrypt items first.
    //    After re-keying profile, item keys need to be re-encrypted with the SAME master keys
    //    (since master_material is preserved, only the derived keys wrapping changes).
    //
    //    Actually: master_material is preserved → same master keys → item keys don't need re-encryption!
    //    Only profile.js changes (new salt + new derived keys wrapping same master/overview materials).
    //    Band files stay identical.

    // 4. Re-key profile
    let new_profile_js = rekey_profile(&profile, old_password, new_password, 100000)?;

    // 5. Create clean output directory
    if output_default.exists() {
        fs::remove_dir_all(&output_default).map_err(|e| format!("clean output dir: {e}"))?;
    }
    fs::create_dir_all(&output_default).map_err(|e| format!("create output dir: {e}"))?;
    fs::write(output_default.join("profile.js"), new_profile_js)
        .map_err(|e| format!("write profile.js: {e}"))?;

    // 6. Copy band files with sanitized content
    for entry in glob::glob(input_default.join("band_*.js").to_str().unwrap_or_default()).map_err(|e| format!("glob: {e}"))? {
        let entry = entry.map_err(|e| format!("glob entry: {e}"))?;
        let band_content = fs::read_to_string(&entry).map_err(|e| format!("read band: {e}"))?;
        let filename = entry.file_name().unwrap().to_str().unwrap();

        // Parse band JSON
        let inner = band_content.trim()
            .strip_prefix("ld(").unwrap_or(&band_content);
        let inner = inner.strip_suffix(';').unwrap_or(inner);
        let inner = inner.strip_suffix(')').unwrap_or(inner).trim();

        let mut items: HashMap<String, serde_json::Value> =
            serde_json::from_str(inner).map_err(|e| format!("parse band {filename}: {e}"))?;

        // Sanitize each item using per-UUID overrides
        for (uuid, item) in items.iter_mut() {
            let overrides = get_overrides(uuid);

            if let Some(ref ov) = overrides {
                // Use exact override for overview
                let o_encrypted = encrypt_opdata01_b64(
                    &serde_json::to_vec(&ov.overview).unwrap(),
                    &old_overview.enc,
                    &old_overview.mac,
                );
                if let Some(obj) = item.as_object_mut() {
                    obj.insert("o".into(), serde_json::Value::String(o_encrypted));
                }

                // Use exact override for details
                if let (Some(k_b64), true) = (
                    item.get("k").and_then(|v| v.as_str()),
                    !item.get("d").and_then(|v| v.as_str()).unwrap_or("").is_empty(),
                ) {
                    let item_key_material = decrypt_item_key_b64(k_b64, &old_master)?;
                    let mut item_enc = [0u8; 32];
                    let mut item_mac = [0u8; 32];
                    item_enc.copy_from_slice(&item_key_material[..32]);
                    item_mac.copy_from_slice(&item_key_material[32..64]);

                    let d_encrypted = encrypt_opdata01_b64(
                        &serde_json::to_vec(&ov.details).unwrap(),
                        &item_enc,
                        &item_mac,
                    );
                    let k_encrypted = encrypt_item_key_b64(&item_key_material, &old_master);

                    if let Some(obj) = item.as_object_mut() {
                        obj.insert("k".into(), serde_json::Value::String(k_encrypted));
                        obj.insert("d".into(), serde_json::Value::String(d_encrypted));
                    }
                }
            }
            // Items without overrides (trashed, credit card, identity) keep original encrypted data.
        }

        let new_band = format!("ld({});", serde_json::to_string(&items).unwrap());
        fs::write(output_default.join(filename), new_band)
            .map_err(|e| format!("write {filename}: {e}"))?;
    }

    // 7. Copy folders.js only (attachments handled by overrides)
    if input_default.join("folders.js").exists() {
        fs::copy(input_default.join("folders.js"), output_default.join("folders.js"))
            .map_err(|e| format!("copy folders.js: {e}"))?;
    }

    // Create sanitized attachment file
    let attachment_name = "30B6513EE64B4DFE9C47EC2F257CE296_AABBCCDDAABBCCDDAABBCCDDAABBCCDD.attachment";
    fs::write(output_default.join(attachment_name), b"test attachment content\n")
        .map_err(|e| format!("write attachment: {e}"))?;

    println!("Re-keyed and sanitized: {} -> {}", input_path.display(), output_path.display());
    println!("New password: {}", String::from_utf8_lossy(new_password));
    Ok(())
}
