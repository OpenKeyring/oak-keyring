use super::*;

use chrono::Utc;
use uuid::Uuid;

use crate::commands::types::{SortDirection, SortField};
use crate::types::credential::CredentialType;
use crate::types::record::TuiRecord;

fn assert_one_of(actual: &str, expected: &[&str]) {
    assert!(
        expected.contains(&actual),
        "expected one of {expected:?}, got {actual:?}"
    );
}

/// Helper to build a TuiRecord with minimal fields for testing.
fn make_record(id: Uuid, name: &str, subtitle: &str) -> TuiRecord {
    TuiRecord {
        id,
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
        tags: Vec::new(),
        sync_status: None,
    }
}

#[allow(dead_code)]
fn make_record_with_type(id: Uuid, name: &str, cred_type: CredentialType) -> TuiRecord {
    TuiRecord {
        id,
        credential_type: cred_type,
        name: name.to_string(),
        subtitle: String::new(),
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
        tags: Vec::new(),
        sync_status: None,
    }
}

#[test]
fn list_state_default_empty() {
    let state = ListPanelState::default();
    assert!(state.records.is_empty());
    assert_eq!(state.selected_index, None);
    assert_eq!(state.scroll_offset, 0);
    assert!(matches!(state.mode, ListMode::Normal));
    assert_eq!(state.sort.field, SortField::CreatedAt);
    assert_eq!(state.sort.direction, SortDirection::Desc);
    assert_eq!(state.total_count, 0);
    assert_eq!(state._visible_height, 0);
}

#[test]
fn list_state_with_records_selects_first() {
    let r1 = make_record(Uuid::new_v4(), "Alpha", "");
    let r2 = make_record(Uuid::new_v4(), "Beta", "");
    let state = ListPanelState::with_records(vec![r1.clone(), r2]);
    assert_eq!(state.records.len(), 2);
    assert_eq!(state.selected_index, Some(0));
    assert_eq!(state.total_count, 2);
    assert_eq!(state.selected_record().unwrap().name, "Alpha");
}

#[test]
fn list_state_move_down() {
    let r1 = make_record(Uuid::new_v4(), "A", "");
    let r2 = make_record(Uuid::new_v4(), "B", "");
    let r3 = make_record(Uuid::new_v4(), "C", "");
    let mut state = ListPanelState::with_records(vec![r1, r2, r3]);

    assert_eq!(state.selected_index, Some(0));
    state.move_down();
    assert_eq!(state.selected_index, Some(1));
    state.move_down();
    assert_eq!(state.selected_index, Some(2));
    // Clamp at last record
    state.move_down();
    assert_eq!(state.selected_index, Some(2));
}

#[test]
fn list_state_move_up() {
    let r1 = make_record(Uuid::new_v4(), "A", "");
    let r2 = make_record(Uuid::new_v4(), "B", "");
    let mut state = ListPanelState::with_records(vec![r1, r2]);

    state.selected_index = Some(1);
    state.move_up();
    assert_eq!(state.selected_index, Some(0));
    // Clamp at first record
    state.move_up();
    assert_eq!(state.selected_index, Some(0));
}

#[test]
fn sort_toggle_direction() {
    let mut state = ListPanelState::default();
    assert_eq!(state.sort.direction, SortDirection::Desc);
    state.toggle_sort_direction();
    assert_eq!(state.sort.direction, SortDirection::Asc);
    state.toggle_sort_direction();
    assert_eq!(state.sort.direction, SortDirection::Desc);
}

#[test]
fn sort_cycle_field() {
    let mut state = ListPanelState::default();
    assert_eq!(state.sort.field, SortField::CreatedAt);

    state.cycle_sort_field();
    assert_eq!(state.sort.field, SortField::UpdatedAt);

    state.cycle_sort_field();
    assert_eq!(state.sort.field, SortField::Name);

    state.cycle_sort_field();
    assert_eq!(state.sort.field, SortField::UsageFrequency);

    // Full cycle back to CreatedAt
    state.cycle_sort_field();
    assert_eq!(state.sort.field, SortField::CreatedAt);
}

#[test]
fn search_mode_enter_exit() {
    let mut state = ListPanelState::default();
    assert!(!state.is_searching());

    state.enter_search();
    assert!(state.is_searching());
    assert!(!state.is_visual());

    state.commit_search();
    assert!(!state.is_searching());
}

#[test]
fn visual_mode_toggle_select() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let r1 = make_record(id1, "A", "");
    let r2 = make_record(id2, "B", "");
    let mut state = ListPanelState::with_records(vec![r1, r2]);

    state.enter_visual();
    assert!(state.is_visual());
    assert!(state.visual_selected_ids().is_empty());

    // Toggle selects id1 and auto-advances to index 1
    state.toggle_select_current();
    assert_eq!(state.selected_index, Some(1));
    assert!(state.visual_selected_ids().contains(&id1));
    assert!(!state.visual_selected_ids().contains(&id2));

    // Toggle again selects id2 and clamps at last index
    state.toggle_select_current();
    assert!(state.visual_selected_ids().contains(&id2));
    assert_eq!(state.selected_index, Some(1)); // clamped at last

    // Toggle id2 off
    state.toggle_select_current();
    assert!(!state.visual_selected_ids().contains(&id2));
}

