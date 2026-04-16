//! Recovery key input — 24-word BIP39 grid component.
//!
//! This is a reusable component (not a Screen trait implementation) used by
//! OnboardingScreen and other flows that need recovery key input or verification.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Constraint;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Row, Table};

use crate::tui::theme::{BORDER, ERROR, PRIMARY, TEXT, TEXT_MUTED, TEXT_SECONDARY};

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum WordGridMode {
    /// All 24 words editable (for restore).
    #[default]
    FullInput,
    /// Only 4 random positions editable (for verification).
    PartialVerify { positions: [usize; 4] },
}

/// State for the 24-word BIP39 recovery key grid.
#[derive(Debug, Clone, Default)]
pub struct WordGridState {
    pub words: [String; 24],
    pub errors: [bool; 24],
    pub focused_index: usize,
    pub mode: WordGridMode,
}

// ── Helpers ────────────────────────────────────────────────────────────────

impl WordGridState {
    /// Returns the indices of editable word slots based on the current mode.
    pub fn editable_indices(&self) -> Vec<usize> {
        match &self.mode {
            WordGridMode::FullInput => (0..24).collect(),
            WordGridMode::PartialVerify { positions } => {
                let mut v: Vec<usize> = positions.to_vec();
                v.sort();
                v
            }
        }
    }

    /// Returns `true` if the word at `index` is editable in the current mode.
    fn is_editable(&self, index: usize) -> bool {
        match &self.mode {
            WordGridMode::FullInput => true,
            WordGridMode::PartialVerify { positions } => positions.contains(&index),
        }
    }

    /// Move focus to the next editable word (wraps around).
    pub fn next_word(&mut self) {
        let editable = self.editable_indices();
        if editable.is_empty() {
            return;
        }
        // Find the next editable index after the current focused_index (wrapping)
        let next = editable
            .iter()
            .find(|&&i| i > self.focused_index)
            .copied()
            .unwrap_or(editable[0]);
        self.focused_index = next;
    }

    /// Move focus to the previous editable word (wraps around).
    pub fn prev_word(&mut self) {
        let editable = self.editable_indices();
        if editable.is_empty() {
            return;
        }
        // Find the previous editable index before the current focused_index (wrapping)
        let prev = editable
            .iter()
            .rev()
            .find(|&&i| i < self.focused_index)
            .copied()
            .unwrap_or(editable[editable.len() - 1]);
        self.focused_index = prev;
    }

    /// Returns `true` if all editable words are non-empty.
    pub fn all_filled(&self) -> bool {
        self.editable_indices()
            .iter()
            .all(|&i| !self.words[i].is_empty())
    }

    /// Returns all 24 words as a `Vec<String>`.
    pub fn collect_words(&self) -> Vec<String> {
        self.words.to_vec()
    }

