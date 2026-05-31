use rusqlite::Connection;

use super::MigrationError;

pub fn up(conn: &Connection) -> Result<(), MigrationError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS record_list_index (
            record_id     TEXT PRIMARY KEY,
            name          TEXT NOT NULL,
            subtitle      TEXT NOT NULL,
            search_text   TEXT NOT NULL,
            name_sort_key TEXT NOT NULL,
            FOREIGN KEY (record_id) REFERENCES records(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_record_list_index_name_sort
            ON record_list_index(name_sort_key);
        CREATE INDEX IF NOT EXISTS idx_record_list_index_search
            ON record_list_index(search_text);",
    )
    .map_err(|source| MigrationError::ExecutionFailed {
        version: 2,
        name: "record_list_index".to_string(),
        source,
    })?;

    Ok(())
}
