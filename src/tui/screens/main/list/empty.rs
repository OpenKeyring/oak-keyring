use ratatui::layout::Rect;
use ratatui::Frame;

use crate::commands::types::RecordFilter;
use crate::tui::components::empty_state::{EmptyStateVariant, EmptyStateWidget};
use crate::tui::state::list_state::{ListMode, ListPanelState};

/// Render empty state when there are no records.
pub(super) fn render_empty_state(
    frame: &mut Frame,
    area: Rect,
    state: &ListPanelState,
    unicode: bool,
    filter: &RecordFilter,
) {
    let variant = build_empty_state_variant(state, filter);
    EmptyStateWidget::view(frame, area, &variant, unicode);
}

/// Build the appropriate empty state variant based on list mode and filter.
pub(super) fn build_empty_state_variant(
    state: &ListPanelState,
    filter: &RecordFilter,
) -> EmptyStateVariant {
    match &state.mode {
        ListMode::Search(search_state) if !search_state.query.is_empty() => {
            EmptyStateVariant::NoSearchResults {
                query: search_state.query.clone(),
            }
        }
        _ => match filter {
            RecordFilter::All => EmptyStateVariant::NoPasswords,
            RecordFilter::Favorites => EmptyStateVariant::NoFavorites,
            RecordFilter::Expired => EmptyStateVariant::NoExpired,
            RecordFilter::HealthIssues => EmptyStateVariant::NoHealthIssues,
            RecordFilter::Trash => EmptyStateVariant::EmptyTrash,
            RecordFilter::Tag(name) => EmptyStateVariant::EmptyTag {
                tag_name: name.clone(),
            },
            RecordFilter::Search(q) => EmptyStateVariant::NoSearchResults { query: q.clone() },
        },
    }
}
