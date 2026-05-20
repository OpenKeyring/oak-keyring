use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::tui::screens::recovery_key::{WordGridMode, WordGridState};

use crate::support::snapshot_locale;

fn render_word_grid(grid: &WordGridState, width: u16, height: u16) -> TestBackend {
    let _locale = snapshot_locale();
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            grid.view(frame, frame.area());
        })
        .unwrap();
    terminal.backend().clone()
}

#[test]
fn word_grid_full_input_empty() {
    let grid = WordGridState::default();
    let backend = render_word_grid(&grid, 80, 10);
    insta::assert_snapshot!("word_grid_full_input_empty", backend);
}

#[test]
fn word_grid_full_input_populated() {
    let mut grid = WordGridState::default();
    grid.mode = WordGridMode::FullInput;
    grid.words[0] = "abandon".to_string();
    grid.words[1] = "ability".to_string();
    grid.words[2] = "able".to_string();
    grid.focused_index = 3;
    let backend = render_word_grid(&grid, 80, 10);
    insta::assert_snapshot!("word_grid_full_input_populated", backend);
}

#[test]
fn word_grid_partial_verify() {
    let mut grid = WordGridState::default();
    grid.mode = WordGridMode::PartialVerify {
        positions: [0, 5, 12, 19],
    };
    grid.words[0] = "abandon".to_string();
    grid.words[5] = "wrong".to_string();
    grid.errors[5] = true;
    grid.focused_index = 12;
    let backend = render_word_grid(&grid, 80, 10);
    insta::assert_snapshot!("word_grid_partial_verify", backend);
}
