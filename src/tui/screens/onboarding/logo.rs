//! ASCII art logo for the onboarding welcome screen.
//! Generated with `toilet -f pagga OpenKeyring | lolcat -f -S 42`.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

const LOGO_LINES: &[&str] = &[
    "░█▀█░█▀█░█▀▀░█▀█░█░█░█▀▀░█░█░█▀▄░▀█▀░█▀█░█▀▀",
    "░█░█░█▀▀░█▀▀░█░█░█▀▄░█▀▀░░█░░█▀▄░░█░░█░█░█░█",
    "░▀▀▀░▀░░░▀▀▀░▀░▀░▀░▀░▀▀▀░░▀░░▀░▀░▀▀▀░▀░▀░▀▀▀",
];

const LINE0: [Color; 44] = [
    Color::Rgb(11, 142, 230), Color::Rgb(10, 146, 227), Color::Rgb(8, 150, 225),
    Color::Rgb(7, 154, 222),  Color::Rgb(5, 158, 219),  Color::Rgb(4, 162, 216),
    Color::Rgb(3, 166, 213),  Color::Rgb(3, 170, 210),  Color::Rgb(2, 174, 206),
    Color::Rgb(1, 178, 203),  Color::Rgb(1, 182, 199),  Color::Rgb(1, 186, 196),
    Color::Rgb(1, 190, 192),  Color::Rgb(1, 193, 189),  Color::Rgb(1, 197, 185),
    Color::Rgb(1, 200, 181),  Color::Rgb(1, 204, 177),  Color::Rgb(2, 207, 173),
    Color::Rgb(3, 210, 169),  Color::Rgb(4, 214, 165),  Color::Rgb(5, 217, 161),
    Color::Rgb(6, 220, 157),  Color::Rgb(7, 222, 153),  Color::Rgb(8, 225, 149),
    Color::Rgb(10, 228, 145), Color::Rgb(12, 230, 141), Color::Rgb(13, 233, 136),
    Color::Rgb(15, 235, 132), Color::Rgb(17, 237, 128), Color::Rgb(20, 239, 124),
    Color::Rgb(22, 241, 119), Color::Rgb(24, 243, 115), Color::Rgb(27, 245, 111),
    Color::Rgb(29, 246, 107), Color::Rgb(32, 248, 103), Color::Rgb(35, 249, 98),
    Color::Rgb(38, 250, 94),  Color::Rgb(41, 251, 90),  Color::Rgb(44, 252, 86),
    Color::Rgb(47, 253, 82),  Color::Rgb(51, 253, 78),  Color::Rgb(54, 254, 74),
    Color::Rgb(58, 254, 71),  Color::Rgb(61, 254, 67),
];

const LINE1: [Color; 44] = [
    Color::Rgb(7, 154, 222),  Color::Rgb(5, 158, 219),  Color::Rgb(4, 162, 216),
    Color::Rgb(3, 166, 213),  Color::Rgb(3, 170, 210),  Color::Rgb(2, 174, 206),
    Color::Rgb(1, 178, 203),  Color::Rgb(1, 182, 199),  Color::Rgb(1, 186, 196),
    Color::Rgb(1, 190, 192),  Color::Rgb(1, 193, 189),  Color::Rgb(1, 197, 185),
    Color::Rgb(1, 200, 181),  Color::Rgb(1, 204, 177),  Color::Rgb(2, 207, 173),
    Color::Rgb(3, 210, 169),  Color::Rgb(4, 214, 165),  Color::Rgb(5, 217, 161),
    Color::Rgb(6, 220, 157),  Color::Rgb(7, 222, 153),  Color::Rgb(8, 225, 149),
    Color::Rgb(10, 228, 145), Color::Rgb(12, 230, 141), Color::Rgb(13, 233, 136),
    Color::Rgb(15, 235, 132), Color::Rgb(17, 237, 128), Color::Rgb(20, 239, 124),
    Color::Rgb(22, 241, 119), Color::Rgb(24, 243, 115), Color::Rgb(27, 245, 111),
    Color::Rgb(29, 246, 107), Color::Rgb(32, 248, 103), Color::Rgb(35, 249, 98),
    Color::Rgb(38, 250, 94),  Color::Rgb(41, 251, 90),  Color::Rgb(44, 252, 86),
    Color::Rgb(47, 253, 82),  Color::Rgb(51, 253, 78),  Color::Rgb(54, 254, 74),
    Color::Rgb(58, 254, 71),  Color::Rgb(61, 254, 67),  Color::Rgb(65, 254, 63),
    Color::Rgb(68, 254, 60),  Color::Rgb(72, 254, 56),
];

const LINE2: [Color; 44] = [
    Color::Rgb(3, 166, 213),  Color::Rgb(3, 170, 210),  Color::Rgb(2, 174, 206),
    Color::Rgb(1, 178, 203),  Color::Rgb(1, 182, 199),  Color::Rgb(1, 186, 196),
    Color::Rgb(1, 190, 192),  Color::Rgb(1, 193, 189),  Color::Rgb(1, 197, 185),
    Color::Rgb(1, 200, 181),  Color::Rgb(1, 204, 177),  Color::Rgb(2, 207, 173),
    Color::Rgb(3, 210, 169),  Color::Rgb(4, 214, 165),  Color::Rgb(5, 217, 161),
    Color::Rgb(6, 220, 157),  Color::Rgb(7, 222, 153),  Color::Rgb(8, 225, 149),
    Color::Rgb(10, 228, 145), Color::Rgb(12, 230, 141), Color::Rgb(13, 233, 136),
    Color::Rgb(15, 235, 132), Color::Rgb(17, 237, 128), Color::Rgb(20, 239, 124),
    Color::Rgb(22, 241, 119), Color::Rgb(24, 243, 115), Color::Rgb(27, 245, 111),
    Color::Rgb(29, 246, 107), Color::Rgb(32, 248, 103), Color::Rgb(35, 249, 98),
    Color::Rgb(38, 250, 94),  Color::Rgb(41, 251, 90),  Color::Rgb(44, 252, 86),
    Color::Rgb(47, 253, 82),  Color::Rgb(51, 253, 78),  Color::Rgb(54, 254, 74),
    Color::Rgb(58, 254, 71),  Color::Rgb(61, 254, 67),  Color::Rgb(65, 254, 63),
    Color::Rgb(68, 254, 60),  Color::Rgb(72, 254, 56),  Color::Rgb(76, 254, 53),
    Color::Rgb(80, 253, 49),  Color::Rgb(84, 253, 46),
];

const LOGO_COLORS: &[&[Color]] = &[&LINE0, &LINE1, &LINE2];

pub fn ascii_logo_lines() -> Vec<Line<'static>> {
    LOGO_LINES
        .iter()
        .zip(LOGO_COLORS.iter())
        .map(|(text, colors)| {
            let spans: Vec<Span<'static>> = text
                .chars()
                .zip(colors.iter())
                .map(|(ch, color)| {
                    Span::styled(
                        ch.to_string(),
                        Style::default().fg(*color).add_modifier(Modifier::BOLD),
                    )
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

pub const LOGO_HEIGHT: u16 = 3;
