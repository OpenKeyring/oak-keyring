//! OpVault parser: reads .opvault directory, decrypts entries, converts to ParsedItem.

use std::collections::HashMap;
use std::path::Path;

use crate::errors::mapping::import_export::ImportExportError;
use crate::services::import_export::parser::ParsedItem;
use crate::types::SecureStr;

use super::crypto;
use super::types::{BandItem, DecryptedDetails, DecryptedKeys, DecryptedOverview, Profile};

/// Parse an .opvault directory and return all non-trashed items.
pub fn parse_opvault(
    path: &Path,
    password: Option<&SecureStr>,
) -> Result<Vec<ParsedItem>, ImportExportError> {
    let password = password.ok_or(ImportExportError::PasswordRequired)?;

    let default_dir = path.join("default");
    if !default_dir.is_dir() {
        return Err(ImportExportError::InvalidFormat(
            "expected .opvault/default/ directory".into(),
        ));
    }

    // 1. Read and parse profile.js.
    let profile = read_profile(&default_dir)?;

    // 2. Derive keys from password.
    let keys = crypto::decrypt_keys_from_profile(&profile, password.get().as_bytes())?;

    // 3. Read and decrypt all band files.
    let mut items = Vec::new();
    for entry in
        glob::glob(default_dir.join("band_*.js").to_str().unwrap_or_default()).map_err(|e| {
            ImportExportError::ParseError {
                format: "opvault".into(),
                reason: format!("glob band files: {e}"),
            }
        })?
    {
        let entry = entry.map_err(|e| ImportExportError::FileReadError {
            path: default_dir.to_path_buf(),
            reason: e.to_string(),
        })?;
        let band_items = parse_band_file(&entry, &keys)?;
        items.extend(band_items);
    }

    Ok(items)
}

/// Read and parse profile.js.
fn read_profile(default_dir: &Path) -> Result<Profile, ImportExportError> {
    let profile_path = default_dir.join("profile.js");
    let content =
        std::fs::read_to_string(&profile_path).map_err(|e| ImportExportError::FileReadError {
            path: profile_path.clone(),
            reason: e.to_string(),
        })?;

    // Strip "var profile=" prefix and trailing ";".
    let json_str = strip_js_wrapper(&content, "var profile=");
    serde_json::from_str(json_str).map_err(|e| ImportExportError::ParseError {
        format: "opvault profile".into(),
        reason: e.to_string(),
    })
}

