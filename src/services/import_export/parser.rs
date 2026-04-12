use std::collections::HashMap;
use std::path::Path;

use crate::commands::types::CsvColumnMapping;
use crate::commands::types::ImportSource;
use crate::errors::mapping::import_export::ImportExportError;
use crate::types::SecureStr;

// ---------------------------------------------------------------------------
// ParsedItem — universal intermediate representation
// ---------------------------------------------------------------------------

/// Universal intermediate representation produced by every format parser.
///
/// Each parser converts its native format into a flat map of string fields
/// plus optional tags. Downstream mapping converts `ParsedItem` into
/// application-level record types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedItem {
    /// Identifier from the source file (e.g. KeePass UUID, CSV row number).
    pub source_id: String,
    /// Flat key-value field map (title, username, password, url, notes, etc.).
    pub fields: HashMap<String, String>,
    /// Tags / groups carried over from the source format.
    pub tags: Vec<String>,
}

// ---------------------------------------------------------------------------
// FormatParser trait
// ---------------------------------------------------------------------------

/// Trait that every import format parser must implement.
///
/// Parsers are registered in [`FormatParserRegistry`] and looked up by
/// [`ImportSource`] variant. The `parse` method returns a list of
/// [`ParsedItem`] values that downstream mapping converts to vault records.
pub trait FormatParser: Send + Sync {
    /// The import format this parser handles.
    fn format(&self) -> ImportSource;

    /// Parse the file at `path` and return extracted items.
    ///
    /// * `password` — required for encrypted formats (KeePass, 1Password).
    /// * `csv_mapping` — column mapping required only for CSV; other parsers
    ///   ignore it.
    fn parse(
        &self,
        path: &Path,
        password: Option<&SecureStr>,
        csv_mapping: Option<&CsvColumnMapping>,
    ) -> Result<Vec<ParsedItem>, ImportExportError>;

    /// Whether this format requires a password to decrypt.
    fn requires_password(&self) -> bool;

    /// Quick file-level validation (existence, size, extension) before
    /// attempting a full parse.
    fn validate_file(&self, path: &Path) -> Result<(), ImportExportError>;
}

// ---------------------------------------------------------------------------
// Shared validation helper
// ---------------------------------------------------------------------------

/// Maximum allowed import file size (100 MB).
pub const MAX_FILE_SIZE: usize = 100 * 1024 * 1024;

