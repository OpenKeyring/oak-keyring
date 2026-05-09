use rusqlite::Connection;

pub mod m001_initial;

/// Current schema version expected by the code.
/// Equals the number of registered migrations.
pub const SCHEMA_VERSION: u32 = 1;

/// A migration function that takes a database connection and applies schema changes.
type MigrationFn = fn(&Connection) -> Result<(), MigrationError>;

/// A single migration entry in the registry.
struct Migration {
    version: u32,
    name: &'static str,
    up: MigrationFn,
}

/// Returns all registered migrations in version order.
fn migrations() -> Vec<Migration> {
    vec![Migration {
        version: 1,
        name: "initial_schema",
        up: m001_initial::up,
    }]
}

/// Reads the current schema version from the metadata table.
/// Returns 0 for an empty database (no metadata table).
pub(crate) fn read_current_version(conn: &Connection) -> u32 {
    conn.query_row(
        "SELECT value FROM metadata WHERE key = 'schema_version'",
        [],
        |row| row.get::<_, String>(0),
    )
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(0)
}

/// Runs all pending migrations on the given connection.
///
/// - Reads current version from metadata table.
/// - Executes each pending migration in order.
/// - Updates schema_version after all migrations succeed.
/// - Logs a warning if database version is newer than expected (downgrade).
pub fn run_migrations(conn: &Connection) -> Result<(), MigrationError> {
    let current = read_current_version(conn);

    if current > SCHEMA_VERSION {
        tracing::warn!(
            current,
            expected = SCHEMA_VERSION,
            "database schema version is newer than expected"
        );
        return Ok(());
    }

    if current == SCHEMA_VERSION {
        return Ok(());
    }

    let pending: Vec<_> = migrations()
        .into_iter()
        .filter(|m| m.version > current)
        .collect();

    for migration in &pending {
        tracing::info!(
            version = migration.version,
            name = migration.name,
            "running migration"
        );
        (migration.up)(conn)?;
    }

    // Update schema_version to the latest.
    conn.execute(
        "INSERT OR REPLACE INTO metadata (key, value) VALUES (?1, ?2)",
        rusqlite::params!["schema_version", SCHEMA_VERSION.to_string()],
    )
    .map_err(|source| MigrationError::ExecutionFailed {
        version: SCHEMA_VERSION,
        name: "update_schema_version".to_string(),
        source,
    })?;

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("migration v{version} '{name}' failed: {source}")]
    ExecutionFailed {
        version: u32,
        name: String,
        source: rusqlite::Error,
    },
    #[error("backup failed: {0}")]
    BackupFailed(std::io::Error),
    #[error("restore failed: {0}")]
    RestoreFailed(std::io::Error),
}

#[cfg(test)]
mod m001_initial_test;
