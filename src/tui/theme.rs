//! Tokyo Night color palette and reusable Style helpers.

use ratatui::style::{Color, Modifier, Style};

// ── Background ──────────────────────────────
pub const BG: Color = Color::Rgb(26, 27, 38); // #1a1b26
pub const BG_BAR: Color = Color::Rgb(31, 35, 53); // #1f2335
pub const BG_SURFACE: Color = Color::Rgb(31, 35, 53); // #1f2335
pub const BORDER: Color = Color::Rgb(41, 46, 66); // #292e42

// ── Text ────────────────────────────────────
pub const TEXT: Color = Color::Rgb(192, 202, 245); // #c0caf5
pub const TEXT_SECONDARY: Color = Color::Rgb(86, 95, 137); // #565f89
pub const TEXT_MUTED: Color = Color::Rgb(59, 66, 97); // #3b4261
pub const TEXT_PLACEHOLDER: Color = Color::Rgb(59, 66, 97); // #3b4261
pub const TEXT_TERTIARY: Color = Color::Rgb(65, 72, 104); // #414868

// ── Semantic Colors ─────────────────────────
pub const BRAND: Color = Color::Rgb(187, 154, 247); // #bb9af7
pub const PRIMARY: Color = Color::Rgb(122, 162, 247); // #7aa2f7
pub const ERROR: Color = Color::Rgb(247, 118, 142); // #f7768e
pub const WARNING: Color = Color::Rgb(255, 158, 100); // #ff9e64
pub const SUCCESS: Color = Color::Rgb(158, 206, 106); // #9ece6a
pub const INFO: Color = Color::Rgb(122, 162, 247); // #7aa2f7

// ── Semantic Icons (ASCII fallbacks in parens) ──
pub const ICON_SUCCESS: &str = "\u{2713}";
pub const ICON_ERROR: &str = "\u{2717}";
pub const ICON_WARNING: &str = "\u{26A0}";
pub const ICON_INFO: &str = "\u{2139}";
pub const ICON_LOCK: &str = "\u{1F510}";
pub const ICON_KEY: &str = "\u{1F511}";
pub const ICON_STAR: &str = "\u{2605}";
pub const ICON_FOLDER: &str = "\u{1F4C1}";
pub const ICON_TRASH: &str = "\u{1F5D1}";
pub const ICON_CHECK: &str = "\u{2611}";
pub const ICON_ARROW_LR: &str = "\u{2190}\u{2192}"; // ←→
pub const ICON_PIPE: &str = "\u{2502}"; // │
pub const ICON_SEARCH: &str = "\u{1F50D}";
pub const ICON_PROGRESS_FILL: &str = "\u{2588}";
pub const ICON_PROGRESS_EMPTY: &str = "\u{2591}";
pub const ICON_DROPDOWN: &str = "\u{25BC}"; // ▼
pub const ICON_PASSWORD_MASK: &str = "\u{2022}"; // •
pub const ICON_SYNC_SYNCING: &str = "\u{27F3}"; // ⟳
pub const ICON_SYNC_ROTATING: &str = "\u{27F2}"; // ⟲
pub const ICON_SYNC_OFFLINE: &str = "\u{25D0}"; // ◐
pub const ICON_NOT_CONFIGURED: &str = "\u{2014}"; // —
pub const SPINNER_FRAMES: &[&str] = &[
    "\u{280B}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283C}", "\u{2834}", "\u{2826}", "\u{2827}",
    "\u{2807}", "\u{280F}",
];

/// ASCII fallbacks for terminals without Unicode support.
pub mod ascii {
    pub const ICON_SUCCESS: &str = "+";
    pub const ICON_ERROR: &str = "x";
    pub const ICON_WARNING: &str = "!";
    pub const ICON_INFO: &str = "i";
    pub const ICON_LOCK: &str = "[LOCK]";
    pub const ICON_KEY: &str = "[KEY]";
    pub const ICON_STAR: &str = "*";
    pub const ICON_FOLDER: &str = "[DIR]";
    pub const ICON_TRASH: &str = "[DEL]";
    pub const ICON_CHECK: &str = "[x]";
    pub const ICON_ARROW_LR: &str = "<->";
    pub const ICON_PIPE: &str = "|";
    pub const ICON_SEARCH: &str = "[?]";
    pub const ICON_PROGRESS_FILL: &str = "#";
    pub const ICON_PROGRESS_EMPTY: &str = "-";
    pub const ICON_DROPDOWN: &str = "v";
    pub const ICON_PASSWORD_MASK: &str = "*";
    pub const ICON_SYNC_SYNCING: &str = "~";
    pub const ICON_SYNC_ROTATING: &str = "~";
    pub const ICON_SYNC_OFFLINE: &str = "o";
    pub const ICON_NOT_CONFIGURED: &str = "-";
    pub const SPINNER_FRAMES: &[&str] = &["-", "\\", "|", "/"];
}

/// Reusable style presets for common UI elements.
pub struct Styles;

impl Styles {
    pub fn password_input() -> Style {
        Style::default().bg(BG_SURFACE).fg(TEXT)
    }
    pub fn focused_border() -> Style {
        Style::default().fg(PRIMARY)
    }
    pub fn unfocused_border() -> Style {
        Style::default().fg(BORDER)
    }
    pub fn error_border() -> Style {
        Style::default().fg(ERROR)
    }
    pub fn error_text() -> Style {
        Style::default().fg(ERROR)
    }
    pub fn success_text() -> Style {
        Style::default().fg(SUCCESS)
    }
    pub fn warning_text() -> Style {
        Style::default().fg(WARNING)
    }
    pub fn brand_text() -> Style {
        Style::default().fg(BRAND).add_modifier(Modifier::BOLD)
    }
    pub fn dim_text() -> Style {
        Style::default().fg(TEXT_MUTED).add_modifier(Modifier::DIM)
    }
    pub fn selected_unfocused() -> Style {
        Style::default().add_modifier(Modifier::DIM)
    }
    pub fn selected_focused() -> Style {
        Style::default().add_modifier(Modifier::REVERSED)
    }
    pub fn button_primary() -> Style {
        Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)
    }
    pub fn button_secondary() -> Style {
        Style::default().fg(TEXT_SECONDARY)
    }
    pub fn button_disabled() -> Style {
        Style::default().fg(TEXT_MUTED).add_modifier(Modifier::DIM)
    }
    pub fn title_bar() -> Style {
        Style::default().bg(BG_BAR).fg(TEXT)
    }
}
