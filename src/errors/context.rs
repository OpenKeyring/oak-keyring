use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

/// Structured error context for interpolating error messages.
///
/// ErrorContext provides typed fields for common error context, which can be
/// converted into a key-value map for message interpolation. Fields are optional
/// to allow flexible context composition.
#[derive(Debug, Clone, Default)]
pub struct ErrorContext {
    /// Unique identifier of a record (e.g., vault record, sync state)
    pub record_id: Option<Uuid>,

    /// Human-readable name of a record or entity
    pub record_name: Option<String>,

    /// Name of a field that caused an error (e.g., "password", "email")
    pub field_name: Option<String>,

    /// Name of a cloud provider or service (e.g., "icloud", "s3")
    pub provider_name: Option<String>,

    /// File system path involved in the error
    pub file_path: Option<PathBuf>,

    /// Expected version number for version conflict errors
    pub expected_version: Option<u64>,

    /// Actual version number for version conflict errors
    pub actual_version: Option<u64>,

    /// Number of attempts made (e.g., retry attempts, connection attempts)
    pub attempt_count: Option<u32>,
}

impl ErrorContext {
    /// Creates a new empty ErrorContext.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use oak_keyring::errors::ErrorContext;
    ///
    /// let ctx = ErrorContext::new();
    /// assert!(ctx.record_id.is_none());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder method: sets the record ID.
    pub fn record_id(mut self, id: Uuid) -> Self {
        self.record_id = Some(id);
        self
    }

    /// Builder method: sets the record name.
    pub fn record_name(mut self, name: impl Into<String>) -> Self {
        self.record_name = Some(name.into());
        self
    }

    /// Builder method: sets the field name.
    pub fn field_name(mut self, name: impl Into<String>) -> Self {
        self.field_name = Some(name.into());
        self
    }

    /// Builder method: sets the provider name.
    pub fn provider_name(mut self, name: impl Into<String>) -> Self {
        self.provider_name = Some(name.into());
        self
    }

