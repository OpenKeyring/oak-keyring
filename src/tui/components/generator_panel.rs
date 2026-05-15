//! Shared generator panel component for standalone dialog and embedded panel.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::t;
use crate::tui::components::length_slider;
use crate::tui::components::strength_bar;
use crate::tui::state::generator_state::{GenerationStyle, GeneratorFocus, GeneratorState};
use crate::tui::theme;

/// Render the generator panel content (used by both standalone and embedded).
/// Returns a vector of Lines to be composed into a Paragraph.
pub fn render_generator_panel(
    state: &GeneratorState,
    is_embedded: bool,
    width: u16,
    unicode: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Style selector (standalone only)
    if !is_embedded {
        lines.push(render_style_selector(state));
        lines.push(separator_line(width));
        lines.push(Line::raw(""));
    }

    // Length slider
    let (label, value, min, max) = match state.style {
        GenerationStyle::Random => (
            t!("tui.generator.length").to_string(),
            state.random_config.length,
            8,
            128,
        ),
        GenerationStyle::Memorable => (
            t!("tui.generator.word_count").to_string(),
            state.memorable_config.word_count,
            3,
            12,
        ),
        GenerationStyle::Pin => (
            t!("tui.generator.length").to_string(),
            state.pin_config.length,
            4,
            16,
        ),
    };
    let slider_focused = state.focus == GeneratorFocus::LengthSlider;
    lines.push(length_slider::render_length_slider(
        label.as_str(),
        value,
        min,
        max,
        slider_focused,
    ));
    lines.push(Line::raw(""));

    // Style-specific options
    match state.style {
        GenerationStyle::Random => {
            lines.extend(render_random_toggles(state));
        }
        GenerationStyle::Memorable => {
            lines.extend(render_memorable_options(state));
        }
        GenerationStyle::Pin => {
            // No extra options for PIN
        }
    }

    lines.push(Line::raw(""));
    lines.push(separator_line(width));
    lines.push(Line::raw(""));

    // Preview
    lines.push(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            state.preview_expose(|s| s.to_owned()),
            Style::default().fg(theme::TEXT),
        ),
    ]));

    // Strength bar
    if let Some(ref strength) = state.strength {
        lines.push(strength_bar::render_strength_bar(strength, unicode));
    } else {
        lines.push(strength_bar::render_empty_strength_bar());
    }

    lines.push(Line::raw(""));
    lines.push(separator_line(width));
    lines.push(Line::raw(""));

    // Buttons
    let regen_style = if state.focus == GeneratorFocus::RegenerateButton {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };
    let action_label = if is_embedded {
        t!("tui.generator.use_password").to_string()
    } else {
        t!("tui.notification.copied_to_clipboard").to_string()
    };
    let action_style = if state.focus == GeneratorFocus::ActionButton {
        Style::default()
            .fg(theme::PRIMARY)
            .add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(theme::PRIMARY)
    };

    lines.push(Line::from(vec![
        Span::raw("     "),
        Span::styled(
            format!(" [ {} ] ", t!("tui.generator.regenerate")),
            regen_style,
        ),
        Span::raw("        "),
        Span::styled(format!(" [ {} ] ", action_label), action_style),
    ]));

    lines
}

/// Render style selector tabs (standalone only).
fn render_style_selector(state: &GeneratorState) -> Line<'static> {
    let styles = [
        (
            GenerationStyle::Random,
            t!("tui.generator.style_random").to_string(),
        ),
        (
            GenerationStyle::Memorable,
            t!("tui.generator.style_memorable").to_string(),
        ),
        (
            GenerationStyle::Pin,
            t!("tui.generator.style_pin").to_string(),
        ),
    ];

    let mut spans = vec![Span::styled(
        format!("  {} ", t!("tui.generator.style_label")),
        Style::default().fg(theme::TEXT_SECONDARY),
    )];
    for (i, (style_type, label)) in styles.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        let is_selected = state.style == *style_type;
        let is_focused = state.focus == GeneratorFocus::StyleSelector && is_selected;
        let s = if is_selected {
            Style::default()
                .fg(theme::PRIMARY)
                .add_modifier(Modifier::BOLD)
        } else if is_focused {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(theme::TEXT_SECONDARY)
        };
        spans.push(Span::styled(format!("[ {} ]", label), s));
    }
    Line::from(spans)
}

