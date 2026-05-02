// Record CRUD operations (create, update, delete, restore, get, list, toggle_favorite)
//
// Split across sub-modules by domain responsibility:
// - crud: create, read, update, delete operations
// - list: listing, filtering, sorting
// - field: field-level record access
// - migration: DEK migration helpers
// - helpers: shared free functions (extract_field, apply_sort, etc.)

mod crud;
mod field;
mod helpers;
mod list;
mod migration;
#[cfg(test)]
mod tests;

// Re-export for external use by sibling modules (audit, history, tag, trash, vault/mod)
pub(crate) use helpers::db_error_to_vault;
