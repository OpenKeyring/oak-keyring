//! Shared generator panel component for standalone dialog and embedded panel.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

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
        GenerationStyle::Random => ("长度", state.random_config.length, 8, 128),
        GenerationStyle::Memorable => ("单词数", state.memorable_config.word_count, 3, 12),
        GenerationStyle::Pin => ("长度", state.pin_config.length, 4, 16),
    };
    let slider_focused = state.focus == GeneratorFocus::LengthSlider;
    lines.push(length_slider::render_length_slider(
        label,
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
        Span::styled(state.preview.clone(), Style::default().fg(theme::TEXT)),
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
        "使用此密码"
    } else {
        "复制到剪贴板"
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
        Span::styled(" [ 重新生成 ] ", regen_style),
        Span::raw("        "),
        Span::styled(format!(" [ {} ] ", action_label), action_style),
    ]));

    lines
}

/// Render style selector tabs (standalone only).
fn render_style_selector(state: &GeneratorState) -> Line<'static> {
    let styles = [
        (GenerationStyle::Random, "Random"),
        (GenerationStyle::Memorable, "Memorable"),
        (GenerationStyle::Pin, "PIN"),
    ];

    let mut spans = vec![Span::styled(
        "  风格 ",
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
    let toggles = [
        (0, "大写字母", state.random_config.uppercase, true),
        (1, "小写字母", true, false),
        (2, "数字", state.random_config.digits, true),
        (3, "特殊符号", state.random_config.symbols, true),
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
        Span::styled(format!("[{}] 首字母大写", check), cap_style),
        Span::raw("    "),
        Span::styled("分隔符: ", Style::default().fg(theme::TEXT_SECONDARY)),
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
        let has_style = lines
            .iter()
            .any(|l| l.spans.iter().any(|s| s.content.contains("风格")));
        assert!(!has_style);
    }

    #[test]
    fn render_panel_has_preview() {
        let state = GeneratorState::new();
        let lines = render_generator_panel(&state, false, 56, true);
        let has_preview = lines
            .iter()
            .any(|l| l.spans.iter().any(|s| s.content.contains(&state.preview)));
        assert!(has_preview);
    }
}