/// Parse a single band file and return ParsedItems.
fn parse_band_file(
    path: &Path,
    keys: &DecryptedKeys,
) -> Result<Vec<ParsedItem>, ImportExportError> {
    let content = std::fs::read_to_string(path).map_err(|e| ImportExportError::FileReadError {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;

    // Band files use "ld({...})" wrapper.
    let json_str = strip_ld_wrapper(&content);
    let entries: HashMap<String, BandItem> =
        serde_json::from_str(json_str).map_err(|e| ImportExportError::ParseError {
            format: "opvault band".into(),
            reason: e.to_string(),
        })?;

    let mut items = Vec::new();
    for entry in entries.values() {
        if entry.trashed {
            continue;
        }
        if let Some(item) = parse_entry(entry, keys)? {
            items.push(item);
        }
    }
    Ok(items)
}

/// Parse a single band entry into a ParsedItem.
fn parse_entry(
    entry: &BandItem,
    keys: &DecryptedKeys,
) -> Result<Option<ParsedItem>, ImportExportError> {
    // Decrypt overview.
    let overview_json =
        crypto::decrypt_opdata01_b64(&entry.o, &keys.overview.enc, &keys.overview.mac)?;
    let overview: DecryptedOverview =
        serde_json::from_slice(&overview_json).map_err(|e| ImportExportError::ParseError {
            format: "opvault overview".into(),
            reason: e.to_string(),
        })?;

    // Decrypt item key.
    let item_key = crypto::decrypt_item_key_b64(&entry.k, &keys.master)?;

    // Decrypt details.
    let detail_json = crypto::decrypt_opdata01_b64(&entry.d, &item_key.enc, &item_key.mac)?;
    let details: DecryptedDetails =
        serde_json::from_slice(&detail_json).map_err(|e| ImportExportError::ParseError {
            format: "opvault detail".into(),
            reason: e.to_string(),
        })?;

    let mut fields = HashMap::new();
    fields.insert("name".into(), overview.title.clone());

    // Extract URL from overview.
    let url = overview
        .urls
        .first()
        .map(|u| u.u.as_str())
        .or(overview.url.as_deref())
        .unwrap_or("");
    if !url.is_empty() {
        fields.insert("url".into(), url.to_string());
    }

    // Extract fields from details by designation.
    let mut username = String::new();
    let mut password = String::new();
    for field in &details.fields {
        match field.designation.as_deref() {
            Some("username") => username = field.value.clone().unwrap_or_default(),
            Some("password") => password = field.value.clone().unwrap_or_default(),
            _ => {}
        }
    }

    // Fallback: cat=005 stores password at details top level.
    if password.is_empty() {
        if let Some(ref pw) = details.password {
            password = pw.clone();
        }
    }

    if !username.is_empty() {
        fields.insert("username".into(), username);
    }
    if !password.is_empty() {
        fields.insert("password".into(), password);
    }

    // Build notes: notesPlain + sections (custom fields, TOTP).
    let mut notes_parts = Vec::new();
    if let Some(ref notes) = details.notes_plain {
        if !notes.is_empty() {
            notes_parts.push(notes.clone());
        }
    }

    // Extract standard fields (username/password/url) from sections if not already set.
    let mut section_username = String::new();
    let mut section_password = String::new();
    let mut section_url = String::new();

    for section in &details.sections {
        for sf in &section.fields {
            if let (Some(n), Some(v)) = (&sf.n, &sf.v) {
                let val_str = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };

                if val_str.is_empty() {
                    continue;
                }

                // Extract standard fields from sections (only if not already set).
                match n.as_str() {
                    "username" if !fields.contains_key("username") => {
                        section_username = val_str;
                        continue;
                    }
                    "password" if !fields.contains_key("password") => {
                        section_password = val_str;
                        continue;
                    }
                    "url" if !fields.contains_key("url") => {
                        section_url = val_str;
                        continue;
                    }
                    _ => {}
                }

                // TOTP or custom fields → notes.
                if n.starts_with("TOTP_") {
                    notes_parts.push(format!("TOTP: {val_str}"));
                } else if let Some(title) = &sf.t {
                    notes_parts.push(format!("{title}: {val_str}"));
                }
            }
        }
    }

    if !section_username.is_empty() {
        fields.insert("username".into(), section_username);
    }
    if !section_password.is_empty() {
        fields.insert("password".into(), section_password);
    }
    if !section_url.is_empty() {
        fields.insert("url".into(), section_url);
    }

    let notes = notes_parts.join("\n");
    if !notes.is_empty() {
        fields.insert("notes".into(), notes);
    }

    let item = ParsedItem {
        source_id: entry.uuid.clone(),
        fields,
        tags: overview.tags,
    };

    match entry.category.as_str() {
        "001" | "003" | "005" | "110" => Ok(Some(item)),
        _ => Ok(None), // Skip unsupported categories.
    }
}

/// Strip a JS variable assignment prefix and trailing semicolon.
fn strip_js_wrapper<'a>(content: &'a str, prefix: &str) -> &'a str {
    let trimmed = content.trim();
    let after_prefix = trimmed.strip_prefix(prefix).unwrap_or(trimmed);
    after_prefix
        .strip_suffix(';')
        .unwrap_or(after_prefix)
        .trim()
}

/// Strip the `ld({...})` or `ld({...});` wrapper from band files.
fn strip_ld_wrapper(content: &str) -> &str {
    let trimmed = content.trim();
    let after = trimmed.strip_prefix("ld(").unwrap_or(trimmed);
    // Strip optional trailing semicolon before the closing paren.
    let after = after.strip_suffix(';').unwrap_or(after);
    after.strip_suffix(')').unwrap_or(after).trim()
}