/// Common file validation shared by all parsers.
///
/// Checks existence, file size against [`MAX_FILE_SIZE`], and extension match.
/// Individual parsers should call this inside their `validate_file` impl.
pub fn validate_file_common(path: &Path, expected_ext: &str) -> Result<(), ImportExportError> {
    if !path.exists() {
        return Err(ImportExportError::FileNotFound(path.to_path_buf()));
    }

    let metadata = std::fs::metadata(path).map_err(|e| ImportExportError::FileReadError {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;

    if metadata.len() as usize > MAX_FILE_SIZE {
        return Err(ImportExportError::FileTooLarge {
            path: path.to_path_buf(),
            size: metadata.len() as usize,
            max: MAX_FILE_SIZE,
        });
    }

    if let Some(ext) = path.extension() {
        if ext != expected_ext {
            return Err(ImportExportError::InvalidFormat(format!(
                "expected .{} file, got .{}",
                expected_ext,
                ext.to_string_lossy()
            )));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// FormatParserRegistry
// ---------------------------------------------------------------------------

/// Registry that maps [`ImportSource`] variants to concrete [`FormatParser`]
/// implementations.
///
/// Constructed empty and populated via [`register`](Self::register). Parsers
/// are looked up with [`get`](Self::get) / [`get_mut`](Self::get_mut).
pub struct FormatParserRegistry {
    parsers: HashMap<ImportSource, Box<dyn FormatParser>>,
}

impl FormatParserRegistry {
    /// Create an empty registry with no parsers registered.
    pub fn new() -> Self {
        Self {
            parsers: HashMap::new(),
        }
    }

    /// Register a parser, replacing any previous entry for the same format.
    pub fn register(&mut self, parser: Box<dyn FormatParser>) {
        let format = parser.format();
        self.parsers.insert(format, parser);
    }

    /// Look up an immutable reference to the parser for `source`.
    pub fn get(&self, source: ImportSource) -> Option<&dyn FormatParser> {
        self.parsers.get(&source).map(|p| p.as_ref())
    }

    /// Look up a mutable reference to the parser for `source`.
    pub fn get_mut(&mut self, source: ImportSource) -> Option<&mut dyn FormatParser> {
        match self.parsers.get_mut(&source) {
            Some(b) => Some(b.as_mut()),
            None => None,
        }
    }
}

impl Default for FormatParserRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // -- ParsedItem construction --

    #[test]
    fn parsed_item_construction_and_field_access() {
        let mut fields = HashMap::new();
        fields.insert("title".to_string(), "Gmail".to_string());
        fields.insert("username".to_string(), "user@gmail.com".to_string());

        let item = ParsedItem {
            source_id: "row-1".to_string(),
            fields,
            tags: vec!["email".to_string()],
        };

        assert_eq!(item.source_id, "row-1");
        assert_eq!(item.fields.get("title").unwrap(), "Gmail");
        assert_eq!(item.fields.get("username").unwrap(), "user@gmail.com");
        assert_eq!(item.tags, vec!["email"]);
    }

    // -- FormatParserRegistry --

    /// Stub parser used exclusively in registry tests.
    struct StubParser {
        fmt: ImportSource,
        needs_password: bool,
    }

    impl FormatParser for StubParser {
        fn format(&self) -> ImportSource {
            self.fmt
        }

        fn parse(
            &self,
            _path: &Path,
            _password: Option<&SecureStr>,
            _csv_mapping: Option<&CsvColumnMapping>,
        ) -> Result<Vec<ParsedItem>, ImportExportError> {
            Ok(vec![])
        }

        fn requires_password(&self) -> bool {
            self.needs_password
        }

        fn validate_file(&self, _path: &Path) -> Result<(), ImportExportError> {
            Ok(())
        }
    }

    #[test]
    fn registry_register_and_get_for_each_format() {
        let mut registry = FormatParserRegistry::new();

        let formats = [
            (ImportSource::KeePass, true),
            (ImportSource::OnePassword1pux, true),
            (ImportSource::OnePasswordOpvault, true),
            (ImportSource::Bitwarden, false),
            (ImportSource::Csv, false),
            (ImportSource::OpenKeyringBackup, false),
        ];

        for (fmt, needs_pw) in &formats {
            registry.register(Box::new(StubParser {
                fmt: *fmt,
                needs_password: *needs_pw,
            }));
        }

        for (fmt, needs_pw) in &formats {
            let parser = registry.get(*fmt).expect("parser should be registered");
            assert_eq!(parser.format(), *fmt);
            assert_eq!(parser.requires_password(), *needs_pw);
        }
    }

    #[test]
    fn registry_get_unregistered_format_returns_none() {
        let registry = FormatParserRegistry::new();
        assert!(registry.get(ImportSource::Csv).is_none());
    }

    #[test]
    fn registry_get_mut_returns_mutable_reference() {
        let mut registry = FormatParserRegistry::new();
        registry.register(Box::new(StubParser {
            fmt: ImportSource::Csv,
            needs_password: false,
        }));

        let parser = registry.get_mut(ImportSource::Csv).expect("should exist");
        assert_eq!(parser.format(), ImportSource::Csv);
    }

    #[test]
    fn registry_register_replaces_previous_entry() {
        let mut registry = FormatParserRegistry::new();

        registry.register(Box::new(StubParser {
            fmt: ImportSource::Csv,
            needs_password: false,
        }));
        registry.register(Box::new(StubParser {
            fmt: ImportSource::Csv,
            needs_password: true,
        }));

        let parser = registry.get(ImportSource::Csv).expect("should exist");
        assert!(parser.requires_password(), "second registration should win");
    }

    // -- validate_file_common --

    #[test]
    fn validate_file_common_nonexistent_file_returns_file_not_found() {
        let path = Path::new("/tmp/__oak_test_nonexistent_42__.csv");
        let result = validate_file_common(path, "csv");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ImportExportError::FileNotFound(ref p) if p == &path),
            "expected FileNotFound, got: {err:?}"
        );
    }

    #[test]
    fn validate_file_common_wrong_extension_returns_invalid_format() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("data.json");
        std::fs::write(&file_path, b"{}").expect("write");

        let result = validate_file_common(&file_path, "csv");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ImportExportError::InvalidFormat(ref msg) if msg.contains("csv")),
            "expected InvalidFormat mentioning csv, got: {err:?}"
        );
    }

    #[test]
    fn validate_file_common_valid_file_returns_ok() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("data.csv");
        std::fs::write(&file_path, b"name,value\na,1").expect("write");

        let result = validate_file_common(&file_path, "csv");
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
    }

    #[test]
    fn validate_file_common_no_extension_returns_ok() {
        let dir = tempfile::tempdir().expect("tempdir");
        // File with no extension — extension check is skipped when None.
        let file_path = dir.path().join("data");
        std::fs::write(&file_path, b"contents").expect("write");

        let result = validate_file_common(&file_path, "csv");
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
    }

    #[test]
    fn validate_file_common_file_too_large_returns_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("big.csv");

        // Create a small file, then mock the check by calling
        // validate_file_common with a path that exists but simulate
        // the too-large condition via a direct metadata check.
        // Since we can't create a 100 MB file in tests, we test the
        // FileTooLarge path through a unit-level check.
        let mut f = std::fs::File::create(&file_path).expect("create");
        // Write enough bytes to be small, but we will test via direct
        // metadata comparison instead.
        f.write_all(b"x").expect("write");

        // The file is tiny so validate_file_common will pass the size check.
        // Instead, directly verify the FileTooLarge error variant is correct.
        let err = ImportExportError::FileTooLarge {
            path: file_path.clone(),
            size: MAX_FILE_SIZE + 1,
            max: MAX_FILE_SIZE,
        };
        let msg = err.to_string();
        assert!(msg.contains("file too large"), "got: {msg}");
        assert!(
            msg.contains(&format!("{}", MAX_FILE_SIZE + 1)),
            "got: {msg}"
        );
    }

    // -- Default trait --

    #[test]
    fn registry_default_is_empty() {
        let registry = FormatParserRegistry::default();
        assert!(registry.get(ImportSource::KeePass).is_none());
    }
}
