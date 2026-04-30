use std::time::Duration;

use rand::Rng;

use crate::errors::mapping::sync::SyncError;

/// RetryPolicy defines the behavior for retrying failed sync operations
/// with exponential backoff and jitter.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (default: 5)
    pub max_retries: u32,
    /// Base delay before first retry (default: 5s)
    pub base_delay: Duration,
    /// Maximum delay cap (default: 300s)
    pub max_delay: Duration,
    /// Exponential multiplier for each attempt (default: 2.0)
    pub multiplier: f64,
    /// Jitter fraction applied to delay (default: 0.2 = ±20%)
    pub jitter_fraction: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 5,
            base_delay: Duration::from_secs(5),
            max_delay: Duration::from_secs(300),
            multiplier: 2.0,
            jitter_fraction: 0.2,
        }
    }
}

impl RetryPolicy {
    /// Returns true if the given error should be retried.
    ///
    /// - `NetworkUnreachable` always returns true (infinite retries, ignores attempt count)
    /// - Retryable errors (`NetworkTimeout`, `ConnectionRefused`, `LockAcquireFailed`,
    ///   `ProviderError`) return true only if `attempt < max_retries`
    /// - Non-retryable errors always return false
    pub fn should_retry(&self, error: &SyncError, attempt: u32) -> bool {
        match error {
            // Always retry NetworkUnreachable - it's a special case for indefinite retry
            SyncError::NetworkUnreachable { .. } => true,
            // Retryable transient errors
            SyncError::NetworkTimeout { .. } => attempt < self.max_retries,
            SyncError::ConnectionRefused { .. } => attempt < self.max_retries,
            SyncError::LockAcquireFailed { .. } => attempt < self.max_retries,
            SyncError::ProviderError { .. } => attempt < self.max_retries,
            // Non-retryable errors
            SyncError::AuthenticationFailed { .. } => false,
            SyncError::TokenExpired => false,
            SyncError::ChecksumMismatch { .. } => false,
            SyncError::AadInconsistent { .. } => false,
            SyncError::SerializationFailed { .. } => false,
            SyncError::DeserializationFailed { .. } => false,
            SyncError::InvalidStateTransition { .. } => false,
            SyncError::LockReleaseFailed { .. } => false,
            SyncError::ProviderNotSupported { .. } => false,
            SyncError::ConfigValidationFailed { .. } => false,
            SyncError::VaultIdentityMismatch { .. } => false,
            SyncError::MetadataVersionConflict { .. } => false,
            SyncError::RecordNotFound { .. } => false,
            SyncError::PermissionDenied { .. } => false,
            SyncError::QuotaExceeded { .. } => false,
            SyncError::Cancelled { .. } => false,
        }
    }

    /// Computes the delay for a given attempt number using exponential backoff with jitter.
    ///
    /// Formula: `delay = min(base_delay * multiplier^attempt, max_delay)`
    /// Then applies jitter: `[delay * (1 - jitter_fraction), delay * (1 + jitter_fraction)]`
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let multiplier_pow = self.multiplier.powi(attempt as i32);
        let base_secs = self.base_delay.as_secs_f64();
        let max_delay_secs = self.max_delay.as_secs_f64();

        let raw_delay_secs =
            if multiplier_pow.is_infinite() || multiplier_pow > f64::MAX / base_secs {
                f64::MAX
            } else {
                base_secs * multiplier_pow
            };

        if raw_delay_secs.is_infinite() || raw_delay_secs > max_delay_secs {
            return self.max_delay;
        }

        let jitter_range = raw_delay_secs * self.jitter_fraction;
        let jitter_ns = (jitter_range * 1e9) as i128;
        let base_ns = (raw_delay_secs * 1e9) as i128;

        if jitter_ns > i128::MAX / 2 || base_ns > i128::MAX / 2 {
            return self.max_delay;
        }

        let mut rng = rand::rng();
        let jitter_offset_ns = rng.random_range(-jitter_ns..=jitter_ns);

        let final_ns = (base_ns + jitter_offset_ns).max(0) as u64;
        let final_duration = Duration::from_nanos(final_ns);

