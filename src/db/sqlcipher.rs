use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::crypto::db_page_key::DbPageKey;
use crate::db::migrations;
use crate::db::schema::{apply_pragmas, InitDbError};

pub(crate) fn sqlcipher_raw_key_pragma(key: &DbPageKey) -> String {
    format!("PRAGMA key = \"x'{}'\";", hex::encode(key.expose()))
}

/// Return the SQLCipher hex-encoded key literal for use with `ATTACH KEY` syntax.
/// Only needed by the benchmark. Production code should use [`apply_key`] or
/// [`open_encrypted_connection`] instead.
#[cfg(feature = "sqlcipher")]
pub fn sqlcipher_key_hex(key: &DbPageKey) -> String {
    format!("x'{}'", hex::encode(key.expose()))
}

pub fn apply_key(conn: &Connection, key: &DbPageKey) -> Result<(), rusqlite::Error> {
    conn.execute_batch(&sqlcipher_raw_key_pragma(key))
}

pub fn cipher_version(conn: &Connection) -> Result<String, rusqlite::Error> {
    conn.query_row("PRAGMA cipher_version", [], |row| row.get(0))
}

pub fn open_encrypted_connection(
    db_path: &Path,
    key: &DbPageKey,
) -> Result<Connection, InitDbError> {
    let conn = Connection::open(db_path)?;
    apply_key(&conn, key)?;
    apply_pragmas(&conn)?;
    migrations::run_migrations(&conn)?;
    Ok(conn)
}

pub fn open_encrypted_vault_dir(
    vault_dir: &Path,
    key: &DbPageKey,
) -> Result<Connection, InitDbError> {
    let db_path: PathBuf = vault_dir.join("vault.db");
    open_encrypted_connection(&db_path, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::db_page_key::test_db_page_key;

    #[test]
    fn raw_key_pragma_uses_64_hex_chars() {
        let key = test_db_page_key([0xabu8; 32]);
        let pragma = sqlcipher_raw_key_pragma(&key);
        assert_eq!(
            pragma,
            "PRAGMA key = \"x'abababababababababababababababababababababababababababababababab'\";"
        );
    }
}
