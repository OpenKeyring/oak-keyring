//! Main screen state: sidebar, status bar, terminal title, and root composition.
//!
//! Contains all state types needed by the three-panel main layout:
//! - [`SidebarState`] — navigation categories, tags, selection
//! - [`StatusBarState`] — clipboard countdown, sync indicator, messages
//! - [`TerminalTitleState`] — dynamic terminal window title
//! - [`MainScreenState`] — root aggregate of all main-screen sub-states

pub mod state;

#[cfg(test)]
mod tests;

pub use state::*;
