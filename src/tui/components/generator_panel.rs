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

/// Characters for the password input box border.
const BOX_TL: &str = "\u{250c}"; // ┌
const BOX_TR: &str = "\u{2510}"; // ┐
const BOX_BL: &str = "\u{2514}"; // └
const BOX_BR: &str = "\u{2518}"; // ┘
const BOX_H: &str = "\u{2500}"; // ─
const BOX_V: &str = "\u{2502}"; // │

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

    lines.push(separator_line(width));

    // Preview — wrapped in a bordered input box
    let inner_width = width.saturating_sub(6) as usize; // 2 padding + 2 borders + margins
    let password_text: String = state.preview_expose(|s: &str| {
        if s.len() > inner_width {
            format!("{}...", &s[..inner_width.saturating_sub(3)])
        } else {
            s.to_owned()
        }
    });
    let border_style = Style::default().fg(theme::BORDER);
    let top_border = format!("{}{}{}", BOX_TL, BOX_H.repeat(inner_width), BOX_TR);
    let bottom_border = format!("{}{}{}", BOX_BL, BOX_H.repeat(inner_width), BOX_BR);
    lines.push(Line::from(Span::styled(
        format!("  {}", top_border),
        border_style,
    )));
    lines.push(Line::from(vec![
        Span::styled(format!("  {} ", BOX_V), border_style),
        Span::styled(password_text, Style::default().fg(theme::TEXT)),
        Span::styled(format!(" {}", BOX_V), border_style),
    ]));
    lines.push(Line::from(Span::styled(
        format!("  {}", bottom_border),
        border_style,
    )));

    // Padding between password box and strength bar (issue 3)
    lines.push(Line::raw(""));

    if let Some(ref strength) = state.strength {
        lines.push(strength_bar::render_strength_bar(strength, unicode));
    } else {
        lines.push(strength_bar::render_empty_strength_bar());
    }

    lines.push(separator_line(width));
    lines.push(Line::raw(""));

    // Buttons with shortcut hints
    let regen_style = if state.focus == GeneratorFocus::RegenerateButton {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };
    let action_style = if state.focus == GeneratorFocus::ActionButton {
        Style::default()
            .fg(theme::PRIMARY)
            .add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(theme::PRIMARY)
    };
    let regen_text = format!(" {}(r) ", t!("tui.generator.regenerate"));
    let action_text = if is_embedded {
        format!(" {}(y) ", t!("tui.generator.use_password"))
    } else {
        format!(" {}(c) ", t!("tui.generator.copy"))
    };

    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("[{}]", regen_text), regen_style),
        Span::raw("        "),
        Span::styled(format!("[{}]", action_text), action_style),
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

    let row_focused = state.focus == GeneratorFocus::StyleSelector;

    let mut spans = vec![Span::styled(
        format!("  {} ", t!("tui.generator.style_label")),
        Style::default().fg(theme::TEXT_SECONDARY),
    )];
    for (i, (style_type, label)) in styles.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        let is_selected = state.style == *style_type;
        let s = if is_selected && row_focused {
            Style::default()
                .fg(theme::PRIMARY)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else if is_selected {
            Style::default()
                .fg(theme::PRIMARY)
                .add_modifier(Modifier::BOLD)
        } else if row_focused {
            Style::default()
                .fg(theme::TEXT_SECONDARY)
                .add_modifier(Modifier::REVERSED)
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
        (1, lowercase.as_str(), state.random_config.lowercase, true),
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
