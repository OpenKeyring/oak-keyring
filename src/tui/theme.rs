//! Tokyo Night color palette and reusable Style helpers.

use ratatui::style::{Color, Modifier, Style};

// ── Background ──────────────────────────────
pub const BG: Color = Color::Rgb(26, 27, 38); // #1a1b26
pub const BG_BAR: Color = Color::Rgb(31, 35, 53); // #1f2335
pub const BG_SURFACE: Color = Color::Rgb(31, 35, 53); // #1f2335
pub const BORDER: Color = Color::Rgb(41, 46, 66); // #292e42

// ── New-look main screen palette ───────────
pub const NL_BG: Color = Color::Rgb(13, 16, 32); // #0D1020
pub const NL_SURFACE: Color = Color::Rgb(20, 24, 39); // #141827
pub const NL_SURFACE_2: Color = Color::Rgb(26, 32, 52); // #1A2034
pub const NL_SELECTED: Color = Color::Rgb(36, 45, 79); // #242D4F
pub const NL_LINE: Color = Color::Rgb(40, 50, 74); // #28324A
pub const NL_FOCUS: Color = Color::Rgb(122, 162, 255); // #7AA2FF
pub const NL_CYAN: Color = Color::Rgb(52, 228, 255); // #34E4FF
pub const NL_TEXT: Color = Color::Rgb(234, 241, 255); // #EAF1FF
pub const NL_TEXT_MUTED: Color = Color::Rgb(140, 150, 181); // #8C96B5
pub const NL_HOT: Color = Color::Rgb(255, 138, 76); // #FF8A4C
pub const NL_SUCCESS: Color = Color::Rgb(167, 240, 112); // #A7F070
pub const NL_DANGER: Color = Color::Rgb(255, 93, 115); // #FF5D73

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
pub const ICON_ARROW_UD: &str = "\u{2191}\u{2193}"; // ↑↓
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
pub const NF_DATABASE: &str = "\u{f1c0}";
pub const NF_KEY: &str = "\u{f084}";
pub const NF_USER: &str = "\u{f007}";
pub const NF_LOCK: &str = "\u{f023}";
pub const NF_SHIELD: &str = "\u{f132}";
pub const NF_GLOBE: &str = "\u{f0ac}";
pub const NF_TAG: &str = "\u{f02b}";
pub const NF_NOTE: &str = "\u{f15b}";
pub const NF_CLOCK: &str = "\u{f017}";
pub const NF_COPY: &str = "\u{f0c5}";
pub const NF_EYE: &str = "\u{f06e}";
pub const NF_EYE_OFF: &str = "\u{f070}";
pub const NF_STAR: &str = "\u{f005}";
pub const NF_LIST: &str = "\u{f03a}";
pub const NF_TRASH: &str = "\u{f1f8}";
pub const NF_BOLT: &str = "\u{f0e7}";
pub const NF_GEAR: &str = "\u{f013}";
pub const NF_INFO: &str = "\u{f129}";
pub const NF_CLIPBOARD: &str = "\u{f328}";
pub const NF_SPARKLES: &str = "\u{e22b}";
pub const NF_UPLOAD: &str = "\u{f093}";
pub const NF_DOWNLOAD: &str = "\u{f019}";
pub const NF_SYNC: &str = "\u{f021}";
pub const NF_SLIDERS: &str = "\u{f1de}";
pub const NF_CHECK_CIRCLE: &str = "\u{f058}";
pub const NF_EXCLAMATION_CIRCLE: &str = "\u{f06a}";
pub const NF_SHIELD_ALT: &str = "\u{f3ed}";
pub const NF_SECURITY_ISSUES: &str = "\u{f0ecc}";
pub const NF_WARNING_TRIANGLE: &str = "\u{f071}";
pub const NF_ACCOUNT_KEY: &str = "\u{f000b}";
pub const NF_API: &str = "\u{f0bc4}";
pub const NF_SSH: &str = "\u{f1575}";
pub const NF_FILE_LOCK: &str = "\u{f0221}";
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
    pub const ICON_ARROW_UD: &str = "^v";
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
    pub const NF_DATABASE: &str = "[db]";
    pub const NF_KEY: &str = "[key]";
    pub const NF_USER: &str = "[user]";
    pub const NF_LOCK: &str = "[lock]";
    pub const NF_SHIELD: &str = "[shield]";
    pub const NF_GLOBE: &str = "[url]";
    pub const NF_TAG: &str = "[tag]";
    pub const NF_NOTE: &str = "[note]";
    pub const NF_CLOCK: &str = "[time]";
    pub const NF_COPY: &str = "[copy]";
    pub const NF_EYE: &str = "[show]";
    pub const NF_EYE_OFF: &str = "[hide]";
    pub const NF_STAR: &str = "*";
    pub const NF_LIST: &str = "[all]";
    pub const NF_TRASH: &str = "[trash]";
    pub const NF_BOLT: &str = "[gen]";
    pub const NF_GEAR: &str = "[cfg]";
    pub const NF_INFO: &str = "[info]";
    pub const NF_CLIPBOARD: &str = "[clip]";
    pub const NF_SPARKLES: &str = "[fx]";
    pub const NF_UPLOAD: &str = "[up]";
    pub const NF_DOWNLOAD: &str = "[down]";
    pub const NF_SYNC: &str = "[sync]";
    pub const NF_SLIDERS: &str = "[opts]";
    pub const NF_CHECK_CIRCLE: &str = "[ok]";
    pub const NF_EXCLAMATION_CIRCLE: &str = "[!]";
    pub const NF_SHIELD_ALT: &str = "[shield]";
    pub const NF_SECURITY_ISSUES: &str = "[health]";
    pub const NF_WARNING_TRIANGLE: &str = "[!]";
    pub const NF_ACCOUNT_KEY: &str = "[L]";
    pub const NF_API: &str = "[API]";
    pub const NF_SSH: &str = "[SSH]";
    pub const NF_FILE_LOCK: &str = "[N]";
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
    pub fn newlook_bg() -> Style {
        Style::default().bg(NL_BG).fg(NL_TEXT)
    }
    pub fn newlook_surface() -> Style {
        Style::default().bg(NL_SURFACE).fg(NL_TEXT)
    }
    pub fn newlook_surface_2() -> Style {
        Style::default().bg(NL_SURFACE_2).fg(NL_TEXT)
    }
    pub fn newlook_border() -> Style {
        Style::default().fg(NL_LINE).bg(NL_SURFACE)
    }
    pub fn newlook_focused_border() -> Style {
        Style::default().fg(NL_FOCUS).bg(NL_SURFACE)
    }
    pub fn newlook_selected() -> Style {
        Style::default().bg(NL_SELECTED).fg(NL_TEXT)
    }
}
