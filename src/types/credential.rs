use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::types::sensitive::SecureStr;

#[derive(Debug, Error)]
pub enum DataError {
    #[error("Invalid credential type: {0}")]
    InvalidCredentialType(String),
    #[error("Invalid audit operation: {0}")]
    InvalidAuditOperation(String),
    #[error("Invalid sync status value: {0}")]
    InvalidSyncStatus(i64),
    #[error("Invalid UUID: {0}")]
    InvalidUuid(String),
    #[error("Missing field: {0}")]
    MissingField(&'static str),
    #[error("Field too long: {field}, max {max}, actual {actual}")]
    FieldTooLong {
        field: &'static str,
        max: usize,
        actual: usize,
    },
    #[error("Empty field: {0}")]
    EmptyField(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CredentialType {
    Login,
    Api,
    Ssh,
}

impl CredentialType {
    pub fn to_db_str(self) -> &'static str {
        match self {
            CredentialType::Login => "login",
            CredentialType::Api => "api",
            CredentialType::Ssh => "ssh",
        }
    }

    pub fn from_db_str(s: &str) -> Result<Self, DataError> {
        match s {
            "login" => Ok(CredentialType::Login),
            "api" => Ok(CredentialType::Api),
            "ssh" => Ok(CredentialType::Ssh),
            _ => Err(DataError::InvalidCredentialType(s.to_string())),
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            CredentialType::Login => "登录",
            CredentialType::Api => "API",
            CredentialType::Ssh => "SSH",
        }
    }

    pub fn list_prefix(self) -> &'static str {
        match self {
            CredentialType::Login => "[L]",
            CredentialType::Api => "[A]",
            CredentialType::Ssh => "[S]",
        }
    }
}

#[derive(Debug)]
pub enum EncryptedPayload {
    Login {
        name: String,
        username: String,
        password: SecureStr,
        url: Option<String>,
        notes: Option<String>,
    },
    Api {
        name: String,
        app_id: String,
        secret_key: SecureStr,
        url: Option<String>,
        notes: Option<String>,
    },
    Ssh {
        name: String,
        public_key: String,
        private_key: Option<SecureStr>,
        passphrase: Option<SecureStr>,
        notes: Option<String>,
    },
}

impl EncryptedPayload {
    pub fn name(&self) -> &str {
        match self {
            EncryptedPayload::Login { name, .. } => name,
            EncryptedPayload::Api { name, .. } => name,
            EncryptedPayload::Ssh { name, .. } => name,
        }
    }

    pub fn credential_type(&self) -> CredentialType {
        match self {
            EncryptedPayload::Login { .. } => CredentialType::Login,
            EncryptedPayload::Api { .. } => CredentialType::Api,
            EncryptedPayload::Ssh { .. } => CredentialType::Ssh,
        }
    }
}
