use std::path::Path;

use rusqlite::Connection;
use thiserror::Error;

use crate::crypto::db_page_key::DbPageKey;
use crate::db::schema::{self, InitDbError};

#[derive(Debug, Error)]
pub enum VaultDbError {
    #[error("plaintext SQLite vault databases are not supported by SQLCipher production startup")]
    PlaintextDatabaseUnsupported,
    #[error("wrong SQLCipher database page key or unreadable encrypted database")]
    WrongDbPageKey,
    #[error("database is corrupt: {0}")]
    CorruptDatabase(String),
    #[error("unsupported schema version: {current} > {supported}")]
    UnsupportedSchemaVersion { current: u32, supported: u32 },
    #[error("database open I/O error: {0}")]
    DbOpenIo(String),
    #[error("database migration failed: {0}")]
    DbMigrationFailed(String),
    #[error("database rollback failed: {0}")]
    DbRollbackFailed(String),
}

pub struct VaultDbFactory;

impl VaultDbFactory {
    pub fn create_sqlcipher_vault(
        vault_dir: &Path,
        key: &DbPageKey,
    ) -> Result<Connection, VaultDbError> {
        std::fs::create_dir_all(vault_dir).map_err(|e| VaultDbError::DbOpenIo(e.to_string()))?;
        let db_path = vault_dir.join("vault.db");
        let conn = open_keyed_connection(&db_path, key)?;
        migrate_and_validate(conn)
    }

    pub fn open_sqlcipher_vault(
        vault_dir: &Path,
        key: &DbPageKey,
    ) -> Result<Connection, VaultDbError> {
        let db_path = vault_dir.join("vault.db");
        if !db_path.exists() {
            return Err(VaultDbError::DbOpenIo(format!(
                "{} not found",
                db_path.display()
            )));
        }
        // Open once with key; fall back to plaintext check on any error.
        let conn = match open_keyed_connection(&db_path, key) {
            Ok(c) => c,
            Err(e) => {
                return Err(if is_plaintext_sqlite(&db_path) {
                    VaultDbError::PlaintextDatabaseUnsupported
                } else {
                    e
                });
            }
        };
        match migrate_and_validate(conn) {
            Ok(conn) => Ok(conn),
            Err(e) => Err(if is_plaintext_sqlite(&db_path) {
                VaultDbError::PlaintextDatabaseUnsupported
            } else {
                e
            }),
        }
    }
}

fn open_keyed_connection(db_path: &Path, key: &DbPageKey) -> Result<Connection, VaultDbError> {
    let conn = Connection::open(db_path).map_err(|e| VaultDbError::DbOpenIo(e.to_string()))?;
    crate::db::sqlcipher::apply_key(&conn, key).map_err(|e| {
        tracing::warn!(error = %e, "failed to apply SQLCipher key to database");
        VaultDbError::WrongDbPageKey
    })?;
    // Probe the keyed connection to surface wrong-key errors before pragma
    // application. PRAGMA key alone does not validate the key material;
    // the first read through the encryption layer reveals mismatches.
    conn.query_row("SELECT COUNT(*) FROM sqlite_master", [], |_row| Ok(()))
        .map_err(|e| {
            tracing::warn!(error = %e, "SQLCipher key validation probe failed");
            VaultDbError::WrongDbPageKey
        })?;
    schema::apply_pragmas(&conn).map_err(|e| VaultDbError::DbOpenIo(e.to_string()))?;
    Ok(conn)
}

fn migrate_and_validate(conn: Connection) -> Result<Connection, VaultDbError> {
    // Check for unsupported schema version before running migrations,
    // consistent with the non-SQLCipher init_db path in schema.rs.
    let current = crate::db::migrations::read_current_version(&conn);
    if current > crate::db::migrations::SCHEMA_VERSION {
        return Err(VaultDbError::UnsupportedSchemaVersion {
            current,
            supported: crate::db::migrations::SCHEMA_VERSION,
        });
    }

    crate::db::migrations::run_migrations(&conn)
        .map_err(|e| VaultDbError::DbMigrationFailed(e.to_string()))?;
    schema::run_quick_check(&conn).map_err(map_init_error)?;
    schema::run_foreign_key_check(&conn).map_err(map_init_error)?;
    Ok(conn)
}

fn map_init_error(err: InitDbError) -> VaultDbError {
    match err {
        InitDbError::IntegrityCheckFailed(msg) => VaultDbError::CorruptDatabase(msg),
        InitDbError::ForeignKeyCheckFailed(msg) => VaultDbError::CorruptDatabase(msg),
        InitDbError::UnsupportedSchemaVersion { current, supported } => {
            VaultDbError::UnsupportedSchemaVersion { current, supported }
        }
        InitDbError::Migration(e) => VaultDbError::DbMigrationFailed(e.to_string()),
        InitDbError::Sqlite(e) => VaultDbError::DbOpenIo(e.to_string()),
    }
}

fn is_plaintext_sqlite(db_path: &Path) -> bool {
    let Ok(conn) = Connection::open(db_path) else {
        return false;
    };
    let result: rusqlite::Result<i64> =
        conn.query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| row.get(0));
    result.is_ok()
}