/// Render Random-style character type toggles.
fn render_random_toggles(state: &GeneratorState) -> Vec<Line<'static>> {
    let uppercase = t!("tui.generator.uppercase").to_string();
    let lowercase = t!("tui.generator.lowercase").to_string();
    let digits = t!("tui.generator.digits").to_string();
    let symbols = t!("tui.generator.symbols").to_string();

    let toggles = [
        (0, uppercase.as_str(), state.random_config.uppercase, true),
        (1, lowercase.as_str(), true, false),
        (2, digits.as_str(), state.random_config.digits, true),
        (3, symbols.as_str(), state.random_config.symbols, true),
    ];

    let mut row_spans: Vec<Span> = vec![Span::raw("  ")];

    for (idx, label, enabled, interactive) in toggles {
        let is_focused = state.focus == GeneratorFocus::Toggle(idx);
        let check = if enabled { "✓" } else { " " };
        let color = if !interactive {
            theme::TEXT_MUTED
        } else if enabled {
            theme::SUCCESS
        } else {
            theme::TEXT_SECONDARY
        };
        let mut style = Style::default().fg(color);
        if is_focused {
            style = style.add_modifier(Modifier::REVERSED);
        }
        row_spans.push(Span::styled(format!("[{}] {}  ", check, label), style));
    }
    vec![Line::from(row_spans)]
}

/// Render Memorable-style options.
fn render_memorable_options(state: &GeneratorState) -> Vec<Line<'static>> {
    let capitalize_focused = state.focus == GeneratorFocus::Toggle(0);
    let sep_focused = state.focus == GeneratorFocus::SeparatorInput;

    let check = if state.memorable_config.capitalize {
        "✓"
    } else {
        " "
    };
    let cap_style = if capitalize_focused {
        Style::default()
            .fg(theme::SUCCESS)
            .add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(theme::SUCCESS)
    };

    let sep_border = if sep_focused {
        Style::default().fg(theme::PRIMARY)
    } else {
        Style::default().fg(theme::BORDER)
    };

    vec![Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("[{}] {}", check, t!("tui.generator.capitalize")),
            cap_style,
        ),
        Span::raw("    "),
        Span::styled(
            format!("{} ", t!("tui.generator.separator_label")),
            Style::default().fg(theme::TEXT_SECONDARY),
        ),
        Span::styled(
            format!("[ {} ]", state.memorable_config.separator),
            sep_border,
        ),
    ])]
}

fn separator_line(width: u16) -> Line<'static> {
    Line::from(Span::styled(
        "─".repeat(width.saturating_sub(2) as usize),
        Style::default().fg(theme::BORDER),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_standalone_random_has_style_selector() {
        let state = GeneratorState::new();
        let lines = render_generator_panel(&state, false, 56, true);
        assert!(!lines.is_empty());
    }

    #[test]
    fn render_embedded_has_no_style_selector() {
        let state = GeneratorState::new();
        let lines = render_generator_panel(&state, true, 56, true);
        let style_label = t!("tui.generator.style_label").to_string();
        let has_style = lines
            .iter()
            .any(|l| l.spans.iter().any(|s| s.content.contains(&style_label)));
        assert!(!has_style);
    }

    #[test]
    fn render_panel_has_preview() {
        let state = GeneratorState::new();
        let lines = render_generator_panel(&state, false, 56, true);
        let preview = state.preview_expose(|s| s.to_owned());
        let has_preview = lines
            .iter()
            .any(|l| l.spans.iter().any(|s| s.content.contains(&preview)));
        assert!(has_preview);
    }
}
