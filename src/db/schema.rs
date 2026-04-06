use rusqlite::Connection;

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

pub fn initialize_schema(conn: &Connection) {
    conn.execute_batch(
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
            name TEXT NOT NULL UNIQUE
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
        );",
    )
    .expect("failed to create schema");
}

pub fn initialize_metadata(conn: &Connection) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let vault_id = uuid::Uuid::new_v4().to_string();
    let device_id = uuid::Uuid::new_v4().to_string();

    let _ = conn.execute(
        "INSERT OR IGNORE INTO metadata (key, value) VALUES (?1, ?2)",
        rusqlite::params!["schema_version", "2"],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO metadata (key, value) VALUES (?1, ?2)",
        rusqlite::params!["vault_id", &vault_id],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO metadata (key, value) VALUES (?1, ?2)",
        rusqlite::params!["device_id", &device_id],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO metadata (key, value) VALUES (?1, ?2)",
        rusqlite::params!["created_at", &now.to_string()],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO metadata (key, value) VALUES (?1, ?2)",
        rusqlite::params!["current_dek_version", "1"],
    );
}

pub fn init_db(path: &std::path::Path) -> Connection {
    let conn = Connection::open(path.join("vault.db")).expect("failed to open vault.db");
    apply_pragmas(&conn);
    initialize_schema(&conn);
    initialize_metadata(&conn);
    conn
}

pub fn init_db_in_memory() -> Connection {
    let conn = Connection::open_in_memory().expect("failed to open in-memory db");
    apply_pragmas(&conn);
    initialize_schema(&conn);
    initialize_metadata(&conn);
    conn
}
