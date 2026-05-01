// Application-layer search (search_records function)

use crate::types::record::TuiRecord;

/// Filter records by a search query using case-insensitive AND logic.
///
/// The query is split by whitespace into terms. A record matches only when
/// every term is found in either the `name` or `subtitle` field
/// (case-insensitive). An empty or whitespace-only query returns all records.
pub fn search_records(records: &[TuiRecord], query: &str) -> Vec<TuiRecord> {
    if query.trim().is_empty() {
        return records.to_vec();
    }

    let terms: Vec<&str> = query.split_whitespace().collect();

    records
        .iter()
        .filter(|record| {
            let name_lower = record.name.to_lowercase();
            let subtitle_lower = record.subtitle.to_lowercase();
            terms.iter().all(|term| {
                let term_lower = term.to_lowercase();
                name_lower.contains(&term_lower) || subtitle_lower.contains(&term_lower)
            })
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::types::credential::CredentialType;
    use crate::types::record::TuiRecord;

    /// Helper: build a minimal TuiRecord with the given name and subtitle.
    fn make_record(name: &str, subtitle: &str) -> TuiRecord {
        TuiRecord {
            id: Uuid::new_v4(),
            credential_type: CredentialType::Login,
            name: name.to_string(),
            subtitle: subtitle.to_string(),
            is_favorite: false,
            is_expired: false,
            expires_at: None,
            has_weak_password: false,
            is_compromised: false,
            duplicate_group_size: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted: false,
            deleted_at: None,
            tags: vec![],
            sync_status: None,
        }
    }

    // --- search "Test" matches record with name "Test Record" ---

    #[test]
    fn search_matches_record_by_name_substring() {
        let records = vec![
            make_record("Test Record", "alice"),
            make_record("Other", "bob"),
        ];

        let result = search_records(&records, "Test");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "Test Record");
    }

    // --- search "foo bar" matches record containing both "foo" and "bar" (AND logic) ---

    #[test]
    fn search_and_logic_all_terms_must_match() {
        let records = vec![
            make_record("foo bar baz", "subtitle"),
            make_record("foo only", "subtitle"),
            make_record("bar only", "subtitle"),
        ];

        let result = search_records(&records, "foo bar");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "foo bar baz");
    }

    // --- search "TEST" matches record with name "test record" (case insensitive) ---

    #[test]
    fn search_is_case_insensitive() {
        let records = vec![make_record("test record", "some subtitle")];

        let result = search_records(&records, "TEST");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "test record");
    }

    // --- empty search string returns all records ---

    #[test]
    fn search_empty_query_returns_all_records() {
        let records = vec![make_record("Alpha", "a"), make_record("Bravo", "b")];

        let result = search_records(&records, "");
        assert_eq!(result.len(), 2);

        let result_whitespace = search_records(&records, "   ");
        assert_eq!(result_whitespace.len(), 2);
    }

    // --- search matches against subtitle field ---

    #[test]
    fn search_matches_against_subtitle() {
        let records = vec![
            make_record("Site A", "alice@example.com"),
            make_record("Site B", "bob@example.com"),
        ];

        let result = search_records(&records, "alice");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "Site A");
    }

    // --- search with no matching records returns empty ---

    #[test]
    fn search_no_match_returns_empty() {
        let records = vec![make_record("Alpha", "subtitle")];

        let result = search_records(&records, "nonexistent");
        assert!(result.is_empty());
    }

    // --- search term can be split across name and subtitle (AND logic cross-field) ---

    #[test]
    fn search_and_logic_cross_fields() {
        let records = vec![
            make_record("GitHub", "alice@example.com"),
            make_record("GitLab", "bob@example.com"),
        ];

        // "git" matches both names, "alice" matches only first subtitle
        let result = search_records(&records, "git alice");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "GitHub");
    }

    // --- search with empty records returns empty ---

    #[test]
    fn search_empty_records_returns_empty() {
        let records: Vec<TuiRecord> = vec![];
        let result = search_records(&records, "anything");
        assert!(result.is_empty());
    }
}
