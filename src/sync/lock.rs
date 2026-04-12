//! Cloud sync lock for mutual exclusion across devices.
//!
//! Implements a distributed lock using a cloud-stored .sync.lock file
//! with TTL-based expiry to prevent race conditions during sync operations.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::cloud::schema::LOCK_FILENAME;
use crate::cloud::CloudStorage;
use crate::errors::mapping::sync::SyncError;

/// Default TTL for lock file (5 minutes).
pub const LOCK_TTL_SECONDS: i64 = 300;

/// Default timeout for acquiring lock (30 seconds).
pub const LOCK_ACQUIRE_TIMEOUT_SECONDS: i64 = 30;

/// Default retry interval when lock is held by another device (5 seconds).
pub const LOCK_ACQUIRE_RETRY_INTERVAL_SECONDS: i64 = 5;

/// Lock file data stored in cloud.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockFileData {
    /// Device identifier that holds the lock.
    pub device_id: String,
    /// When the lock was acquired (ISO 8601 format).
    pub acquired_at: String,
    /// When the lock expires (ISO 8601 format).
    pub expires_at: String,
}

/// Distributed lock for cloud sync operations.
///
/// Manages mutual exclusion across multiple devices by storing a lock file
/// in cloud storage with TTL-based expiry.
#[derive(Debug, Clone)]
pub struct SyncLock {
    /// Cloud storage for lock file operations.
    storage: CloudStorage,
    /// Unique identifier for this device.
    device_id: String,
    /// Time-to-live for lock file.
    lock_ttl: Duration,
    /// Maximum time to wait for lock acquisition.
    acquire_timeout: Duration,
    /// Interval between retry attempts when lock is held.
    retry_interval: Duration,
}

impl SyncLock {
    /// Creates a new SyncLock with default timeouts.
    ///
    /// Default values:
    /// - lock_ttl: 300 seconds (5 minutes)
    /// - acquire_timeout: 30 seconds
    /// - retry_interval: 5 seconds
    pub fn new(storage: CloudStorage, device_id: String) -> Self {
        Self {
            storage,
            device_id,
            lock_ttl: Duration::seconds(LOCK_TTL_SECONDS),
            acquire_timeout: Duration::seconds(LOCK_ACQUIRE_TIMEOUT_SECONDS),
            retry_interval: Duration::seconds(LOCK_ACQUIRE_RETRY_INTERVAL_SECONDS),
        }
    }

    /// Builder pattern for custom timeout values.
    pub fn with_timeouts(
        mut self,
        lock_ttl: Duration,
        acquire_timeout: Duration,
        retry_interval: Duration,
    ) -> Self {
        self.lock_ttl = lock_ttl;
        self.acquire_timeout = acquire_timeout;
        self.retry_interval = retry_interval;
        self
    }