    /// Handle a key event. Returns `Some(words)` when Enter is pressed and all
    /// editable words are filled.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Vec<String>> {
        match key.code {
            KeyCode::Tab => {
                self.next_word();
                None
            }
            KeyCode::BackTab => {
                self.prev_word();
                None
            }
            KeyCode::Enter => {
                if self.all_filled() {
                    Some(self.collect_words())
                } else {
                    self.next_word();
                    None
                }
            }
            KeyCode::Backspace => {
                if self.is_editable(self.focused_index) {
                    self.words[self.focused_index].pop();
                }
                None
            }
            KeyCode::Char(c) if c.is_alphabetic() => {
                if self.is_editable(self.focused_index) {
                    let word = &mut self.words[self.focused_index];
                    if word.len() < 12 {
                        word.push(c);
                    }
                }
                None
            }
            KeyCode::Char(' ') => {
                if !self.words[self.focused_index].is_empty() {
                    self.next_word();
                }
                None
            }
            _ => None,
        }
    }

    /// Render the word grid into the given `Frame` and `area`.
    pub fn view(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        // 4 columns x 6 rows
        let rows: Vec<Row> = (0..6)
            .map(|row| {
                let cells: Vec<Line> = (0..4)
                    .map(|col| {
                        let idx = row * 4 + col;
                        self.render_cell(idx)
                    })
                    .collect();
                Row::new(cells)
            })
            .collect();

        let widths = [
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ];

        let table = Table::new(rows, widths).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BORDER)),
        );

        frame.render_widget(table, area);
    }

    /// Build the styled `Line` for a single cell at `index`.
    fn render_cell(&self, index: usize) -> Line<'static> {
        let is_focused = index == self.focused_index;
        let is_error = self.errors[index];
        let is_editable = self.is_editable(index);

        // Number prefix: right-aligned 2-digit
        let num_str = format!("{:>2}.", index + 1);

        let (word_text, base_style) = if !is_editable {
            // Non-editable in PartialVerify: dimmed dots
            (
                "\u{00b7}\u{00b7}\u{00b7}\u{00b7}".to_string(),
                Style::default().fg(TEXT_MUTED),
            )
        } else if self.words[index].is_empty() {
            // Empty editable: placeholder
            ("____".to_string(), Style::default().fg(TEXT_MUTED))
        } else if is_error {
            // Error word
            (self.words[index].clone(), Style::default().fg(ERROR))
        } else {
            // Normal filled word
            (self.words[index].clone(), Style::default().fg(TEXT))
        };

        // Build spans for the cell
        let num_style = if is_focused {
            Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_SECONDARY)
        };

        let word_style = if is_focused {
            base_style.add_modifier(Modifier::UNDERLINED)
        } else {
            base_style
        };

        let separator = Span::styled(" ", Style::default());

        Line::from(vec![
            Span::styled(num_str, num_style),
            separator,
            Span::styled(word_text, word_style),
        ])
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_key_grid_default() {
        let state = WordGridState::default();
        assert_eq!(state.focused_index, 0);
        assert!(!state.all_filled());
        assert!(state.words.iter().all(|w| w.is_empty()));
        assert!(state.errors.iter().all(|&e| !e));
        assert_eq!(state.mode, WordGridMode::FullInput);
    }

    #[test]
    fn recovery_key_next_prev_word() {
        let mut state = WordGridState::default();
        assert_eq!(state.focused_index, 0);

        state.next_word();
        assert_eq!(state.focused_index, 1);

        state.next_word();
        assert_eq!(state.focused_index, 2);

        state.prev_word();
        assert_eq!(state.focused_index, 1);

        // Wrap around forward
        state.focused_index = 23;
        state.next_word();
        assert_eq!(state.focused_index, 0);

        // Wrap around backward
        state.focused_index = 0;
        state.prev_word();
        assert_eq!(state.focused_index, 23);
    }

    #[test]
    fn recovery_key_partial_verify_only_editable() {
        let state = WordGridState {
            mode: WordGridMode::PartialVerify {
                positions: [3, 7, 11, 19],
            },
            ..Default::default()
        };

        let editable = state.editable_indices();
        assert_eq!(editable, vec![3, 7, 11, 19]);

        // Verify non-position indices are not editable
        for i in 0..24 {
            if [3, 7, 11, 19].contains(&i) {
                assert!(state.is_editable(i), "index {} should be editable", i);
            } else {
                assert!(!state.is_editable(i), "index {} should not be editable", i);
            }
        }
    }

    #[test]
    fn recovery_key_partial_verify_navigation() {
        let mut state = WordGridState {
            focused_index: 0,
            mode: WordGridMode::PartialVerify {
                positions: [3, 7, 11, 19],
            },
            ..Default::default()
        };

        // Starting at 0 (not in positions), next should go to first editable (3)
        state.next_word();
        assert_eq!(state.focused_index, 3);

        state.next_word();
        assert_eq!(state.focused_index, 7);

        state.next_word();
        assert_eq!(state.focused_index, 11);

        state.next_word();
        assert_eq!(state.focused_index, 19);

        // Wrap forward
        state.next_word();
        assert_eq!(state.focused_index, 3);

        // Wrap backward from 3
        state.prev_word();
        assert_eq!(state.focused_index, 19);

        state.prev_word();
        assert_eq!(state.focused_index, 11);
    }

    #[test]
    fn recovery_key_all_filled_check() {
        let mut state = WordGridState::default();
        assert!(!state.all_filled());

        // Fill all words
        for word in &mut state.words {
            word.push_str("abandon");
        }
        assert!(state.all_filled());
    }

    #[test]
    fn recovery_key_all_filled_partial() {
        let mut state = WordGridState {
            mode: WordGridMode::PartialVerify {
                positions: [0, 8, 16, 23],
            },
            ..Default::default()
        };

        assert!(!state.all_filled());

        // Fill only the editable positions
        state.words[0] = "abandon".to_string();
        state.words[8] = "zoo".to_string();
        state.words[16] = "correct".to_string();
        state.words[23] = "art".to_string();

        assert!(state.all_filled());
    }

    #[test]
    fn recovery_key_handle_key_typing() {
        let mut state = WordGridState::default();

        let result = state.handle_key(KeyEvent::new(
            KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(result.is_none());
        assert_eq!(state.words[0], "a");

        let result = state.handle_key(KeyEvent::new(
            KeyCode::Char('b'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(result.is_none());
        assert_eq!(state.words[0], "ab");

        // Space moves to next word
        let result = state.handle_key(KeyEvent::new(
            KeyCode::Char(' '),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(result.is_none());
        assert_eq!(state.focused_index, 1);

        // Backspace on empty word at index 1 does nothing, then typing works
        let result = state.handle_key(KeyEvent::new(
            KeyCode::Backspace,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(result.is_none());
        assert_eq!(state.words[1], "");

        let result = state.handle_key(KeyEvent::new(
            KeyCode::Char('z'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(result.is_none());
        assert_eq!(state.words[1], "z");
    }

    #[test]
    fn recovery_key_handle_key_max_length() {
        let mut state = WordGridState::default();

        // Type 13 chars — only 12 should be kept
        for c in "abcdefghijklm".chars() {
            state.handle_key(KeyEvent::new(
                KeyCode::Char(c),
                crossterm::event::KeyModifiers::NONE,
            ));
        }
        assert_eq!(state.words[0].len(), 12);
        assert_eq!(state.words[0], "abcdefghijkl");
    }

    #[test]
    fn recovery_key_handle_key_enter_not_filled() {
        let mut state = WordGridState::default();

        let result = state.handle_key(KeyEvent::new(
            KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(result.is_none());
        // Enter when not all filled advances to next word
        assert_eq!(state.focused_index, 1);
    }

    #[test]
    fn recovery_key_handle_key_enter_all_filled() {
        let mut state = WordGridState::default();
        for word in &mut state.words {
            word.push_str("abandon");
        }

        let result = state.handle_key(KeyEvent::new(
            KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(result.is_some());
        let words = result.unwrap();
        assert_eq!(words.len(), 24);
        assert!(words.iter().all(|w| w == "abandon"));
    }

    #[test]
    fn recovery_key_collect_words() {
        let mut state = WordGridState::default();
        state.words[0] = "abandon".to_string();
        state.words[23] = "zoo".to_string();

        let words = state.collect_words();
        assert_eq!(words.len(), 24);
        assert_eq!(words[0], "abandon");
        assert_eq!(words[23], "zoo");
        assert!(words[1..23].iter().all(|w| w.is_empty()));
    }

    #[test]
    fn recovery_key_non_alphabetic_ignored() {
        let mut state = WordGridState::default();

        let result = state.handle_key(KeyEvent::new(
            KeyCode::Char('1'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(result.is_none());
        assert_eq!(state.words[0], "");

        let result = state.handle_key(KeyEvent::new(
            KeyCode::Char('-'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(result.is_none());
        assert_eq!(state.words[0], "");
    }

    #[test]
    fn recovery_key_backspace_pops_char() {
        let mut state = WordGridState::default();
        state.words[0] = "abc".to_string();
        state.focused_index = 0;

        state.handle_key(KeyEvent::new(
            KeyCode::Backspace,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(state.words[0], "ab");

        state.handle_key(KeyEvent::new(
            KeyCode::Backspace,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(state.words[0], "a");

        state.handle_key(KeyEvent::new(
            KeyCode::Backspace,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(state.words[0], "");
    }

    #[test]
    fn recovery_key_partial_verify_non_editable_ignores_input() {
        let mut state = WordGridState {
            focused_index: 0,
            mode: WordGridMode::PartialVerify {
                positions: [3, 7, 11, 19],
            },
            ..Default::default()
        };

        // Index 0 is not editable, typing should be ignored
        let result = state.handle_key(KeyEvent::new(
            KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(result.is_none());
        assert_eq!(state.words[0], "");

        // Backspace on non-editable does nothing
        let result = state.handle_key(KeyEvent::new(
            KeyCode::Backspace,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(result.is_none());
        assert_eq!(state.words[0], "");

        // Tab should move to first editable (3)
        state.handle_key(KeyEvent::new(
            KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(state.focused_index, 3);

        // Now typing should work
        state.handle_key(KeyEvent::new(
            KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(state.words[3], "a");
    }
}
