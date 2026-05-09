use std::path::Path;

use rusqlite::Connection;

use crate::db::migrations::{self, MigrationError};

pub fn apply_pragmas(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA busy_timeout=5000;
         PRAGMA foreign_keys=ON;
         PRAGMA cache_size=-8000;",
    )
}

pub fn init_db(path: &Path) -> Result<Connection, InitDbError> {
    let db_path = path.join("vault.db");
    let conn = Connection::open(&db_path)?;
    apply_pragmas(&conn)?;

    let current = migrations::read_current_version(&conn);
    let needs_migration = current < migrations::SCHEMA_VERSION;

    if current > migrations::SCHEMA_VERSION {
        tracing::warn!(
            current,
            expected = migrations::SCHEMA_VERSION,
            "database schema version is newer than expected"
        );
    }

    if needs_migration && current > 0 {
        let backup_path = path.join("vault.db.migration.bak");
        std::fs::copy(&db_path, &backup_path).map_err(MigrationError::BackupFailed)?;

        match migrations::run_migrations(&conn) {
            Ok(()) => {
                if let Err(e) = std::fs::remove_file(&backup_path) {
                    tracing::warn!(error = %e, "failed to remove migration backup");
                }
            }
            Err(e) => {
                drop(conn);
                if let Err(restore_err) = std::fs::copy(&backup_path, &db_path) {
                    tracing::error!(
                        migration_error = %e,
                        restore_error = %restore_err,
                        "migration failed AND backup restore failed"
                    );
                    return Err(MigrationError::RestoreFailed(restore_err).into());
                }
                return Err(e.into());
            }
        }
    } else if needs_migration {
        migrations::run_migrations(&conn)?;
    }

    Ok(conn)
}

pub fn init_db_in_memory() -> Connection {
    let conn = Connection::open_in_memory().expect("failed to open in-memory db");
    apply_pragmas(&conn).expect("failed to apply pragmas");
    migrations::run_migrations(&conn).expect("migration failed");
    conn
}

#[derive(Debug, thiserror::Error)]
pub enum InitDbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("migration error: {0}")]
    Migration(#[from] MigrationError),
}
