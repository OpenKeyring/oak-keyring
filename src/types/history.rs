use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::types::sensitive::SecureStr;

#[derive(Debug, Clone)]
pub struct PasswordHistory {
    pub id: i64,
    pub record_id: Uuid,
    pub encrypted_password: Vec<u8>,
    pub nonce: [u8; 24],
    pub dek_version: u32,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct PasswordHistoryView {
    pub id: i64,
    pub password: SecureStr,
    pub changed_at: DateTime<Utc>,
}
