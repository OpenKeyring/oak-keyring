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

// Custom serialization for EncryptedPayload that exposes secrets during serialization
// This is safe because the serialized JSON is immediately encrypted
impl serde::Serialize for EncryptedPayload {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;

        match self {
            EncryptedPayload::Login { name, username, password, url, notes } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("Login", &LoginVariant {
                    name,
                    username,
                    password: password.expose(),
                    url,
                    notes,
                })?;
                map.end()
            }
            EncryptedPayload::Api { name, app_id, secret_key, url, notes } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("Api", &ApiVariant {
                    name,
                    app_id,
                    secret_key: secret_key.expose(),
                    url,
                    notes,
                })?;
                map.end()
            }
            EncryptedPayload::Ssh { name, public_key, private_key, passphrase, notes } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("Ssh", &SshVariant {
                    name,
                    public_key,
                    private_key: private_key.as_ref().map(|pk| pk.expose()),
                    passphrase: passphrase.as_ref().map(|pp| pp.expose()),
                    notes,
                })?;
                map.end()
            }
        }
    }
}

// Helper structs for serialization
#[derive(serde::Serialize)]
struct LoginVariant<'a> {
    name: &'a String,
    username: &'a String,
    password: &'a str,
    url: &'a Option<String>,
    notes: &'a Option<String>,
}

#[derive(serde::Serialize)]
struct ApiVariant<'a> {
    name: &'a String,
    app_id: &'a String,
    secret_key: &'a str,
    url: &'a Option<String>,
    notes: &'a Option<String>,
}

#[derive(serde::Serialize)]
struct SshVariant<'a> {
    name: &'a String,
    public_key: &'a String,
    private_key: Option<&'a str>,
    passphrase: Option<&'a str>,
    notes: &'a Option<String>,
}

// Custom deserialization for EncryptedPayload that wraps secrets in SecureStr
impl<'de> serde::Deserialize<'de> for EncryptedPayload {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        struct EncryptedPayloadVisitor;

        impl<'de> Visitor<'de> for EncryptedPayloadVisitor {
            type Value = EncryptedPayload;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an externally-tagged enum with Login, Api, or Ssh variant")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                // Externally-tagged enums have the variant name as the key
                let variant_key = map.next_key::<String>()?
                    .ok_or_else(|| de::Error::custom("missing variant key"))?;

                match variant_key.as_str() {
                    "Login" => {
                        let login: LoginFields = map.next_value()?;
                        Ok(EncryptedPayload::Login {
                            name: login.name,
                            username: login.username,
                            password: SecureStr::new(login.password),
                            url: login.url,
                            notes: login.notes,
                        })
                    }
                    "Api" => {
                        let api: ApiFields = map.next_value()?;
                        Ok(EncryptedPayload::Api {
                            name: api.name,
                            app_id: api.app_id,
                            secret_key: SecureStr::new(api.secret_key),
                            url: api.url,
                            notes: api.notes,
                        })
                    }
                    "Ssh" => {
                        let ssh: SshFields = map.next_value()?;
                        Ok(EncryptedPayload::Ssh {
                            name: ssh.name,
                            public_key: ssh.public_key,
                            private_key: ssh.private_key.map(|pk| SecureStr::new(pk)),
                            passphrase: ssh.passphrase.map(|pp| SecureStr::new(pp)),
                            notes: ssh.notes,
                        })
                    }
                    _ => Err(de::Error::custom(format!("unknown variant: {variant_key}"))),
                }
            }
        }

        deserializer.deserialize_map(EncryptedPayloadVisitor)
    }
}

// Helper structs for deserialization
#[derive(serde::Deserialize)]
struct LoginFields {
    name: String,
    username: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
}

#[derive(serde::Deserialize)]
struct ApiFields {
    name: String,
    app_id: String,
    secret_key: String,
    url: Option<String>,
    notes: Option<String>,
}

#[derive(serde::Deserialize)]
struct SshFields {
    name: String,
    public_key: String,
    private_key: Option<String>,
    passphrase: Option<String>,
    notes: Option<String>,
}

// Clone implementation for EncryptedPayload
// This clones the underlying secret values, which is intentional for internal operations
impl Clone for EncryptedPayload {
    fn clone(&self) -> Self {
        match self {
            EncryptedPayload::Login { name, username, password, url, notes } => {
                EncryptedPayload::Login {
                    name: name.clone(),
                    username: username.clone(),
                    password: SecureStr::new(password.expose().to_string()),
                    url: url.clone(),
                    notes: notes.clone(),
                }
            }
            EncryptedPayload::Api { name, app_id, secret_key, url, notes } => {
                EncryptedPayload::Api {
                    name: name.clone(),
                    app_id: app_id.clone(),
                    secret_key: SecureStr::new(secret_key.expose().to_string()),
                    url: url.clone(),
                    notes: notes.clone(),
                }
            }
            EncryptedPayload::Ssh { name, public_key, private_key, passphrase, notes } => {
                EncryptedPayload::Ssh {
                    name: name.clone(),
                    public_key: public_key.clone(),
                    private_key: private_key.as_ref().map(|pk| SecureStr::new(pk.expose().to_string())),
                    passphrase: passphrase.as_ref().map(|pp| SecureStr::new(pp.expose().to_string())),
                    notes: notes.clone(),
                }
            }
        }
    }
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
