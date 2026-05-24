//! View dispatch — routes rendering to the current screen.

use ratatui::Frame;

use crate::app::App;
use crate::commands::types::Screen;
use crate::tui::state::animation::EffectKind;
use crate::tui::theme;
use crate::tui::traits::screen::Screen as ScreenTrait;

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    // Show "terminal too small" warning if below minimum size.
    if app.state.too_small {
        render_too_small(frame, area);
        return;
    }

    // Route to current screen's view().
    match app.state.current_screen {
        Screen::Main => {
            app.state.screens.main.view(frame, area);
        }
        Screen::Unlock => {
            app.state.screens.unlock.view(frame, area);
        }
        Screen::Onboarding => {
            app.state.screens.onboarding.view(frame, area);
        }
        Screen::KeyRecovery => {
            app.state.screens.key_recovery.view(frame, area);
        }
        Screen::DatabaseRecovery => {
            app.state.screens.database_recovery.view(frame, area);
        }
        Screen::Config => {
            app.state.screens.config.view(frame, area);
        }
        Screen::ChangeMasterPassword => {
            app.state.screens.change_master_password.view(frame, area);
        }
        Screen::SetNewMasterPassword => {
            app.state.screens.set_new_master_password.view(frame, area);
        }
        Screen::ImportExport => {
            app.state.screens.import_export.view(frame, area);
        }
        Screen::AuditLog => {
            app.state.screens.audit_log.view(frame, area);
        }
        Screen::SyncConflict => {
            app.state.screens.sync_conflict.view(frame, area);
        }
        Screen::PasswordGenerator => {
            app.state.screens.password_generator.view(frame, area);
        }
        Screen::CreateRecord => {
            app.state.screens.create_record.view(frame, area);
        }
        Screen::EditRecord { .. } => {
            app.state.screens.edit_record.view(frame, area);
        }
    }

    // Render global notification overlay (on top of screen content).
    if let Some(ref msg) = app.state.shared.notification.current_message {
        crate::tui::components::notification::render_notification(frame, area, msg);
    }

    // Apply active animation effect to the frame buffer.
    let area = app
        .state
        .shared
        .animation
        .active_effect
        .as_ref()
        .map(|active| animation_effect_area(&app.state, frame.area(), active.kind))
        .unwrap_or_else(|| frame.area());
    if let Some(active) = app.state.shared.animation.active_effect.as_mut() {
        prepare_animation_area(frame.buffer_mut(), area, active.kind);
        active.effect.process(
            std::time::Duration::from_millis(50).into(),
            frame.buffer_mut(),
            area,
        );
    }
    app.state.shared.animation.clear_finished();
}

fn prepare_animation_area(
    buffer: &mut ratatui::buffer::Buffer,
    area: ratatui::layout::Rect,
    kind: EffectKind,
) {
    if !matches!(
        kind,
        EffectKind::OnboardingForward | EffectKind::OnboardingBack
    ) {
        return;
    }

    let edge_color = crate::tui::animation::effects::onboarding_slide_edge_color();
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.bg = edge_color;
            }
        }
    }
}

fn animation_effect_area(
    state: &crate::tui::state::AppState,
    frame_area: ratatui::layout::Rect,
    kind: EffectKind,
) -> ratatui::layout::Rect {
    match kind {
        EffectKind::OnboardingIntro
        | EffectKind::OnboardingForward
        | EffectKind::OnboardingBack => onboarding_animation_area(state, frame_area),
        _ => frame_area,
    }
}

