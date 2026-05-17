use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::db::migrations;
use crate::db::schema::{apply_pragmas, InitDbError};

pub fn sqlcipher_raw_key_pragma(key: &[u8; 32]) -> String {
    format!("PRAGMA key = \"x'{}'\";", hex::encode(key))
}

pub fn apply_key(conn: &Connection, key: &[u8; 32]) -> Result<(), rusqlite::Error> {
    conn.execute_batch(&sqlcipher_raw_key_pragma(key))
}

pub fn cipher_version(conn: &Connection) -> Result<String, rusqlite::Error> {
    conn.query_row("PRAGMA cipher_version", [], |row| row.get(0))
}

pub fn open_encrypted_connection(
    db_path: &Path,
    key: &[u8; 32],
) -> Result<Connection, InitDbError> {
    let conn = Connection::open(db_path)?;
    apply_key(&conn, key)?;
    apply_pragmas(&conn)?;
    migrations::run_migrations(&conn)?;
    Ok(conn)
}

pub fn open_encrypted_vault_dir(
    vault_dir: &Path,
    key: &[u8; 32],
) -> Result<Connection, InitDbError> {
    let db_path: PathBuf = vault_dir.join("vault.db");
    open_encrypted_connection(&db_path, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_key_pragma_uses_64_hex_chars() {
        let key = [0xabu8; 32];
        let pragma = sqlcipher_raw_key_pragma(&key);
        assert_eq!(
            pragma,
            "PRAGMA key = \"x'abababababababababababababababababababababababababababababababab'\";"
        );
    }
}
