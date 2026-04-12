//! Cloud sync schema constants.
//!
//! Defines file format constants and filenames used by the cloud sync service.

/// File format version. Incremented when the format changes.
pub const FORMAT_VERSION: u32 = 1;

/// Semantic schema version identifier.
pub const SCHEMA_VERSION: &str = "open-keyring-v1";

/// Filename for the metadata file.
pub const METADATA_FILENAME: &str = ".metadata.json";

/// Directory name for encrypted record files.
pub const RECORDS_DIR: &str = "records";

/// Filename for the sync lock file.
pub const LOCK_FILENAME: &str = ".sync.lock";
