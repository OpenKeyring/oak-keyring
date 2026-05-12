use secrecy::{ExposeSecret, ExposeSecretMut, SecretBox};

pub struct SecureStr(SecretBox<String>);

impl SecureStr {
    pub fn new(value: String) -> Self {
        Self(SecretBox::new(Box::new(value)))
    }

    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }

    pub(crate) fn expose_mut(&mut self) -> &mut String {
        self.0.expose_secret_mut()
    }
}

impl std::fmt::Debug for SecureStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***REDACTED***")
    }
}

pub struct SecureBytes(SecretBox<Vec<u8>>);

impl SecureBytes {
    pub fn new(value: Vec<u8>) -> Self {
        Self(SecretBox::new(Box::new(value)))
    }

    pub fn expose(&self) -> &[u8] {
        self.0.expose_secret()
    }

    pub(crate) fn expose_mut(&mut self) -> &mut Vec<u8> {
        self.0.expose_secret_mut()
    }
}

impl std::fmt::Debug for SecureBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***REDACTED***")
    }
}
