use std::path::Path;

use rusqlite::Connection;

use crate::db::migrations::{self, MigrationError};

pub fn apply_pragmas(conn: &Connection) {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA busy_timeout=5000;
         PRAGMA foreign_keys=ON;
         PRAGMA cache_size=-8000;",
    )
    .expect("failed to apply pragmas");
}

pub fn init_db(path: &Path) -> Result<Connection, InitDbError> {
    let db_path = path.join("vault.db");
    let conn = Connection::open(&db_path)?;
    apply_pragmas(&conn);

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
                let _ = std::fs::remove_file(path.join("vault.db.migration.bak"));
            }
            Err(e) => {
                drop(conn);
                let backup_path = path.join("vault.db.migration.bak");
                let _ = std::fs::copy(&backup_path, &db_path);
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
    apply_pragmas(&conn);
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
