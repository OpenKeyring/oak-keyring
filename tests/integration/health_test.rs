//! Integration tests for S3 Health Service.
//!
//! HIBP tests that require network are NOT marked #[ignore] but handle
//! network failure gracefully — they skip instead of failing in CI.

use chrono::Utc;
use oak_keyring::commands::types::{HealthIssue, HealthReport};
use oak_keyring::config::security::{HealthCheckFrequency, SecurityConfig};
use oak_keyring::crypto::strength::evaluate_strength;
use oak_keyring::services::health::{should_run, HealthService};
use oak_keyring::types::credential::CredentialType;
use oak_keyring::types::record::StoredRecord;
use oak_keyring::types::sensitive::SecureStr;
use uuid::Uuid;

fn make_rec(id: Uuid) -> StoredRecord {
    StoredRecord {
        id,
        credential_type: CredentialType::Login,
        encrypted_data: vec![],
        nonce: [0u8; 24],
        dek_version: 1,
        aad: vec![],
        is_favorite: false,
        expires_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        updated_by: "test".into(),
        version: 1,
        deleted: false,
        deleted_at: None,
        tags: vec![],
    }
}

#[test]
fn acceptance_weak_password_123_detected() {
    // AC#1: password "123" detected as VeryWeak
    let strength = evaluate_strength("123");
    assert!(
        matches!(
            strength.level,
            oak_keyring::crypto::strength::StrengthLevel::VeryWeak
        ),
        "expected VeryWeak, got {:?}",
        strength.level
    );
}

#[test]
fn acceptance_duplicate_password_grouping() {
    // AC#2: two records with same password → duplicate group
    let service = HealthService::new();
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let same_pw = "abc123".to_string();

    let records = vec![make_rec(id1), make_rec(id2)];
    let decrypt_fn = |_| Ok(SecureStr::new(same_pw.clone()));

    let report = service.run_full_check(&records, decrypt_fn);
    assert_eq!(report.duplicate_passwords.len(), 1);
    assert_eq!(report.duplicate_passwords[0].len(), 2);
}

#[test]
fn acceptance_hibp_check_password_is_compromised() {
    // AC#3: "password" is known to be compromised in HIBP
    // Gracefully handles network unavailability (CI)
    let service = HealthService::new();
    let result = service.check_hibp_single(&SecureStr::new("password".to_string()));
    match result {
        Ok(compromised) => assert!(compromised, "expected 'password' to be compromised"),
        Err(e) => {
            // Network unavailable in CI — skip, don't fail
            eprintln!("HIBP test skipped: {}", e);
        }
    }
}

#[test]
fn acceptance_get_issue_for_priority_ordering() {
    // AC#5: Weak beats Duplicate
    let id = Uuid::new_v4();
    let report = HealthReport {
        weak_passwords: vec![id],
        duplicate_passwords: vec![vec![id, Uuid::new_v4()]],
        ..HealthReport::empty()
    };
    assert_eq!(report.get_issue_for(id), Some(HealthIssue::Weak));

    // AC#6: Compromised beats everything
    let report = HealthReport {
        weak_passwords: vec![id],
        compromised: vec![id],
        ..HealthReport::empty()
    };
    assert_eq!(report.get_issue_for(id), Some(HealthIssue::Compromised));
}

#[test]
fn acceptance_should_run_disabled() {
    // AC#7: disabled returns false
    let config = SecurityConfig {
        health_check_enabled: false,
        ..Default::default()
    };
    assert!(!should_run(&config, None));
}

#[test]
fn acceptance_should_run_daily_within_24h() {
    // AC#8: Daily, checked 23h ago → false
    let config = SecurityConfig {
        health_check_enabled: true,
        health_check_frequency: HealthCheckFrequency::Daily,
        ..Default::default()
    };
    let recent = chrono::Utc::now() - chrono::Duration::hours(23);
    assert!(!should_run(&config, Some(recent)));
}

#[test]
fn acceptance_expired_yesterday_detected() {
    // AC#9: expires_at = yesterday → in expired list
    let past = chrono::Utc::now() - chrono::Duration::days(1);
    let mut rec = make_rec(Uuid::new_v4());
    rec.expires_at = Some(past);

    let service = HealthService::new();
    let report = service.run_full_check(&[rec], |_| Ok(SecureStr::new("strongpass".to_string())));
    assert!(!report.expired.is_empty());
}

#[test]
fn acceptance_expired_tomorrow_not_detected() {
    // AC#10: expires_at = tomorrow → NOT in expired list
    let future = chrono::Utc::now() + chrono::Duration::days(1);
    let mut rec = make_rec(Uuid::new_v4());
    rec.expires_at = Some(future);

    let service = HealthService::new();
    let report = service.run_full_check(&[rec], |_| Ok(SecureStr::new("strongpass".to_string())));
    assert!(report.expired.is_empty());
}
