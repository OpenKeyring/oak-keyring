use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::commands::types::HealthReport;
use crate::config::security::{HealthCheckFrequency, SecurityConfig};
use crate::crypto::strength::{evaluate_strength, StrengthLevel};
use crate::errors::mapping::health::HealthError;
use crate::types::credential::CredentialType;
use crate::types::record::StoredRecord;
use crate::types::sensitive::SecureStr;

/// Result of decrypting a single record's password for health checking.
/// Holds the record ID and decrypted password briefly for analysis.
struct PasswordEntry {
    id: Uuid,
    password: SecureStr,
}

/// Detect weak passwords among decrypted password entries.
/// Returns UUIDs of records whose password strength is VeryWeak or Weak.
fn detect_weak_passwords(entries: &[PasswordEntry]) -> Vec<Uuid> {
    entries
        .iter()
        .filter(|entry| {
            let strength = evaluate_strength(entry.password.get());
            matches!(
                strength.level,
                StrengthLevel::VeryWeak | StrengthLevel::Weak
            )
        })
        .map(|entry| entry.id)
        .collect()
}

/// Detect duplicate passwords using SHA-256 hash grouping.
/// Returns groups of record IDs that share the same password.
/// Only groups with 2+ members are returned.
fn detect_duplicate_passwords(entries: &[PasswordEntry]) -> Vec<Vec<Uuid>> {
    let mut hash_groups: HashMap<[u8; 32], Vec<Uuid>> = HashMap::new();

    for entry in entries {
        let mut hasher = Sha256::new();
        hasher.update(entry.password.get().as_bytes());
        let hash: [u8; 32] = hasher.finalize().into();
        hash_groups.entry(hash).or_default().push(entry.id);
    }

    hash_groups
        .into_values()
        .filter(|group| group.len() >= 2)
        .collect()
}

