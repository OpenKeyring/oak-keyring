use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct ErrorContext {
    pub fields: HashMap<String, String>,
}

impl ErrorContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, key: &str, value: &str) -> Self {
        self.fields.insert(key.to_string(), value.to_string());
        self
    }
}