fn onboarding_animation_area(
    state: &crate::tui::state::AppState,
    frame_area: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
    use crate::tui::screens::onboarding::views_setup::header_rows;
    use crate::tui::screens::onboarding::{OnboardingScreen, OnboardingStep};
    use crate::tui::terminal::WidthTier;

    if !matches!(state.current_screen, Screen::Onboarding) {
        return centered_animation_area(frame_area, 24, 72);
    }

    let wide = WidthTier::from_width(frame_area.width) != WidthTier::TooSmall;
    let hdr = header_rows(wide);
    let (height, width) = match &state.screens.onboarding.current_step {
        OnboardingStep::Welcome => (hdr + 18, 60),
        OnboardingStep::RecoveryDisplay => {
            let learn_extra = if state.screens.onboarding.learn_more_expanded {
                5
            } else {
                0
            };
            (hdr + 19 + learn_extra, 72)
        }
        OnboardingStep::RecoveryVerify { .. } => (hdr + 20, 60),
        OnboardingStep::RecoveryInput => (hdr + 16, 72),
        OnboardingStep::SecurityAdvisory => (hdr + 10, 60),
        OnboardingStep::ImportSource => (hdr + 20, 60),
        OnboardingStep::ImportPreview => (hdr + 18, 60),
        OnboardingStep::SetPassword => (hdr + 7, 60),
    };

    OnboardingScreen::centered_content(frame_area, height, width)
}

fn centered_animation_area(
    area: ratatui::layout::Rect,
    height: u16,
    width: u16,
) -> ratatui::layout::Rect {
    use ratatui::layout::{Constraint, Layout};

    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height.min(area.height)),
        Constraint::Fill(1),
    ])
    .split(area);
    let horizontal = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(width.min(area.width)),
        Constraint::Fill(1),
    ])
    .split(vertical[1]);

    horizontal[1]
}

fn render_too_small(frame: &mut Frame, area: ratatui::layout::Rect) {
    use ratatui::layout::Alignment;
    use ratatui::widgets::{Block, Borders, Paragraph};

    let (w, h) = (area.width, area.height);
    let text = format!("Terminal too small: {}x{}\nMinimum required: 80x24", w, h);
    let paragraph = Paragraph::new(text)
        .style(theme::Styles::warning_text())
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme::Styles::error_border()),
        );
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn onboarding_transition_area_is_limited_to_content_region() {
        let mut state = crate::tui::state::AppState {
            current_screen: Screen::Onboarding,
            ..crate::tui::state::AppState::default()
        };
        state.screens.onboarding.current_step =
            crate::tui::screens::onboarding::OnboardingStep::Welcome;
        let frame_area = Rect::new(0, 0, 120, 40);

        let area = animation_effect_area(&state, frame_area, EffectKind::OnboardingForward);

        assert!(area.x > frame_area.x);
        assert!(area.y > frame_area.y);
        assert!(area.width < frame_area.width);
        assert!(area.height < frame_area.height);
        assert_eq!(area.width, 60);
    }

    #[test]
    fn non_onboarding_transition_area_remains_full_frame() {
        let state = crate::tui::state::AppState {
            current_screen: Screen::Main,
            ..crate::tui::state::AppState::default()
        };
        let frame_area = Rect::new(0, 0, 120, 40);

        let area = animation_effect_area(&state, frame_area, EffectKind::ScreenIn);

        assert_eq!(area, frame_area);
    }

    #[test]
    fn onboarding_slide_prepares_muted_edge_background() {
        let area = Rect::new(1, 1, 3, 2);
        let mut buffer = ratatui::buffer::Buffer::empty(Rect::new(0, 0, 6, 5));

        prepare_animation_area(&mut buffer, area, EffectKind::OnboardingForward);

        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                assert_eq!(
                    buffer.cell((x, y)).expect("cell").bg,
                    crate::tui::theme::BG_SURFACE
                );
            }
        }
        assert_ne!(
            buffer.cell((0, 0)).expect("outside cell").bg,
            crate::tui::theme::BG_SURFACE
        );
    }

    #[test]
    fn non_onboarding_slide_effect_does_not_prepare_background() {
        let area = Rect::new(1, 1, 3, 2);
        let mut buffer = ratatui::buffer::Buffer::empty(Rect::new(0, 0, 6, 5));

        prepare_animation_area(&mut buffer, area, EffectKind::ScreenIn);

        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                assert_ne!(
                    buffer.cell((x, y)).expect("cell").bg,
                    crate::tui::theme::BG_SURFACE
                );
            }
        }
    }
}
