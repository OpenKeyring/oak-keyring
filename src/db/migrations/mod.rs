pub mod m001_initial;
pub mod runner;

pub(crate) use runner::read_current_version;
pub use runner::{run_migrations, MigrationError, SCHEMA_VERSION};

#[cfg(test)]
mod m001_initial_test;
