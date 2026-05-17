use std::ffi::OsString;
use std::path::{Path, PathBuf};
#[cfg(feature = "sqlcipher-poc")]
use std::time::Duration;

#[cfg(feature = "sqlcipher-poc")]
use rusqlite::backup::Backup;
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

fn run_quick_check(conn: &Connection) -> Result<(), InitDbError> {
    let result: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if result.eq_ignore_ascii_case("ok") {
        Ok(())
    } else {
        Err(InitDbError::IntegrityCheckFailed(result))
    }
}

fn run_foreign_key_check(conn: &Connection) -> Result<(), InitDbError> {
    let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
    let mut rows = stmt.query([])?;
    if let Some(row) = rows.next()? {
        let table: String = row.get(0)?;
        let rowid: i64 = row.get(1)?;
        let parent: String = row.get(2)?;
        let fk_index: i64 = row.get(3)?;
        Err(InitDbError::ForeignKeyCheckFailed(format!(
            "table={table} rowid={rowid} parent={parent} fk_index={fk_index}"
        )))
    } else {
        Ok(())
    }
}

pub fn checkpoint_wal(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);")
}

pub fn init_db(path: &Path) -> Result<Connection, InitDbError> {
    let db_path = path.join("vault.db");
    let existed_before_open = db_path.exists();
    let conn = Connection::open(&db_path)?;
    apply_pragmas(&conn)?;

    if existed_before_open {
        run_quick_check(&conn)?;
    }

    let current = migrations::read_current_version(&conn);
    let needs_migration = current < migrations::SCHEMA_VERSION;

    if current > migrations::SCHEMA_VERSION {
        return Err(InitDbError::UnsupportedSchemaVersion {
            current,
            supported: migrations::SCHEMA_VERSION,
        });
    }

    let conn = if needs_migration && current > 0 {
        let backup_path = path.join("vault.db.migration.bak");
        run_with_backup(conn, &db_path, &backup_path, |c| {
            migrations::run_migrations(c)
        })?
    } else {
        if needs_migration {
            migrations::run_migrations(&conn)?;
        }
        conn
    };

    run_foreign_key_check(&conn)?;
    Ok(conn)
}

pub fn init_db_in_memory() -> Connection {
    let conn = Connection::open_in_memory().expect("failed to open in-memory db");
    apply_pragmas(&conn).expect("failed to apply pragmas");
    migrations::run_migrations(&conn).expect("migration failed");
    conn
}

/// Runs `work` against `conn`, snapshotting the database to `backup_path` first
/// and restoring on failure.
///
/// On success: the backup is removed.
/// On failure: the connection is closed, any stale WAL/SHM sidecars are deleted,
/// and the backup is copied over the main file. The backup is retained for forensics.
///
/// Exposed for testing — callers in production should go through `init_db`.
pub fn run_with_backup<F>(
    conn: Connection,
    db_path: &Path,
    backup_path: &Path,
    work: F,
) -> Result<Connection, InitDbError>
where
    F: FnOnce(&Connection) -> Result<(), MigrationError>,
{
    backup_database(&conn, backup_path)?;
    run_with_existing_backup(conn, db_path, backup_path, work)
}

/// SQLCipher PoC-only variant of `run_with_backup`.
///
/// `key` must match the already-open source connection. The destination backup
/// connection is keyed before `Backup::new` so the retained backup file is also
/// SQLCipher-encrypted.
#[cfg(feature = "sqlcipher-poc")]
pub fn run_with_encrypted_backup<F>(
    conn: Connection,
    db_path: &Path,
    backup_path: &Path,
    key: &[u8; 32],
    work: F,
) -> Result<Connection, InitDbError>
where
    F: FnOnce(&Connection) -> Result<(), MigrationError>,
{
    remove_backup_files(backup_path)?;
    backup_encrypted_database(&conn, backup_path, key)?;
    run_with_existing_backup(conn, db_path, backup_path, work)
}