#[test]
fn visual_select_all_deselect_all() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let r1 = make_record(id1, "A", "");
    let r2 = make_record(id2, "B", "");
    let mut state = ListPanelState::with_records(vec![r1, r2]);

    state.enter_visual();
    state.select_all();
    let ids = state.visual_selected_ids();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&id1));
    assert!(ids.contains(&id2));

    state.deselect_all();
    assert!(state.visual_selected_ids().is_empty());
}

#[test]
fn search_filter_fuzzy_match() {
    let r1 = make_record(Uuid::new_v4(), "GitHub Login", "user@github.com");
    let r2 = make_record(Uuid::new_v4(), "AWS Credentials", "admin@aws.com");
    let r3 = make_record(Uuid::new_v4(), "GitLab Token", "dev@gitlab.com");
    let all = vec![r1, r2, r3];

    let mut state = ListPanelState::default();
    state.enter_search();
    state.update_search_query("git".to_string());

    let filtered = state.apply_search_filter(all);
    assert_eq!(filtered.len(), 2); // GitHub and GitLab
    assert!(filtered.iter().any(|r| r.name == "GitHub Login"));
    assert!(filtered.iter().any(|r| r.name == "GitLab Token"));
}

#[test]
fn search_filter_multi_term_and_logic() {
    let r1 = make_record(Uuid::new_v4(), "GitHub Personal Token", "dev@github.com");
    let r2 = make_record(Uuid::new_v4(), "GitHub Login", "user@github.com");
    let r3 = make_record(Uuid::new_v4(), "GitLab Token", "dev@gitlab.com");
    let all = vec![r1, r2, r3];

    let mut state = ListPanelState::default();
    state.enter_search();
    // Both "github" AND "token" must match
    state.update_search_query("github token".to_string());

    let filtered = state.apply_search_filter(all);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "GitHub Personal Token");
}

#[test]
fn batch_cleanup_removes_records() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();
    let r1 = make_record(id1, "A", "");
    let r2 = make_record(id2, "B", "");
    let r3 = make_record(id3, "C", "");
    let mut state = ListPanelState::with_records(vec![r1, r2, r3]);
    assert_eq!(state.records.len(), 3);

    // Select the second record, then remove the first two
    state.selected_index = Some(1);
    state.enter_visual();
    state.cleanup_after_batch(&[id1, id2]);

    // After cleanup: only r3 remains, mode is back to Normal
    assert_eq!(state.records.len(), 1);
    assert_eq!(state.records[0].id, id3);
    assert!(!state.is_visual());
    // Selection should be clamped to the remaining last item
    assert_eq!(state.selected_index, Some(0));
    assert_eq!(state.total_count, 1);
}

#[test]
fn format_type_prefix_variants() {
    assert_eq!(format_type_prefix(&CredentialType::Login), "");
    assert_eq!(format_type_prefix(&CredentialType::Api), "[API] ");
    assert_eq!(format_type_prefix(&CredentialType::Ssh), "[SSH] ");
    assert_eq!(format_type_prefix(&CredentialType::SecureNote), "[N] ");
}

#[test]
fn format_relative_time_today() {
    let now = Utc::now();
    let result = format_relative_time(&now);
    // Should be HH:MM format
    let parts: Vec<&str> = result.split(':').collect();
    assert_eq!(parts.len(), 2);
    let hour: u32 = parts[0].parse().unwrap();
    let min: u32 = parts[1].parse().unwrap();
    assert!(hour < 24);
    assert!(min < 60);
}

#[test]
fn format_relative_time_yesterday() {
    let yesterday = Utc::now() - chrono::Duration::try_days(1).unwrap();
    let result = format_relative_time(&yesterday);
    assert_one_of(&result, &["yesterday", "昨天"]);
}

#[test]
fn format_relative_time_days_ago() {
    let dt = Utc::now() - chrono::Duration::try_days(3).unwrap();
    let result = format_relative_time(&dt);
    assert_one_of(&result, &["3 days ago", "3天前"]);
}

#[test]
fn format_relative_time_weeks_ago() {
    let dt = Utc::now() - chrono::Duration::try_days(14).unwrap();
    let result = format_relative_time(&dt);
    assert_one_of(&result, &["2 weeks ago", "2周前"]);
}

#[test]
fn update_search_query_sets_cursor() {
    let mut state = ListPanelState::default();
    state.enter_search();
    state.update_search_query("hello world".to_string());

    if let ListMode::Search(ref s) = state.mode {
        assert_eq!(s.query, "hello world");
        assert_eq!(s.cursor, 11); // len of "hello world"
    } else {
        panic!("Expected search mode");
    }
}

#[test]
fn adjust_scroll_keeps_selected_visible() {
    let records: Vec<TuiRecord> = (0..20)
        .map(|i| make_record(Uuid::new_v4(), &format!("R{}", i), ""))
        .collect();
    let mut state = ListPanelState::with_records(records);
    state._visible_height = 5; // only 5 rows visible

    // Jump to near the end
    state.selected_index = Some(18);
    state.adjust_scroll();
    // scroll_offset should be 18 - 5 + 1 = 14
    assert_eq!(state.scroll_offset, 14);

    // Jump back to top
    state.selected_index = Some(0);
    state.adjust_scroll();
    assert_eq!(state.scroll_offset, 0);
}