        if final_duration > self.max_delay {
            self.max_delay
        } else {
            final_duration
        }
    }

    /// Resets the retry attempt counter.
    /// Returns 0 (the caller manages the actual attempt counter).
    #[allow(clippy::unnecessary_wraps)]
    pub fn reset(&self) -> u32 {
        0
    }
}

/// BackoffTimer manages retry timing with stateful attempt tracking.
#[derive(Debug, Clone)]
pub struct BackoffTimer {
    policy: RetryPolicy,
    attempt: u32,
}

impl BackoffTimer {
    /// Creates a new BackoffTimer with the given policy, starting at attempt 0.
    pub fn new(policy: RetryPolicy) -> Self {
        Self { policy, attempt: 0 }
    }

    /// Returns the next delay duration and increments the attempt counter.
    pub fn next_delay(&mut self) -> Duration {
        let delay = self.policy.delay_for_attempt(self.attempt);
        self.attempt += 1;
        delay
    }

    /// Returns true if the given error should be retried based on current attempt.
    pub fn should_retry(&self, error: &SyncError) -> bool {
        self.policy.should_retry(error, self.attempt)
    }

    /// Resets the attempt counter to 0.
    pub fn reset(&mut self) {
        self.attempt = 0;
    }

    /// Returns the current attempt number.
    pub fn attempt(&self) -> u32 {
        self.attempt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::mapping::sync::SyncError;

    #[test]
    fn default_policy_values() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_retries, 5);
        assert_eq!(policy.base_delay, Duration::from_secs(5));
        assert_eq!(policy.max_delay, Duration::from_secs(300));
        assert_eq!(policy.multiplier, 2.0);
        assert_eq!(policy.jitter_fraction, 0.2);
    }

    #[test]
    fn should_retry_network_timeout() {
        let policy = RetryPolicy::default();
        let error = SyncError::NetworkTimeout {
            message: "timed out".to_string(),
        };

        // Should retry for attempts 0-4
        for attempt in 0..5 {
            assert!(
                policy.should_retry(&error, attempt),
                "should_retry should be true for attempt {}",
                attempt
            );
        }

        // Should not retry at attempt 5
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

        // Should always retry, even at high attempt counts
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
    fn delay_sequence_within_range() {
        let policy = RetryPolicy::default();

        let expected_delays = [(0, 5.0), (1, 10.0), (2, 20.0), (3, 40.0), (4, 80.0)];

        for (attempt, expected_secs) in expected_delays {
            let delay = policy.delay_for_attempt(attempt);
            let expected = Duration::from_secs_f64(expected_secs);
            let min = expected.mul_f64(0.8);
            let max = expected.mul_f64(1.2);

            assert!(
                delay >= min && delay <= max,
                "delay {:?} for attempt {} not in range [{:?}, {:?}]",
                delay,
                attempt,
                min,
                max
            );
        }
    }

    #[test]
    fn delay_capped_at_max() {
        let policy = RetryPolicy::default();

        // High attempt should be capped at max_delay
        let delay = policy.delay_for_attempt(100);
        assert_eq!(
            delay, policy.max_delay,
            "delay should be capped at max_delay"
        );
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

        // At attempt 0, should retry network timeout
        assert!(timer.should_retry(&network_timeout));
        assert!(!timer.should_retry(&auth_failed));

        // Increment to attempt 5
        for _ in 0..5 {
            timer.next_delay();
        }

        // At attempt 5, network timeout should not be retried
        assert!(!timer.should_retry(&network_timeout));
        assert!(!timer.should_retry(&auth_failed));
    }

    #[test]
    fn backoff_timer_produces_valid_delays() {
        let policy = RetryPolicy::default();
        let mut timer = BackoffTimer::new(policy);

        for attempt in 0..5 {
            let delay = timer.next_delay();
            let expected = Duration::from_secs(5 * 2u64.pow(attempt));
            let max_expected = expected.mul_f64(1.2);

            assert!(
                delay <= max_expected,
                "delay {:?} at attempt {} should be <= {:?}",
                delay,
                attempt,
                max_expected
            );
        }
    }
}