fn run_with_existing_backup<F>(
    conn: Connection,
    db_path: &Path,
    backup_path: &Path,
    work: F,
) -> Result<Connection, InitDbError>
where
    F: FnOnce(&Connection) -> Result<(), MigrationError>,
{
    match work(&conn) {
        Ok(()) => {
            if let Err(e) = std::fs::remove_file(backup_path) {
                tracing::warn!(error = %e, "failed to remove migration backup");
            }
            Ok(conn)
        }
        Err(work_err) => {
            drop(conn);
            if let Err(restore_err) = restore_database(backup_path, db_path) {
                tracing::error!(
                    migration_error = %work_err,
                    restore_error = %restore_err,
                    "migration failed AND backup restore failed"
                );
                return Err(MigrationError::RestoreFailed(restore_err).into());
            }
            Err(work_err.into())
        }
    }
}

/// Snapshots `src` to `dst_path` using SQLite's online backup API.
///
/// The online backup API reads through the WAL, so the destination is a
/// transaction-consistent copy of the live database — including commits that
/// have not yet been checkpointed into the main file.
fn backup_database(src: &Connection, dst_path: &Path) -> Result<(), MigrationError> {
    // Connection::backup opens the destination as a SQLite database. If a stale
    // .bak from a prior run exists, opening it would merge with that file's pages
    // rather than producing a fresh snapshot.
    remove_backup_files(dst_path)?;

    src.backup(rusqlite::DatabaseName::Main, dst_path, None)
        .map_err(|e| MigrationError::BackupFailed(std::io::Error::other(e)))
}

fn remove_backup_files(dst_path: &Path) -> Result<(), MigrationError> {
    for sidecar in [None, Some("-wal"), Some("-shm")] {
        let p = sidecar.map_or_else(|| dst_path.to_path_buf(), |s| with_suffix(dst_path, s));
        if p.exists() {
            std::fs::remove_file(&p).map_err(MigrationError::BackupFailed)?;
        }
    }
    Ok(())
}

#[cfg(feature = "sqlcipher-poc")]
fn backup_encrypted_database(
    src: &Connection,
    dst_path: &Path,
    key: &[u8; 32],
) -> Result<(), MigrationError> {
    let mut dst = Connection::open(dst_path)
        .map_err(|e| MigrationError::BackupFailed(std::io::Error::other(e)))?;
    crate::db::sqlcipher::apply_key(&dst, key)
        .map_err(|e| MigrationError::BackupFailed(std::io::Error::other(e)))?;

    let backup = Backup::new(src, &mut dst)
        .map_err(|e| MigrationError::BackupFailed(std::io::Error::other(e)))?;
    backup
        .run_to_completion(100, Duration::from_millis(0), None)
        .map_err(|e| MigrationError::BackupFailed(std::io::Error::other(e)))
}

/// Restores `db_path` from `backup_path`.
///
/// Removes any stale WAL/SHM sidecars on the live database before copying so
/// SQLite does not replay a partial post-failure WAL onto the restored file.
fn restore_database(backup_path: &Path, db_path: &Path) -> std::io::Result<()> {
    for suffix in ["-wal", "-shm"] {
        let p = with_suffix(db_path, suffix);
        if p.exists() {
            std::fs::remove_file(&p)?;
        }
    }
    std::fs::copy(backup_path, db_path)?;
    Ok(())
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s: OsString = path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

#[derive(Debug, thiserror::Error)]
pub enum InitDbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("migration error: {0}")]
    Migration(#[from] MigrationError),
    #[error("database integrity check failed: {0}")]
    IntegrityCheckFailed(String),
    #[error("database foreign key check failed: {0}")]
    ForeignKeyCheckFailed(String),
    #[error("database schema version {current} is newer than this build supports ({supported})")]
    UnsupportedSchemaVersion { current: u32, supported: u32 },
}
