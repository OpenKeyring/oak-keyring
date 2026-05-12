use secrecy::{ExposeSecret, ExposeSecretMut, SecretBox};
use zeroize::Zeroize;

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

pub struct SensitiveInput {
    value: SecureStr,
}

impl SensitiveInput {
    pub fn new() -> Self {
        Self {
            value: SecureStr::new(String::new()),
        }
    }

    pub fn push_char(&mut self, c: char) {
        self.value.expose_mut().push(c);
    }

    pub fn pop_char(&mut self) {
        self.value.expose_mut().pop();
    }

    pub fn clear(&mut self) {
        self.value.expose_mut().zeroize();
        self.value.expose_mut().clear();
    }

    pub fn len(&self) -> usize {
        self.value.expose().chars().count()
    }

    pub fn is_empty(&self) -> bool {
        self.value.expose().is_empty()
    }

    pub fn expose<R>(&self, f: impl FnOnce(&str) -> R) -> R {
        f(self.value.expose())
    }

    pub fn expose_mut<R>(&mut self, f: impl FnOnce(&mut String) -> R) -> R {
        f(self.value.expose_mut())
    }

    pub fn take_secure(&mut self) -> SecureStr {
        std::mem::replace(&mut self.value, SecureStr::new(String::new()))
    }
}

impl Default for SensitiveInput {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SensitiveInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***REDACTED***")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_input_accumulates_and_takes_secret() {
        let mut input = SensitiveInput::new();
        input.push_char('a');
        input.push_char('b');
        input.push_char('c');

        assert_eq!(input.len(), 3);
        assert_eq!(input.expose(|s| s.to_string()), "abc");

        input.pop_char();
        assert_eq!(input.expose(|s| s.to_string()), "ab");

        let secret = input.take_secure();
        assert_eq!(secret.expose(), "ab");
        assert!(input.is_empty());
    }
}
