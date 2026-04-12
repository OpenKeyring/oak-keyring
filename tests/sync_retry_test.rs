use oak_keyring::errors::mapping::sync::SyncError;
use oak_keyring::sync::retry::{BackoffTimer, RetryPolicy};

#[test]
fn default_policy_values() {
    let policy = RetryPolicy::default();
    assert_eq!(policy.max_retries, 5);
    assert_eq!(policy.base_delay, std::time::Duration::from_secs(5));
    assert_eq!(policy.max_delay, std::time::Duration::from_secs(300));
    assert_eq!(policy.multiplier, 2.0);
    assert_eq!(policy.jitter_fraction, 0.2);
}

#[test]
fn delay_sequence_within_range() {
    let policy = RetryPolicy::default();

    let expected_delays = [(0, 5.0), (1, 10.0), (2, 20.0), (3, 40.0), (4, 80.0)];

    for (attempt, expected_secs) in expected_delays {
        let delay = policy.delay_for_attempt(attempt);
        let expected = std::time::Duration::from_secs_f64(expected_secs);
        let min = expected.mul_f64(0.8);
        let max = expected.mul_f64(1.2);
        assert!(
            delay >= min && delay <= max,
            "delay {:?} not in range [{:?}, {:?}]",
            delay,
            min,
            max
        );
    }
}

#[test]
fn delay_capped_at_max() {
    let policy = RetryPolicy::default();
    let delay = policy.delay_for_attempt(100);
    assert_eq!(delay, policy.max_delay);
}

#[test]
fn should_retry_network_timeout() {
    let policy = RetryPolicy::default();
    let error = SyncError::NetworkTimeout {
        message: "timed out".to_string(),
    };

    for attempt in 0..5 {
        assert!(
            policy.should_retry(&error, attempt),
            "NetworkTimeout should retry at attempt {}",
            attempt
        );
    }
    assert!(!policy.should_retry(&error, 5));
}

#[test]
fn should_retry_connection_refused() {
    let policy = RetryPolicy::default();
    let error = SyncError::ConnectionRefused {
        endpoint: "localhost:8080".to_string(),
    };
    assert!(policy.should_retry(&error, 0));
    assert!(policy.should_retry(&error, 4));
    assert!(!policy.should_retry(&error, 5));
}

#[test]
fn should_retry_network_unreachable_infinite() {
    let policy = RetryPolicy::default();
    let error = SyncError::NetworkUnreachable {
        message: "unreachable".to_string(),
    };

    assert!(policy.should_retry(&error, 0));
    assert!(policy.should_retry(&error, 100));
    assert!(policy.should_retry(&error, 1000));
}

#[test]
fn should_not_retry_auth_failed() {
    let policy = RetryPolicy::default();
    let error = SyncError::AuthenticationFailed {
        reason: "invalid".to_string(),
    };
    assert!(!policy.should_retry(&error, 0));
    assert!(!policy.should_retry(&error, 100));
}

#[test]
fn should_not_retry_checksum_mismatch() {
    let policy = RetryPolicy::default();
    let error = SyncError::ChecksumMismatch {
        expected: "abc".to_string(),
        actual: "def".to_string(),
        record_id: "rec_1".to_string(),
    };
    assert!(!policy.should_retry(&error, 0));
    assert!(!policy.should_retry(&error, 100));
}

#[test]
fn should_not_retry_serialization_failed() {
    let policy = RetryPolicy::default();
    let error = SyncError::SerializationFailed {
        message: "json error".to_string(),
    };
    assert!(!policy.should_retry(&error, 0));
    assert!(!policy.should_retry(&error, 100));
}

#[test]
fn should_retry_lock_acquire() {
    let policy = RetryPolicy::default();
    let error = SyncError::LockAcquireFailed {
        reason: "timeout".to_string(),
    };
    assert!(policy.should_retry(&error, 0));
    assert!(policy.should_retry(&error, 4));
    assert!(!policy.should_retry(&error, 5));
}

#[test]
fn should_retry_provider_error() {
    let policy = RetryPolicy::default();
    let error = SyncError::ProviderError {
        provider: "s3".to_string(),
        message: "transient".to_string(),
    };
    assert!(policy.should_retry(&error, 0));
    assert!(policy.should_retry(&error, 4));
    assert!(!policy.should_retry(&error, 5));
}

#[test]
fn should_not_retry_quota_exceeded() {
    let policy = RetryPolicy::default();
    let error = SyncError::QuotaExceeded {
        provider: "dropbox".to_string(),
    };
    assert!(!policy.should_retry(&error, 0));
    assert!(!policy.should_retry(&error, 100));
}

#[test]
fn should_not_retry_permission_denied() {
    let policy = RetryPolicy::default();
    let error = SyncError::PermissionDenied {
        path: "/vault".to_string(),
    };
    assert!(!policy.should_retry(&error, 0));
    assert!(!policy.should_retry(&error, 100));
}

#[test]
fn backoff_timer_increments_attempt() {
    let policy = RetryPolicy::default();
    let mut timer = BackoffTimer::new(policy);

    assert_eq!(timer.attempt(), 0);
    timer.next_delay();
    assert_eq!(timer.attempt(), 1);
    timer.next_delay();
    assert_eq!(timer.attempt(), 2);
    timer.next_delay();
    assert_eq!(timer.attempt(), 3);
}

#[test]
fn backoff_timer_reset() {
    let policy = RetryPolicy::default();
    let mut timer = BackoffTimer::new(policy);

    timer.next_delay();
    timer.next_delay();
    assert_eq!(timer.attempt(), 2);

    timer.reset();
    assert_eq!(timer.attempt(), 0);
}

#[test]
fn backoff_timer_delegates_should_retry() {
    let policy = RetryPolicy::default();
    let mut timer = BackoffTimer::new(policy);

    let network_timeout = SyncError::NetworkTimeout {
        message: "timed out".to_string(),
    };
    let auth_failed = SyncError::AuthenticationFailed {
        reason: "invalid".to_string(),
    };

    assert!(timer.should_retry(&network_timeout));
    assert!(!timer.should_retry(&auth_failed));

    for _ in 0..5 {
        timer.next_delay();
    }

    assert!(!timer.should_retry(&network_timeout));
    assert!(!timer.should_retry(&auth_failed));
}
