pub mod migrations;
pub mod models;
pub mod queries;
pub mod schema;
#[cfg(feature = "sqlcipher")]
pub mod sqlcipher;
#[cfg(feature = "sqlcipher")]
pub mod vault_db;

pub use queries::DbError;
pub use schema::{init_db, init_db_in_memory, InitDbError};

#[cfg(test)]
mod models_test;

#[cfg(test)]
mod queries_test;

#[cfg(test)]
mod schema_test;
