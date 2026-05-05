//! OpVault JSON data structures for serde deserialization.

use serde::Deserialize;

/// Key pair: encryption key + HMAC key.
#[derive(Debug, Clone)]
pub struct KeyPair {
    pub enc: [u8; 32],
    pub mac: [u8; 32],
}

/// All decrypted keys needed for OpVault parsing.
#[derive(Debug)]
pub struct DecryptedKeys {
    pub master: KeyPair,
    pub overview: KeyPair,
}

/// profile.js lock section.
#[derive(Debug, Deserialize)]
pub struct ProfileLock {
    pub iterations: u32,
    pub salt: String,
    #[serde(rename = "masterKey")]
    pub master_key: String,
    #[serde(rename = "overviewKey")]
    pub overview_key: String,
}

/// profile.js top-level structure.
///
/// Supports two formats:
/// - 1Password native: `{"uuid": "...", "lock": {"iterations": ..., ...}}`
/// - KeePassXC export: `{"uuid": "...", "iterations": ..., "salt": "...", ...}` (flat)
#[derive(Debug, Deserialize)]
pub struct Profile {
    pub uuid: String,
    /// Nested lock object (1Password native format).
    #[serde(default)]
    pub lock: Option<ProfileLock>,
    /// Flat fields (KeePassXC export format).
    #[serde(default)]
    pub iterations: Option<u32>,
    #[serde(default)]
    pub salt: Option<String>,
    #[serde(default, rename = "masterKey")]
    pub master_key: Option<String>,
    #[serde(default, rename = "overviewKey")]
    pub overview_key: Option<String>,
}

impl Profile {
    /// Resolve lock fields from either nested `lock` or flat top-level fields.
    pub fn resolve_lock(&self) -> Result<ProfileLock, String> {
        if let Some(ref lock) = self.lock {
            return Ok(ProfileLock {
                iterations: lock.iterations,
                salt: lock.salt.clone(),
                master_key: lock.master_key.clone(),
                overview_key: lock.overview_key.clone(),
            });
        }
        match (
            self.iterations,
            &self.salt,
            &self.master_key,
            &self.overview_key,
        ) {
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

/// Band entry from band_*.js files.
#[derive(Debug, Deserialize)]
pub struct BandItem {
    pub uuid: String,
    pub category: String,
    #[serde(default)]
    pub trashed: bool,
    /// Encrypted overview (opdata01 base64).
    pub o: String,
    /// Encrypted item key (separate format, base64).
    pub k: String,
    /// Encrypted details (opdata01 base64).
    pub d: String,
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub findex: String,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub updated: i64,
}

/// Decrypted overview JSON.
#[derive(Debug, Deserialize)]
pub struct DecryptedOverview {
    pub title: String,
    #[serde(default, rename = "URLs")]
    pub urls: Vec<UrlEntry>,
    pub url: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub ainfo: Option<String>,
}

/// URL entry in overview.
#[derive(Debug, Deserialize)]
pub struct UrlEntry {
    pub u: String,
}

/// Decrypted details JSON.
#[derive(Debug, Deserialize)]
pub struct DecryptedDetails {
    #[serde(default)]
    pub fields: Vec<ItemField>,
    #[serde(default)]
    pub sections: Vec<Section>,
    #[serde(default, rename = "notesPlain")]
    pub notes_plain: Option<String>,
    /// Top-level password (cat=005 Password items store it here, not in fields).
    #[serde(default)]
    pub password: Option<String>,
}

/// Item field from details.
#[derive(Debug, Deserialize)]
pub struct ItemField {
    pub designation: Option<String>,
    pub name: Option<String>,
    pub value: Option<String>,
    #[serde(rename = "type")]
    pub field_type: Option<String>,
}

/// Section containing custom fields.
#[derive(Debug, Deserialize)]
pub struct Section {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub fields: Vec<SectionField>,
}

/// Custom field within a section.
#[derive(Debug, Deserialize)]
pub struct SectionField {
    pub k: Option<String>,
    pub n: Option<String>,
    pub t: Option<String>,
    pub v: Option<serde_json::Value>,
}
