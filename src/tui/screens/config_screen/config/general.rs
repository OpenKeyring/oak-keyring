use ratatui::layout::Constraint;
use ratatui::{layout::Rect, Frame};

use crate::config::AnimationMode;
use crate::t;
use crate::tui::state::config_state::GeneralConfigForm;
use crate::tui::theme;

pub fn render(frame: &mut Frame, area: Rect, form: &GeneralConfigForm, focused: usize) {
    let chunks = super::render::vertical_chunks(
        area,
        &[
            Constraint::Length(2), // Title
            Constraint::Length(1), // Language
            Constraint::Length(1), // Auto lock
            Constraint::Length(1), // Clipboard
            Constraint::Length(1), // Trash
            Constraint::Length(1), // Animation
            Constraint::Length(2), // Spacer
            Constraint::Length(3), // Import/export buttons
            Constraint::Min(0),
        ],
    );

    super::render::render_section_title(frame, chunks[0], t!("tui.config.tab_general").as_ref());
    super::render::render_setting_row(
        frame,
        chunks[1],
        theme::NF_GLOBE,
        t!("tui.config.language").as_ref(),
        &super::render::dropdown_control(&form.language),
        focused == 0,
        true,
    );
    super::render::render_setting_row(
        frame,
        chunks[2],
        theme::NF_CLOCK,
        t!("tui.config.auto_lock").as_ref(),
        &super::render::dropdown_control(&t!("tui.config.seconds", n = form.auto_lock_seconds)),
        focused == 1,
        true,
    );
    super::render::render_setting_row(
        frame,
        chunks[3],
        theme::NF_CLIPBOARD,
        t!("tui.config.clipboard_clear").as_ref(),
        &super::render::dropdown_control(&t!(
            "tui.config.seconds",
            n = form.clipboard_clear_seconds
        )),
        focused == 2,
        true,
    );
    super::render::render_setting_row(
        frame,
        chunks[4],
        theme::NF_TRASH,
        t!("tui.config.trash_retention").as_ref(),
        &super::render::dropdown_control(&t!("tui.config.days", n = form.trash_retention_days)),
        focused == 3,
        true,
    );

    let anim_label = match form.animation {
        AnimationMode::Auto => t!("tui.config.animation_auto").to_string(),
        AnimationMode::On => t!("tui.config.animation_on").to_string(),
        AnimationMode::Off => t!("tui.config.animation_off").to_string(),
    };
    super::render::render_setting_row(
        frame,
        chunks[5],
        theme::NF_SPARKLES,
        t!("tui.config.animation").as_ref(),
        &super::render::dropdown_control(&anim_label),
        focused == 4,
        true,
    );

    super::render::render_button_row(
        frame,
        chunks[7],
        (
            theme::NF_UPLOAD,
            t!("tui.config.import_button").as_ref(),
            focused == 5,
        ),
        (
            theme::NF_DOWNLOAD,
            t!("tui.config.export_button").as_ref(),
            focused == 6,
        ),
    );
}
