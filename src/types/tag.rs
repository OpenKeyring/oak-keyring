#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Tag {
    pub id: i64,
    pub name: String,
}

/// Sort metadata for a tag — used by the sidebar for Frequency and RecentlyUsed sorting.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TagSortMeta {
    /// Number of non-deleted records associated with this tag.
    pub record_count: usize,
    /// Timestamp of the most recently updated record with this tag (Unix epoch, 0 if none).
    pub last_used_at: i64,
}
