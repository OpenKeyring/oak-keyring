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
#[derive(Debug, Deserialize)]
pub struct Profile {
    pub uuid: String,
    pub lock: ProfileLock,
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
