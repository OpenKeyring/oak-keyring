use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

/// Structured error context for i18n interpolation and logging.
///
/// Per spec §6.3, provides typed fields with builder pattern.
/// `record_id` is for logging only and does NOT appear in interpolation map.
///
/// `extra` serves as a fallback HashMap for diagnostic kv pairs that don't
/// have a dedicated typed slot (e.g. checksum values, AAD fields, lock reasons,
/// state transition names). These are merged into the interpolation map.
#[derive(Debug, Clone, Default)]
pub struct ErrorContext {
    /// Record ID (logging only, not shown to user)
    pub record_id: Option<Uuid>,
    /// Record name (i18n interpolation: `%{record_name}`)
    pub record_name: Option<String>,
    /// Field name (i18n interpolation: `%{field_name}`)
    pub field_name: Option<String>,
    /// Sync provider name (i18n interpolation: `%{provider}`)
    pub provider_name: Option<String>,
    /// File path (i18n interpolation: `%{path}`)
    pub file_path: Option<PathBuf>,
    /// Optimistic lock expected version (i18n interpolation: `%{expected}`)
    pub expected_version: Option<u64>,
    /// Optimistic lock actual version (i18n interpolation: `%{actual}`)
    pub actual_version: Option<u64>,
    /// Remaining attempt count (i18n interpolation: `%{remaining}`)
    pub attempt_count: Option<u32>,
    /// Fallback diagnostic kv pairs merged into interpolation map.
    /// For data that does not fit the typed fields above.
    pub extra: HashMap<String, String>,
}

impl ErrorContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_id(mut self, id: Uuid) -> Self {
        self.record_id = Some(id);
        self
    }

    pub fn record_name(mut self, name: impl Into<String>) -> Self {
        self.record_name = Some(name.into());
        self
    }

    pub fn field_name(mut self, name: impl Into<String>) -> Self {
        self.field_name = Some(name.into());
        self
    }

    pub fn provider_name(mut self, name: impl Into<String>) -> Self {
        self.provider_name = Some(name.into());
        self
    }

    pub fn file_path(mut self, path: PathBuf) -> Self {
        self.file_path = Some(path);
        self
    }

    pub fn expected_version(mut self, v: u64) -> Self {
        self.expected_version = Some(v);
        self
    }

    pub fn actual_version(mut self, v: u64) -> Self {
        self.actual_version = Some(v);
        self
    }

    pub fn attempt_count(mut self, count: u32) -> Self {
        self.attempt_count = Some(count);
        self
    }

    /// Add a diagnostic key-value pair not covered by typed fields.
    pub fn extra(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }

    /// Convert to HashMap for i18n variable substitution.
    ///
    /// Per spec §8.5, `record_id` is NOT included (UUID not shown to user).
    /// `extra` entries are merged last so they cannot shadow typed keys.
    pub fn to_interpolation_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();

        if let Some(ref name) = self.record_name {
            map.insert("record_name".to_string(), name.clone());
        }
        if let Some(ref provider) = self.provider_name {
            map.insert("provider".to_string(), provider.clone());
        }
        if let Some(ref path) = self.file_path {
            map.insert("path".to_string(), path.to_string_lossy().to_string());
        }
        if let Some(expected) = self.expected_version {
            map.insert("expected".to_string(), expected.to_string());
        }
        if let Some(actual) = self.actual_version {
            map.insert("actual".to_string(), actual.to_string());
        }
        if let Some(count) = self.attempt_count {
            map.insert("remaining".to_string(), count.to_string());
        }
        if let Some(ref field) = self.field_name {
            map.insert("field_name".to_string(), field.clone());
        }

        // Merge extra fallback fields — typed slots take precedence.
        for (k, v) in &self.extra {
            map.entry(k.clone()).or_insert_with(|| v.clone());
        }

        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_pattern_chains() {
        let ctx = ErrorContext::new()
            .record_id(Uuid::new_v4())
            .record_name("GitHub")
            .field_name("password")
            .expected_version(5)
            .actual_version(7);

        assert!(ctx.record_id.is_some());
        assert_eq!(ctx.record_name, Some("GitHub".to_string()));
        assert_eq!(ctx.field_name, Some("password".to_string()));
        assert_eq!(ctx.expected_version, Some(5));
        assert_eq!(ctx.actual_version, Some(7));
    }

    #[test]
    fn extra_field_merged_into_map() {
        let ctx = ErrorContext::new()
            .extra("local_token", "abc123")
            .extra("remote_token", "xyz789");

        let map = ctx.to_interpolation_map();
        assert_eq!(map.get("local_token"), Some(&"abc123".to_string()));
        assert_eq!(map.get("remote_token"), Some(&"xyz789".to_string()));
    }

    #[test]
    fn typed_slot_takes_precedence_over_extra() {
        let ctx = ErrorContext::new()
            .record_name("TypedName")
            .extra("record_name", "ExtraName");

        let map = ctx.to_interpolation_map();
        assert_eq!(map.get("record_name"), Some(&"TypedName".to_string()));
    }

    #[test]
    fn interpolation_map_excludes_record_id() {
        let id = Uuid::new_v4();
        let ctx = ErrorContext::new().record_id(id).record_name("TestRecord");

        let map = ctx.to_interpolation_map();

        assert!(!map.contains_key("record_id"));
        assert!(!map.contains_key(&id.to_string()));
        assert_eq!(map.get("record_name"), Some(&"TestRecord".to_string()));
    }

    #[test]
    fn interpolation_map_provider_key() {
        let ctx = ErrorContext::new().provider_name("iCloud");
        let map = ctx.to_interpolation_map();
        assert_eq!(map.get("provider"), Some(&"iCloud".to_string()));
    }

    #[test]
    fn interpolation_map_path_key() {
        let ctx = ErrorContext::new().file_path(PathBuf::from("/tmp/test.db"));
        let map = ctx.to_interpolation_map();
        assert_eq!(map.get("path"), Some(&"/tmp/test.db".to_string()));
    }

    #[test]
    fn interpolation_map_version_keys() {
        let ctx = ErrorContext::new().expected_version(1).actual_version(2);
        let map = ctx.to_interpolation_map();
        assert_eq!(map.get("expected"), Some(&"1".to_string()));
        assert_eq!(map.get("actual"), Some(&"2".to_string()));
    }

    #[test]
    fn interpolation_map_attempt_count_key() {
        let ctx = ErrorContext::new().attempt_count(3);
        let map = ctx.to_interpolation_map();
        assert_eq!(map.get("remaining"), Some(&"3".to_string()));
    }

    #[test]
    fn interpolation_map_field_name_key() {
        let ctx = ErrorContext::new().field_name("username");
        let map = ctx.to_interpolation_map();
        assert_eq!(map.get("field_name"), Some(&"username".to_string()));
    }

    #[test]
    fn default_empty_context() {
        let ctx = ErrorContext::default();
        assert!(ctx.record_id.is_none());
        assert!(ctx.record_name.is_none());
        assert!(ctx.field_name.is_none());
        assert!(ctx.provider_name.is_none());
        assert!(ctx.file_path.is_none());
        assert!(ctx.expected_version.is_none());
        assert!(ctx.actual_version.is_none());
        assert!(ctx.attempt_count.is_none());
        assert!(ctx.extra.is_empty());

        let map = ctx.to_interpolation_map();
        assert!(map.is_empty());
    }
}