// ── Trash time helper tests ────────────────────────────────────────────

#[test]
fn format_days_since_deletion_today() {
    let now = Utc::now();
    let result = format_days_since_deletion(&now);
    assert_one_of(&result, &["Deleted 0 days ago", "0 天前删除"]);
}

#[test]
fn format_days_since_deletion_yesterday() {
    let yesterday = Utc::now() - chrono::Duration::try_days(1).unwrap();
    let result = format_days_since_deletion(&yesterday);
    assert_one_of(&result, &["Deleted 1 days ago", "1 天前删除"]);
}

#[test]
fn format_days_since_deletion_week() {
    let dt = Utc::now() - chrono::Duration::try_days(7).unwrap();
    let result = format_days_since_deletion(&dt);
    assert_one_of(&result, &["Deleted 7 days ago", "7 天前删除"]);
}

#[test]
fn format_remaining_days_normal() {
    let deleted_at = Utc::now() - chrono::Duration::try_days(10).unwrap();
    let retention_days = 30;
    let result = format_remaining_days(&deleted_at, retention_days);
    assert_one_of(&result, &["20 days remaining", "剩余 20 天"]);
}

#[test]
fn format_remaining_days_never_delete() {
    let deleted_at = Utc::now() - chrono::Duration::try_days(10).unwrap();
    let result = format_remaining_days(&deleted_at, 0);
    assert_one_of(&result, &["Will not auto-delete", "不会自动删除"]);
}

#[test]
fn format_remaining_days_expired() {
    let deleted_at = Utc::now() - chrono::Duration::try_days(31).unwrap();
    let retention_days = 30;
    let result = format_remaining_days(&deleted_at, retention_days);
    assert_one_of(&result, &["0 days remaining", "剩余 0 天"]);
}

#[test]
fn trash_warning_tier_safe() {
    let tier = trash_warning_tier(20);
    assert_eq!(tier, TrashWarningTier::Safe);
}

#[test]
fn trash_warning_tier_moderate() {
    let tier = trash_warning_tier(10);
    assert_eq!(tier, TrashWarningTier::Moderate);
}

#[test]
fn trash_warning_tier_urgent() {
    let tier = trash_warning_tier(5);
    assert_eq!(tier, TrashWarningTier::Urgent);
}

#[test]
fn trash_warning_tier_critical() {
    let tier = trash_warning_tier(2);
    assert_eq!(tier, TrashWarningTier::Critical);
}

#[test]
fn trash_warning_tier_zero() {
    let tier = trash_warning_tier(0);
    assert_eq!(tier, TrashWarningTier::Critical);
}

// ── Post-batch cleanup tests ─────────────────────────────────────────

#[test]
fn cleanup_after_batch_exits_visual_and_positions_cursor() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();
    let r1 = make_record(id1, "A", "");
    let r2 = make_record(id2, "B", "");
    let r3 = make_record(id3, "C", "");
    let mut state = ListPanelState::with_records(vec![r1, r2, r3]);

    // Enter visual, select id1 and id2, cursor at index 1
    state.enter_visual();
    if let ListMode::Visual(ref mut vs) = state.mode {
        vs.selected_ids.insert(id1);
        vs.selected_ids.insert(id2);
    }
    state.selected_index = Some(1);

    // Batch delete id1 and id2
    state.cleanup_after_batch(&[id1, id2]);

    // Should exit visual mode
    assert!(!state.is_visual());
    // Only r3 remains
    assert_eq!(state.records.len(), 1);
    // Cursor should be clamped to valid index
    assert!(state.selected_index.unwrap() < state.records.len());
    // Total count updated
    assert_eq!(state.total_count, 1);
}

#[test]
fn cleanup_preserves_cursor_if_record_remains() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();
    let r1 = make_record(id1, "A", "");
    let r2 = make_record(id2, "B", "");
    let r3 = make_record(id3, "C", "");
    let mut state = ListPanelState::with_records(vec![r1, r2, r3]);

    // Cursor at r3 (index 2), delete r1 only
    state.selected_index = Some(2);
    state.enter_visual();
    if let ListMode::Visual(ref mut vs) = state.mode {
        vs.selected_ids.insert(id1);
    }
    state.cleanup_after_batch(&[id1]);

    // r3 (index 1 after removal) should still be selected
    assert_eq!(state.records.len(), 2);
    assert!(state.selected_index.unwrap() < state.records.len());
}

#[test]
fn cleanup_empty_list_sets_none_selection() {
    let id1 = Uuid::new_v4();
    let r1 = make_record(id1, "A", "");
    let mut state = ListPanelState::with_records(vec![r1]);

    state.enter_visual();
    state.cleanup_after_batch(&[id1]);

    assert!(state.records.is_empty());
    assert_eq!(state.selected_index, None);
    assert!(!state.is_visual());
}