/// Check a single password against HIBP k-Anonymity API.
/// Returns Ok(true) if compromised, Ok(false) if safe.
/// Returns Err on network failure (caller should handle gracefully).
fn check_hibp_single_password(password: &str, agent: &ureq::Agent) -> Result<bool, HealthError> {
    // 1. SHA-1 hash of password, uppercase hex
    let mut hasher = sha1::Sha1::new();
    hasher.update(password.as_bytes());
    let hash_bytes = hasher.finalize();
    let hash_hex = hex::encode_upper(hash_bytes);

    // 2. Split into prefix (5 chars) and suffix (remaining 35 chars)
    let prefix = &hash_hex[..5];
    let suffix = &hash_hex[5..];

    // 3. Query HIBP API using the shared agent (connection reuse)
    let url = format!("https://api.pwnedpasswords.com/range/{prefix}");
    let response = agent
        .get(&url)
        .call()
        .map_err(|e| HealthError::HibpApiError(e.to_string()))?;

    let body = response
        .into_body()
        .read_to_vec()
        .map_err(|e| HealthError::HibpApiError(e.to_string()))?;

    let body_str = String::from_utf8(body).map_err(|e| HealthError::HibpApiError(e.to_string()))?;

    // 4. Check if our suffix appears in the response
    for line in body_str.lines() {
        let suffix_part = line.split(':').next().unwrap_or("");
        if suffix_part.eq_ignore_ascii_case(suffix) {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Batch HIBP check with 100ms delay between requests.
/// Returns UUIDs of records with compromised passwords.
/// Network failures are silently skipped (degraded mode).
///
/// **Note:** This function uses `thread::sleep` for rate limiting and blocks
/// the calling thread for `n * 100ms`. Plan K (Executor) should offload
/// `run_full_check` to a background thread to avoid freezing the TUI.
fn check_hibp_batch(entries: &[PasswordEntry], agent: &ureq::Agent) -> Vec<Uuid> {
    let mut compromised = Vec::new();

    for (i, entry) in entries.iter().enumerate() {
        // Rate limit: 100ms between requests (skip delay for first request)
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        match check_hibp_single_password(entry.password.get(), agent) {
            Ok(true) => compromised.push(entry.id),
            Ok(false) => {}
            Err(_) => {
                // Silent degradation: skip this record, continue with others
                tracing::debug!(
                    record_id = %entry.id,
                    "HIBP check failed, skipping record"
                );
            }
        }
    }

    compromised
}

/// Detect expired records based on `expires_at` field.
/// No decryption needed — works directly on StoredRecord.
fn detect_expired_records(records: &[StoredRecord]) -> Vec<Uuid> {
    let now = Utc::now();
    records
        .iter()
        .filter(|r| !r.deleted && r.expires_at.is_some_and(|exp| exp < now))
        .map(|r| r.id)
        .collect()
}

/// Determine whether a health check should run based on config and last check time.
/// Follows S3 spec execution timing logic.
pub fn should_run(config: &SecurityConfig, last_check: Option<DateTime<Utc>>) -> bool {
    if !config.health_check_enabled {
        return false;
    }

    let now = Utc::now();
    match last_check {
        None => true, // Never checked before
        Some(last) => match config.health_check_frequency {
            HealthCheckFrequency::OnStartup => true,
            HealthCheckFrequency::Daily => {
                now.signed_duration_since(last).num_seconds() > 24 * 3600
            }
            HealthCheckFrequency::Weekly => {
                now.signed_duration_since(last).num_seconds() > 7 * 24 * 3600
            }
        },
    }
}

/// Health check service for password security analysis.
///
/// Provides full health checks (weak + duplicate + HIBP + expired),
/// single-record HIBP checks, and execution timing control.
pub struct HealthService {
    /// HTTP agent for HIBP API calls. None = offline mode.
    http_agent: Option<ureq::Agent>,
}

impl Default for HealthService {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthService {
    /// Create a new HealthService.
    /// Attempts to initialize an HTTP agent for HIBP checks.
    /// If agent creation fails, operates in offline mode (local checks only).
    pub fn new() -> Self {
        let http_agent = Some(ureq::Agent::new_with_defaults());
        Self { http_agent }
    }

    /// Create an offline-only HealthService (no HIBP checks).
    #[cfg(test)]
    pub fn new_offline() -> Self {
        Self { http_agent: None }
    }

    /// Run a full health check on all non-deleted Login records.
    ///
    /// This method requires access to VaultService for record decryption.
    /// It takes a list of stored records and a decrypt function to avoid
    /// direct coupling with VaultService internals.
    ///
    /// # Arguments
    /// * `records` - All stored records (will be filtered to non-deleted Login)
    /// * `decrypt_fn` - Function to decrypt a record's password field
    ///
    /// # Returns
    /// HealthReport with all detected issues.
    pub fn run_full_check<F>(&self, records: &[StoredRecord], decrypt_fn: F) -> HealthReport
    where
        F: Fn(Uuid) -> Result<SecureStr, String>,
    {
        // 1. Filter to non-deleted Login records
        let login_records: Vec<&StoredRecord> = records
            .iter()
            .filter(|r| !r.deleted && r.credential_type == CredentialType::Login)
            .collect();

        let total_checked = login_records.len();

        // 2. Decrypt passwords (collect for analysis)
        let mut entries: Vec<PasswordEntry> = Vec::new();
        for record in &login_records {
            match decrypt_fn(record.id) {
                Ok(password) => entries.push(PasswordEntry {
                    id: record.id,
                    password,
                }),
                Err(_) => {
                    // Skip records that fail decryption
                    tracing::debug!(record_id = %record.id, "skipping record: decryption failed");
                }
            }
        }

        // 3. Run all detections
        let weak_passwords = detect_weak_passwords(&entries);
        let duplicate_passwords = detect_duplicate_passwords(&entries);

        // 4. HIBP check (only if HTTP agent is available)
        let compromised = match self.http_agent.as_ref() {
            Some(agent) => check_hibp_batch(&entries, agent),
            None => {
                tracing::debug!("HIBP check skipped: offline mode");
                Vec::new()
            }
        };

        // 5. Expiry detection (uses all non-deleted records, not just Login)
        let expired = detect_expired_records(records);

        // entries dropped here — all SecureStr passwords zeroized

        HealthReport {
            weak_passwords,
            duplicate_passwords,
            compromised,
            expired,
            total_checked,
        }
    }

    /// Check a single password against HIBP.
    /// Returns Ok(true) if the password has been compromised.
    /// Returns Err if HIBP API is unavailable (offline mode or network failure).
    pub fn check_hibp_single(&self, password: &SecureStr) -> Result<bool, HealthError> {
        let agent = self
            .http_agent
            .as_ref()
            .ok_or_else(|| HealthError::HibpApiError("offline mode".into()))?;
        check_hibp_single_password(password.get(), agent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::sensitive::SecureStr;

    fn make_entry(password: &str) -> PasswordEntry {
        PasswordEntry {
            id: Uuid::new_v4(),
            password: SecureStr::new(password.to_string()),
        }
    }

    fn make_stored_record(expires_at: Option<DateTime<Utc>>) -> StoredRecord {
        StoredRecord {
            id: Uuid::new_v4(),
            credential_type: CredentialType::Login,
            encrypted_data: vec![],
            nonce: [0u8; 24],
            dek_version: 1,
            aad: vec![],
            is_favorite: false,
            expires_at,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            updated_by: "test".to_string(),
            version: 1,
            deleted: false,
            deleted_at: None,
            tags: vec![],
        }
    }

    fn make_security_config(enabled: bool, frequency: HealthCheckFrequency) -> SecurityConfig {
        SecurityConfig {
            health_check_enabled: enabled,
            health_check_frequency: frequency,
            audit_enabled: true,
            audit_retention_days: 365,
            rotation: crate::types::rotation::RotationConfig::default(),
        }
    }

    // --- Weak password tests ---

    #[test]
    fn detect_weak_finds_very_weak_passwords() {
        let entries = vec![make_entry("123"), make_entry("abc")];
        let weak = detect_weak_passwords(&entries);
        assert_eq!(weak.len(), 2);
    }

    #[test]
    fn detect_weak_skips_strong_passwords() {
        let entries = vec![make_entry("StrongP@ssw0rd2024!Long")];
        let weak = detect_weak_passwords(&entries);
        assert!(weak.is_empty());
    }

    #[test]
    fn detect_weak_mixed_strength() {
        let entries = vec![
            make_entry("123"),         // VeryWeak
            make_entry("abcd1234"),    // Weak
            make_entry("Str0ng!Pass"), // Fair or above
        ];
        let weak = detect_weak_passwords(&entries);
        assert_eq!(weak.len(), 2);
    }

    #[test]
    fn detect_weak_empty_input() {
        let weak = detect_weak_passwords(&[]);
        assert!(weak.is_empty());
    }

    // --- Duplicate password tests ---

    #[test]
    fn detect_duplicate_finds_identical_passwords() {
        let pw = "same_password_123";
        let entries = vec![make_entry(pw), make_entry(pw), make_entry("different")];
        let dups = detect_duplicate_passwords(&entries);
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].len(), 2);
    }

    #[test]
    fn detect_duplicate_no_duplicates() {
        let entries = vec![make_entry("alpha"), make_entry("beta"), make_entry("gamma")];
        let dups = detect_duplicate_passwords(&entries);
        assert!(dups.is_empty());
    }

    #[test]
    fn detect_duplicate_multiple_groups() {
        let entries = vec![
            make_entry("same1"),
            make_entry("same1"),
            make_entry("same2"),
            make_entry("same2"),
            make_entry("unique"),
        ];
        let dups = detect_duplicate_passwords(&entries);
        assert_eq!(dups.len(), 2);
    }

    #[test]
    fn detect_duplicate_single_entry_no_group() {
        let entries = vec![make_entry("only_one")];
        let dups = detect_duplicate_passwords(&entries);
        assert!(dups.is_empty());
    }

    // --- HIBP hash computation tests ---

    #[test]
    fn hibp_sha1_hash_format_is_correct() {
        let mut hasher = sha1::Sha1::new();
        hasher.update(b"password");
        let hash_bytes = hasher.finalize();
        let hash_hex = hex::encode_upper(hash_bytes);
        assert_eq!(hash_hex, "5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8");
        assert_eq!(&hash_hex[..5], "5BAA6");
    }

    #[test]
    fn hibp_prefix_suffix_split_is_correct() {
        let hash_hex = "5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8";
        let prefix = &hash_hex[..5];
        let suffix = &hash_hex[5..];
        assert_eq!(prefix, "5BAA6");
        assert_eq!(suffix.len(), 35);
    }

    // --- Expiry detection tests ---

    #[test]
    fn detect_expired_finds_past_expiry() {
        let past = Utc::now() - chrono::Duration::days(1);
        let records = vec![make_stored_record(Some(past))];
        let expired = detect_expired_records(&records);
        assert_eq!(expired.len(), 1);
    }

    #[test]
    fn detect_expired_skips_future_expiry() {
        let future = Utc::now() + chrono::Duration::days(1);
        let records = vec![make_stored_record(Some(future))];
        let expired = detect_expired_records(&records);
        assert!(expired.is_empty());
    }

    #[test]
    fn detect_expired_skips_no_expiry() {
        let records = vec![make_stored_record(None)];
        let expired = detect_expired_records(&records);
        assert!(expired.is_empty());
    }

    #[test]
    fn detect_expired_skips_deleted_records() {
        let past = Utc::now() - chrono::Duration::days(1);
        let mut record = make_stored_record(Some(past));
        record.deleted = true;
        let records = vec![record];
        let expired = detect_expired_records(&records);
        assert!(expired.is_empty());
    }

    // --- should_run tests ---

    #[test]
    fn should_run_returns_false_when_disabled() {
        let config = make_security_config(false, HealthCheckFrequency::OnStartup);
        assert!(!should_run(&config, None));
    }

    #[test]
    fn should_run_returns_true_when_never_checked() {
        let config = make_security_config(true, HealthCheckFrequency::Weekly);
        assert!(should_run(&config, None));
    }

    #[test]
    fn should_run_on_startup_always_returns_true() {
        let config = make_security_config(true, HealthCheckFrequency::OnStartup);
        let recent = Utc::now() - chrono::Duration::seconds(10);
        assert!(should_run(&config, Some(recent)));
    }

    #[test]
    fn should_run_daily_returns_false_within_24h() {
        let config = make_security_config(true, HealthCheckFrequency::Daily);
        let recent = Utc::now() - chrono::Duration::hours(23);
        assert!(!should_run(&config, Some(recent)));
    }

    #[test]
    fn should_run_daily_returns_true_after_24h() {
        let config = make_security_config(true, HealthCheckFrequency::Daily);
        let old = Utc::now() - chrono::Duration::hours(25);
        assert!(should_run(&config, Some(old)));
    }

    #[test]
    fn should_run_weekly_returns_false_within_7d() {
        let config = make_security_config(true, HealthCheckFrequency::Weekly);
        let recent = Utc::now() - chrono::Duration::days(6);
        assert!(!should_run(&config, Some(recent)));
    }

    #[test]
    fn should_run_weekly_returns_true_after_7d() {
        let config = make_security_config(true, HealthCheckFrequency::Weekly);
        let old = Utc::now() - chrono::Duration::days(8);
        assert!(should_run(&config, Some(old)));
    }

    // --- HealthService integration tests ---

    #[test]
    fn full_check_with_mock_decrypt() {
        let service = HealthService::new_offline();

        let rec1 = make_stored_record(None);
        let rec2 = make_stored_record(None);

        let id1 = rec1.id;
        let id2 = rec2.id;

        let records = vec![rec1, rec2];

        let decrypt_fn = |id: Uuid| -> Result<SecureStr, String> {
            if id == id1 {
                Ok(SecureStr::new("123".to_string())) // VeryWeak
            } else if id == id2 {
                Ok(SecureStr::new("Str0ng!P@ssw0rd2024!".to_string())) // Strong
            } else {
                Err("not found".into())
            }
        };

        let report = service.run_full_check(&records, decrypt_fn);

        assert_eq!(report.total_checked, 2);
        assert_eq!(report.weak_passwords.len(), 1);
        assert!(report.weak_passwords.contains(&id1));
        assert!(!report.weak_passwords.contains(&id2));
        assert!(report.compromised.is_empty()); // offline mode
    }

    #[test]
    fn full_check_empty_vault() {
        let service = HealthService::new_offline();
        let report = service.run_full_check(&[], |_id| Err("none".into()));
        assert_eq!(report.issue_count(), 0);
        assert_eq!(report.total_checked, 0);
    }

    #[test]
    fn full_check_skips_deleted_records() {
        let mut deleted = make_stored_record(None);
        deleted.deleted = true;

        let service = HealthService::new_offline();
        let report = service.run_full_check(&[deleted], |_id| Ok(SecureStr::new("weak".into())));
        assert_eq!(report.total_checked, 0);
    }

    #[test]
    fn full_check_handles_decryption_failure_gracefully() {
        let rec = make_stored_record(None);
        let service = HealthService::new_offline();

        let report = service.run_full_check(&[rec], |_id| Err("decrypt error".into()));
        assert_eq!(report.total_checked, 1);
        assert_eq!(report.weak_passwords.len(), 0); // skipped due to decryption failure
    }
}
