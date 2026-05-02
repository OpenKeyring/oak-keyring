// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("data conversion error: {0}")]
    Data(#[from] crate::types::credential::DataError),
    #[error("invalid UUID: {0}")]
    Uuid(#[from] uuid::Error),
}

pub(crate) type Result<T> = std::result::Result<T, DbError>;

// ---------------------------------------------------------------------------
// Sub-modules
// ---------------------------------------------------------------------------

mod audit;
mod metadata;
mod password_history;
mod record;
mod sync;
mod tag;

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// Re-exports — all public functions available at `crate::db::queries::*`
// ---------------------------------------------------------------------------

pub use audit::*;
pub use metadata::*;
pub use password_history::*;
pub use record::*;
pub use sync::*;
pub use tag::*;
