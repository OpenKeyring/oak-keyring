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
                let mut map = serializer.serialize_map(Some(5))?;
                map.serialize_entry("name", name)?;
                map.serialize_entry("username", username)?;
                map.serialize_entry("password", password.expose())?;
                if let Some(url) = url {
                    map.serialize_entry("url", url)?;
                }
                if let Some(notes) = notes {
                    map.serialize_entry("notes", notes)?;
                }
                map.end()
            }
            EncryptedPayload::Api { name, app_id, secret_key, url, notes } => {
                let mut map = serializer.serialize_map(Some(5))?;
                map.serialize_entry("name", name)?;
                map.serialize_entry("app_id", app_id)?;
                map.serialize_entry("secret_key", secret_key.expose())?;
                if let Some(url) = url {
                    map.serialize_entry("url", url)?;
                }
                if let Some(notes) = notes {
                    map.serialize_entry("notes", notes)?;
                }
                map.end()
            }
            EncryptedPayload::Ssh { name, public_key, private_key, passphrase, notes } => {
                let mut map = serializer.serialize_map(Some(5))?;
                map.serialize_entry("name", name)?;
                map.serialize_entry("public_key", public_key)?;
                if let Some(private_key) = private_key {
                    map.serialize_entry("private_key", private_key.expose())?;
                }
                if let Some(passphrase) = passphrase {
                    map.serialize_entry("passphrase", passphrase.expose())?;
                }
                if let Some(notes) = notes {
                    map.serialize_entry("notes", notes)?;
                }
                map.end()
            }
        }
    }
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
                formatter.write_str("an object with variant discriminator")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                // Try to determine variant by checking which fields exist
                let mut name = None;
                let mut username = None;
                let mut password = None;
                let mut app_id = None;
                let mut secret_key = None;
                let mut public_key = None;
                let mut private_key = None;
                let mut passphrase = None;
                let mut url = None;
                let mut notes = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => { name = Some(map.next_value()?); }
                        "username" => { username = Some(map.next_value()?); }
                        "password" => {
                            let pw: String = map.next_value()?;
                            password = Some(SecureStr::new(pw));
                        }
                        "app_id" => { app_id = Some(map.next_value()?); }
                        "secret_key" => {
                            let key: String = map.next_value()?;
                            secret_key = Some(SecureStr::new(key));
                        }
                        "public_key" => { public_key = Some(map.next_value()?); }
                        "private_key" => {
                            if let Some(pk) = map.next_value::<Option<String>>()? {
                                private_key = Some(SecureStr::new(pk));
                            }
                        }
                        "passphrase" => {
                            if let Some(pp) = map.next_value::<Option<String>>()? {
                                passphrase = Some(SecureStr::new(pp));
                            }
                        }
                        "url" => { url = map.next_value()?; }
                        "notes" => { notes = map.next_value()?; }
                        _ => { map.next_value::<de::IgnoredAny>()?; }
                    }
                }

                // Determine variant based on which fields are present
                if password.is_some() {
                    Ok(EncryptedPayload::Login {
                        name: name.ok_or_else(|| de::Error::missing_field("name"))?,
                        username: username.ok_or_else(|| de::Error::missing_field("username"))?,
                        password: password.ok_or_else(|| de::Error::missing_field("password"))?,
                        url,
                        notes,
                    })
                } else if secret_key.is_some() {
                    Ok(EncryptedPayload::Api {
                        name: name.ok_or_else(|| de::Error::missing_field("name"))?,
                        app_id: app_id.ok_or_else(|| de::Error::missing_field("app_id"))?,
                        secret_key: secret_key.ok_or_else(|| de::Error::missing_field("secret_key"))?,
                        url,
                        notes,
                    })
                } else if public_key.is_some() {
                    Ok(EncryptedPayload::Ssh {
                        name: name.ok_or_else(|| de::Error::missing_field("name"))?,
                        public_key: public_key.ok_or_else(|| de::Error::missing_field("public_key"))?,
                        private_key,
                        passphrase,
                        notes,
                    })
                } else {
                    Err(de::Error::custom("unable to determine EncryptedPayload variant"))
                }
            }
        }

        deserializer.deserialize_map(EncryptedPayloadVisitor)
    }
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