    /// Builder method: sets the file path.
    pub fn file_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.file_path = Some(path.into());
        self
    }

    /// Builder method: sets the expected version.
    pub fn expected_version(mut self, version: u64) -> Self {
        self.expected_version = Some(version);
        self
    }

    /// Builder method: sets the actual version.
    pub fn actual_version(mut self, version: u64) -> Self {
        self.actual_version = Some(version);
        self
    }

    /// Builder method: sets the attempt count.
    pub fn attempt_count(mut self, count: u32) -> Self {
        self.attempt_count = Some(count);
        self
    }

    /// Converts the context into a key-value map for message interpolation.
    ///
    /// # Exclusions
    ///
    /// The `record_id` field is excluded from the map because UUIDs are not
    /// user-friendly and should not be displayed directly in error messages.
    ///
    /// # Returns
    ///
    /// A HashMap where keys are field names in snake_case and values are
    /// string representations of the field values.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use oak_keyring::errors::ErrorContext;
    /// use std::path::PathBuf;
    ///
    /// let ctx = ErrorContext::new()
    ///     .record_name("My Password")
    ///     .field_name("password")
    ///     .attempt_count(3);
    ///
    /// let map = ctx.to_interpolation_map();
    /// assert_eq!(map.get("record_name"), Some(&"My Password".to_string()));
    /// assert_eq!(map.get("field_name"), Some(&"password".to_string()));
    /// assert_eq!(map.get("attempt_count"), Some(&"3".to_string()));
    /// assert!(!map.contains_key("record_id")); // record_id is excluded
    /// ```
    pub fn to_interpolation_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();

        if let Some(ref name) = self.record_name {
            map.insert("record_name".to_string(), name.clone());
        }

        if let Some(ref field) = self.field_name {
            map.insert("field_name".to_string(), field.clone());
        }

        if let Some(ref provider) = self.provider_name {
            map.insert("provider_name".to_string(), provider.clone());
        }

        if let Some(ref path) = self.file_path {
            map.insert("file_path".to_string(), path.display().to_string());
        }

        if let Some(expected) = self.expected_version {
            map.insert("expected_version".to_string(), expected.to_string());
        }

        if let Some(actual) = self.actual_version {
            map.insert("actual_version".to_string(), actual.to_string());
        }

        if let Some(count) = self.attempt_count {
            map.insert("attempt_count".to_string(), count.to_string());
        }

        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn builder_pattern_works() {
        let id = Uuid::new_v4();
        let ctx = ErrorContext::new()
            .record_id(id)
            .record_name("Test Record")
            .field_name("password")
            .provider_name("icloud")
            .file_path("/vault/ok.db")
            .expected_version(5)
            .actual_version(3)
            .attempt_count(2);

        assert_eq!(ctx.record_id, Some(id));
        assert_eq!(ctx.record_name, Some("Test Record".to_string()));
        assert_eq!(ctx.field_name, Some("password".to_string()));
        assert_eq!(ctx.provider_name, Some("icloud".to_string()));
        assert_eq!(ctx.file_path, Some(PathBuf::from("/vault/ok.db")));
        assert_eq!(ctx.expected_version, Some(5));
        assert_eq!(ctx.actual_version, Some(3));
        assert_eq!(ctx.attempt_count, Some(2));
    }

    #[test]
    fn builder_chain_is_flexible() {
        // Test that builder methods can be called in any order
        let ctx1 = ErrorContext::new().record_name("Test").attempt_count(1);

        let ctx2 = ErrorContext::new().attempt_count(1).record_name("Test");

        assert_eq!(ctx1.record_name, ctx2.record_name);
        assert_eq!(ctx1.attempt_count, ctx2.attempt_count);
    }

    #[test]
    fn interpolation_map_excludes_record_id() {
        let id = Uuid::new_v4();
        let ctx = ErrorContext::new().record_id(id).record_name("Test");

        let map = ctx.to_interpolation_map();
        assert!(!map.contains_key("record_id"));
        assert!(map.contains_key("record_name"));
    }

    #[test]
    fn interpolation_map_includes_all_other_fields() {
        let id = Uuid::new_v4();
        let ctx = ErrorContext::new()
            .record_id(id) // Should be excluded
            .record_name("My Record")
            .field_name("email")
            .provider_name("s3")
            .file_path("/data/backup.zip")
            .expected_version(10)
            .actual_version(8)
            .attempt_count(5);

        let map = ctx.to_interpolation_map();

        assert_eq!(map.len(), 7); // All fields except record_id
        assert_eq!(map.get("record_name"), Some(&"My Record".to_string()));
        assert_eq!(map.get("field_name"), Some(&"email".to_string()));
        assert_eq!(map.get("provider_name"), Some(&"s3".to_string()));
        assert_eq!(map.get("file_path"), Some(&"/data/backup.zip".to_string()));
        assert_eq!(map.get("expected_version"), Some(&"10".to_string()));
        assert_eq!(map.get("actual_version"), Some(&"8".to_string()));
        assert_eq!(map.get("attempt_count"), Some(&"5".to_string()));
    }

    #[test]
    fn interpolation_map_with_partial_context() {
        // Only some fields are set
        let ctx = ErrorContext::new().record_name("Partial").attempt_count(1);

        let map = ctx.to_interpolation_map();

        assert_eq!(map.len(), 2);
        assert_eq!(map.get("record_name"), Some(&"Partial".to_string()));
        assert_eq!(map.get("attempt_count"), Some(&"1".to_string()));
    }

    #[test]
    fn interpolation_map_with_empty_context() {
        let ctx = ErrorContext::new();
        let map = ctx.to_interpolation_map();

        assert!(map.is_empty());
    }

    #[test]
    fn error_context_is_cloneable() {
        let ctx = ErrorContext::new().record_name("Test").attempt_count(1);

        let cloned = ctx.clone();
        assert_eq!(ctx.record_name, cloned.record_name);
        assert_eq!(ctx.attempt_count, cloned.attempt_count);
    }

    #[test]
    fn error_context_default_is_empty() {
        let ctx = ErrorContext::default();
        assert!(ctx.record_id.is_none());
        assert!(ctx.record_name.is_none());
        assert!(ctx.field_name.is_none());
        assert!(ctx.provider_name.is_none());
        assert!(ctx.file_path.is_none());
        assert!(ctx.expected_version.is_none());
        assert!(ctx.actual_version.is_none());
        assert!(ctx.attempt_count.is_none());
    }
}
