use rusqlite::Connection;

use super::MigrationError;

pub fn up(conn: &Connection) -> Result<(), MigrationError> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|source| MigrationError::ExecutionFailed {
            version: 1,
            name: "initial_schema".to_string(),
            source,
        })?;

    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS records (
            id               TEXT    PRIMARY KEY,
            credential_type  TEXT    NOT NULL,
            encrypted_data   BLOB    NOT NULL,
            nonce            BLOB    NOT NULL,
            dek_version      INTEGER NOT NULL DEFAULT 1,
            aad              BLOB,
            is_favorite      INTEGER NOT NULL DEFAULT 0,
            expires_at       INTEGER,
            created_at       INTEGER NOT NULL,
            updated_at       INTEGER NOT NULL,
            updated_by       TEXT    NOT NULL,
            version          INTEGER NOT NULL DEFAULT 1,
            deleted          INTEGER NOT NULL DEFAULT 0,
            deleted_at       INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_records_credential_type ON records(credential_type);
        CREATE INDEX IF NOT EXISTS idx_records_updated_at ON records(updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_records_deleted ON records(deleted);
        CREATE INDEX IF NOT EXISTS idx_records_is_favorite ON records(is_favorite) WHERE is_favorite = 1;
        CREATE INDEX IF NOT EXISTS idx_records_expires_at ON records(expires_at) WHERE expires_at IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_records_deleted_at ON records(deleted_at) WHERE deleted = 1;
        CREATE INDEX IF NOT EXISTS idx_records_dek_version ON records(dek_version);

        CREATE TABLE IF NOT EXISTS tags (
            id   INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE CHECK(length(name) <= 50)
        );
        CREATE TABLE IF NOT EXISTS record_tags (
            record_id TEXT NOT NULL,
            tag_id    INTEGER NOT NULL,
            PRIMARY KEY (record_id, tag_id),
            FOREIGN KEY (record_id) REFERENCES records(id) ON DELETE CASCADE,
            FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_record_tags_tag_id ON record_tags(tag_id);

        CREATE TABLE IF NOT EXISTS password_history (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            record_id           TEXT NOT NULL,
            encrypted_password  BLOB NOT NULL,
            nonce               BLOB NOT NULL,
            dek_version         INTEGER NOT NULL DEFAULT 1,
            changed_at          INTEGER NOT NULL,
            FOREIGN KEY (record_id) REFERENCES records(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_password_history_record_id ON password_history(record_id, changed_at DESC);

        CREATE TABLE IF NOT EXISTS audit_log (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            operation   TEXT NOT NULL,
            record_id   TEXT,
            record_name TEXT,
            detail      TEXT,
            occurred_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_audit_log_occurred_at ON audit_log(occurred_at DESC);
        CREATE INDEX IF NOT EXISTS idx_audit_log_operation ON audit_log(operation);
        CREATE INDEX IF NOT EXISTS idx_audit_log_record_id ON audit_log(record_id) WHERE record_id IS NOT NULL;

        CREATE TABLE IF NOT EXISTS sync_state (
            record_id        TEXT PRIMARY KEY,
            cloud_updated_at INTEGER,
            local_updated_at INTEGER NOT NULL,
            sync_status      INTEGER NOT NULL,
            conflict_data    BLOB,
            FOREIGN KEY (record_id) REFERENCES records(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_sync_state_status ON sync_state(sync_status);

        CREATE TABLE IF NOT EXISTS metadata (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS record_health_state (
            record_id             TEXT PRIMARY KEY,
            record_version        INTEGER NOT NULL,
            evaluated_at          INTEGER,
            weak_password         INTEGER,
            duplicate_group_size  INTEGER,
            compromised           INTEGER,
            expired               INTEGER,
            FOREIGN KEY (record_id) REFERENCES records(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_record_health_version
            ON record_health_state(record_version);
        CREATE INDEX IF NOT EXISTS idx_record_health_compromised
            ON record_health_state(compromised) WHERE compromised = 1;
        CREATE INDEX IF NOT EXISTS idx_record_health_expired
            ON record_health_state(expired) WHERE expired = 1;
        CREATE INDEX IF NOT EXISTS idx_record_health_weak
            ON record_health_state(weak_password) WHERE weak_password = 1;",
    )
    .map_err(|source| MigrationError::ExecutionFailed {
        version: 1,
        name: "initial_schema".to_string(),
        source,
    })?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs();
    let vault_id = uuid::Uuid::new_v4().to_string();
    let device_id = uuid::Uuid::new_v4().to_string();

    let seed = |key: &str, value: &str| -> Result<(), MigrationError> {
        tx.execute(
            "INSERT OR IGNORE INTO metadata (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        )
        .map_err(|source| MigrationError::ExecutionFailed {
            version: 1,
            name: "initial_schema".to_string(),
            source,
        })?;
        Ok(())
    };

    seed("schema_version", "1")?;
    seed("vault_id", &vault_id)?;
    seed("device_id", &device_id)?;
    seed("created_at", &now.to_string())?;
    seed("current_dek_version", "1")?;

    tx.commit()
        .map_err(|source| MigrationError::ExecutionFailed {
            version: 1,
            name: "initial_schema".to_string(),
            source,
        })?;

    Ok(())
}