    /// Attempts to acquire the distributed lock.
    ///
    /// Reads existing lock file:
    /// - If no lock exists or lock is expired -> creates new lock
    /// - If lock is held by this device -> extends/renews the lock
    /// - If lock is held by another device and still valid -> waits and retries
    /// - If total wait exceeds acquire_timeout -> returns LockAcquireFailed
    pub async fn acquire(&self) -> Result<(), SyncError> {
        let start = Utc::now();

        loop {
            let elapsed = Utc::now().signed_duration_since(start);
            if elapsed.num_seconds() >= self.acquire_timeout.num_seconds() {
                return Err(SyncError::LockAcquireFailed {
                    reason: format!("timeout after {:?} waiting for lock", self.acquire_timeout),
                });
            }

            match self.try_acquire().await {
                Ok(()) => return Ok(()),
                Err(SyncError::LockAcquireFailed { .. }) => {
                    let remaining = self.acquire_timeout - elapsed;
                    let sleep_duration = if remaining < self.retry_interval {
                        remaining
                    } else {
                        self.retry_interval
                    };

                    if sleep_duration.num_milliseconds() > 0 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(
                            sleep_duration.num_milliseconds() as u64,
                        ))
                        .await;
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn try_acquire(&self) -> Result<(), SyncError> {
        let existing = self.storage.download_raw(LOCK_FILENAME).await?;

        match existing {
            None => {
                // No lock exists, create one
                let lock_data = self.create_lock_data();
                self.upload_lock(&lock_data).await?;
                Ok(())
            }
            Some(data) => {
                let lock_data: LockFileData = serde_json::from_slice(&data).map_err(|e| {
                    SyncError::DeserializationFailed {
                        message: format!("lock file: {}", e),
                    }
                })?;

                if self.is_expired(&lock_data) {
                    // Lock is expired, we can take it
                    let new_lock = self.create_lock_data();
                    self.upload_lock(&new_lock).await?;
                    Ok(())
                } else if lock_data.device_id == self.device_id {
                    // We already hold the lock, extend it
                    let renewed_lock = self.create_lock_data();
                    self.upload_lock(&renewed_lock).await?;
                    Ok(())
                } else {
                    // Another device holds a valid lock
                    Err(SyncError::LockAcquireFailed {
                        reason: format!(
                            "lock held by device '{}' until {}",
                            lock_data.device_id, lock_data.expires_at
                        ),
                    })
                }
            }
        }
    }

    /// Releases the lock if held by this device.
    ///
    /// Idempotent: if no lock exists or lock is held by another device,
    /// this returns Ok without modifying anything.
    pub async fn release(&self) -> Result<(), SyncError> {
        let existing = self.storage.download_raw(LOCK_FILENAME).await?;

        match existing {
            None => {
                // No lock exists, nothing to release (idempotent)
                Ok(())
            }
            Some(data) => {
                let lock_data: LockFileData = serde_json::from_slice(&data).map_err(|e| {
                    SyncError::DeserializationFailed {
                        message: format!("lock file: {}", e),
                    }
                })?;

                if lock_data.device_id == self.device_id {
                    // We hold the lock, delete it
                    self.storage.delete_raw(LOCK_FILENAME).await?;
                    Ok(())
                } else {
                    // Another device holds the lock, don't delete
                    Ok(())
                }
            }
        }
    }

    /// Checks if the lock is currently held (by any device) and not expired.
    pub async fn is_locked(&self) -> Result<bool, SyncError> {
        let existing = self.storage.download_raw(LOCK_FILENAME).await?;

        match existing {
            None => Ok(false),
            Some(data) => {
                let lock_data: LockFileData = serde_json::from_slice(&data).map_err(|e| {
                    SyncError::DeserializationFailed {
                        message: format!("lock file: {}", e),
                    }
                })?;
                Ok(!self.is_expired(&lock_data))
            }
        }
    }

    /// Checks if the lock is held by this device specifically.
    pub async fn is_locked_by_self(&self) -> Result<bool, SyncError> {
        let existing = self.storage.download_raw(LOCK_FILENAME).await?;

        match existing {
            None => Ok(false),
            Some(data) => {
                let lock_data: LockFileData = serde_json::from_slice(&data).map_err(|e| {
                    SyncError::DeserializationFailed {
                        message: format!("lock file: {}", e),
                    }
                })?;

                if self.is_expired(&lock_data) {
                    Ok(false)
                } else {
                    Ok(lock_data.device_id == self.device_id)
                }
            }
        }
    }

    /// Creates new lock data with current timestamp and TTL.
    fn create_lock_data(&self) -> LockFileData {
        let now = Utc::now();
        let expires = now + self.lock_ttl;

        LockFileData {
            device_id: self.device_id.clone(),
            acquired_at: now.to_rfc3339(),
            expires_at: expires.to_rfc3339(),
        }
    }

    /// Checks if a lock has expired based on its expires_at timestamp.
    fn is_expired(&self, lock: &LockFileData) -> bool {
        match DateTime::parse_from_rfc3339(&lock.expires_at) {
            Ok(expires) => {
                let expires_utc: DateTime<Utc> = expires.with_timezone(&Utc);
                Utc::now() >= expires_utc
            }
            Err(_) => {
                // If we can't parse the expiry, assume it's expired to be safe
                true
            }
        }
    }

    /// Uploads lock data to cloud storage.
    async fn upload_lock(&self, lock_data: &LockFileData) -> Result<(), SyncError> {
        let json =
            serde_json::to_string(lock_data).map_err(|e| SyncError::SerializationFailed {
                message: e.to_string(),
            })?;
        let bytes: Vec<u8> = json.into_bytes();
        self.storage.upload_raw(LOCK_FILENAME, &bytes).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates a CloudStorage instance backed by in-memory storage.
    fn test_storage() -> CloudStorage {
        let op = opendal::Operator::new(opendal::services::Memory::default())
            .unwrap()
            .finish();
        CloudStorage::new(op, "memory".to_string())
    }

    #[tokio::test]
    async fn acquire_lock_from_empty_cloud() {
        let storage = test_storage();
        let lock = SyncLock::new(storage, "device-1".to_string());

        lock.acquire().await.unwrap();

        assert!(lock.is_locked().await.unwrap());
        assert!(lock.is_locked_by_self().await.unwrap());
    }

    #[tokio::test]
    async fn acquire_lock_when_already_holding() {
        let storage = test_storage();
        let lock = SyncLock::new(storage, "device-1".to_string());

        // First acquisition
        lock.acquire().await.unwrap();
        assert!(lock.is_locked_by_self().await.unwrap());

        // Second acquisition should renew
        lock.acquire().await.unwrap();
        assert!(lock.is_locked_by_self().await.unwrap());
    }

    #[tokio::test]
    async fn acquire_lock_when_another_device_holds_it() {
        let storage = test_storage();
        let lock1 = SyncLock::new(storage.clone(), "device-1".to_string());
        let lock2 = SyncLock::new(storage, "device-2".to_string());

        // Device 1 acquires lock
        lock1.acquire().await.unwrap();
        assert!(lock1.is_locked_by_self().await.unwrap());

        // Device 2 tries to acquire with short timeout
        let short_timeout_lock = lock2.with_timeouts(
            Duration::seconds(300),
            Duration::milliseconds(500), // Very short timeout
            Duration::milliseconds(100), // Short retry interval
        );

        let result = short_timeout_lock.acquire().await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, SyncError::LockAcquireFailed { .. }));
    }

    #[tokio::test]
    async fn release_own_lock() {
        let storage = test_storage();
        let lock = SyncLock::new(storage, "device-1".to_string());

        lock.acquire().await.unwrap();
        assert!(lock.is_locked_by_self().await.unwrap());

        lock.release().await.unwrap();

        assert!(!lock.is_locked().await.unwrap());
        assert!(!lock.is_locked_by_self().await.unwrap());
    }

    #[tokio::test]
    async fn release_when_no_lock_exists() {
        let storage = test_storage();
        let lock = SyncLock::new(storage, "device-1".to_string());

        // Should be idempotent - no error
        lock.release().await.unwrap();

        assert!(!lock.is_locked().await.unwrap());
    }

    #[tokio::test]
    async fn release_when_another_device_holds_lock() {
        let storage = test_storage();
        let lock1 = SyncLock::new(storage.clone(), "device-1".to_string());
        let lock2 = SyncLock::new(storage, "device-2".to_string());

        // Device 1 acquires
        lock1.acquire().await.unwrap();
        assert!(lock1.is_locked_by_self().await.unwrap());

        // Device 2 tries to release (should not delete)
        lock2.release().await.unwrap();

        // Lock should still exist (owned by device-1)
        assert!(lock1.is_locked_by_self().await.unwrap());
        assert!(!lock2.is_locked_by_self().await.unwrap());
    }

    #[tokio::test]
    async fn expired_lock_can_be_overwritten() {
        let storage = test_storage();
        let lock1 = SyncLock::new(storage.clone(), "device-1".to_string());
        let lock2 = SyncLock::new(storage, "device-2".to_string());

        // Device 1 acquires lock with very short TTL
        let short_ttl_lock1 = lock1.with_timeouts(
            Duration::milliseconds(50), // Very short TTL
            Duration::seconds(30),
            Duration::seconds(5),
        );
        short_ttl_lock1.acquire().await.unwrap();

        // Wait for lock to expire
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Device 2 should be able to acquire
        lock2.acquire().await.unwrap();
        assert!(lock2.is_locked_by_self().await.unwrap());
        assert!(!short_ttl_lock1.is_locked_by_self().await.unwrap());
    }

    #[tokio::test]
    async fn is_locked_returns_correct_state() {
        let storage = test_storage();
        let lock = SyncLock::new(storage, "device-1".to_string());

        // Initially not locked
        assert!(!lock.is_locked().await.unwrap());

        // After acquiring
        lock.acquire().await.unwrap();
        assert!(lock.is_locked().await.unwrap());

        // After releasing
        lock.release().await.unwrap();
        assert!(!lock.is_locked().await.unwrap());
    }

    #[tokio::test]
    async fn is_locked_by_self_returns_correct_state() {
        let storage = test_storage();
        let lock = SyncLock::new(storage, "device-1".to_string());

        // Initially not locked by self
        assert!(!lock.is_locked_by_self().await.unwrap());

        // After acquiring
        lock.acquire().await.unwrap();
        assert!(lock.is_locked_by_self().await.unwrap());

        // After releasing
        lock.release().await.unwrap();
        assert!(!lock.is_locked_by_self().await.unwrap());
    }

    #[tokio::test]
    async fn is_expired_detects_expired_lock() {
        let storage = test_storage();

        // Create a lock with 1ms TTL (will expire immediately)
        let lock = SyncLock::new(storage, "device-1".to_string()).with_timeouts(
            Duration::milliseconds(1),
            Duration::seconds(30),
            Duration::seconds(5),
        );

        lock.acquire().await.unwrap();

        // Wait for expiry
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Lock should be expired but not deleted
        assert!(!lock.is_locked().await.unwrap()); // Expired
        assert!(!lock.is_locked_by_self().await.unwrap()); // Not held by self
    }

    #[tokio::test]
    async fn with_timeouts_customizes_values() {
        let storage = test_storage();
        let lock = SyncLock::new(storage, "device-1".to_string()).with_timeouts(
            Duration::seconds(600), // 10 minutes TTL
            Duration::seconds(60),  // 1 minute timeout
            Duration::seconds(10),  // 10 second retry
        );

        lock.acquire().await.unwrap();

        // Verify we can acquire (would timeout quickly with default values if broken)
        assert!(lock.is_locked_by_self().await.unwrap());
    }
}
