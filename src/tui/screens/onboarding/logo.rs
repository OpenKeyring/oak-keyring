//! ASCII art logo for the onboarding welcome screen.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

const LOGO_LINES: &[&str] = &[
    r#"   ____                   __ __                _            "#,
    r#"  / __ \____  ___  ____  / //_/__  __  _______(_)___  ____ _"#,
    r#" / / / / __ \/ _ \/ __ \/ ,< / _ \/ / / / ___/ / __ \/ __ `/"#,
    r#"/ /_/ / /_/ /  __/ / / / /| /  __/ /_/ / /  / / / / / /_/ / "#,
    r#"\____/ .___/\___/_/ /_/_/ |_\___/\__, /_/  /_/_/ /_/\__, /  "#,
    r#"    /_/                         /____/             /____/   "#,
];

// Gradient from brand purple #bb9af7 to deeper purple #9d7cd8
const LOGO_COLORS: [Color; 6] = [
    Color::Rgb(187, 154, 247), // #bb9af7
    Color::Rgb(178, 146, 244), // #b292f4
    Color::Rgb(169, 138, 241), // #a98af1
    Color::Rgb(160, 130, 238), // #a082ee
    Color::Rgb(151, 122, 235), // #977aeb
    Color::Rgb(157, 124, 216), // #9d7cd8
];

pub fn ascii_logo_lines() -> Vec<Line<'static>> {
    LOGO_LINES
        .iter()
        .zip(LOGO_COLORS.iter())
        .map(|(text, color)| {
            Line::from(vec![Span::styled(
                *text,
                Style::default().fg(*color).add_modifier(Modifier::BOLD),
            )])
        })
        .collect()
}

pub const LOGO_HEIGHT: u16 = 6;
pub const LOGO_WIDTH: u16 = 60;
