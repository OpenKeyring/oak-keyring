//! Loading indicators: spinner frame counter and progress bar state.

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

#[derive(Debug, Clone, Default)]
pub struct SpinnerState {
    pub frame_index: usize,
    pub label: String,
}

impl SpinnerState {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            frame_index: 0,
            label: label.into(),
        }
    }

    pub fn tick(&mut self) {
        self.frame_index = (self.frame_index + 1) % SPINNER_FRAMES.len();
    }

    pub fn frame(&self) -> &str {
        SPINNER_FRAMES[self.frame_index]
    }

    pub fn frames_ascii() -> &'static [&'static str] {
        &["-", "\\", "|", "/"]
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProgressBarState {
    pub current: usize,
    pub total: usize,
    pub label: String,
    pub width: usize,
}

impl ProgressBarState {
    pub fn new(total: usize, label: impl Into<String>) -> Self {
        Self {
            current: 0,
            total,
            label: label.into(),
            width: 30,
        }
    }

    pub fn progress(&self) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        self.current as f64 / self.total as f64
    }

    pub fn percentage(&self) -> u8 {
        (self.progress() * 100.0) as u8
    }
}

/// Global loading state — only one active at a time.
#[derive(Debug, Clone, Default)]
pub enum LoadingIndicator {
    #[default]
    None,
    Spinner(SpinnerState),
    Progress(ProgressBarState),
}

#[derive(Debug, Clone, Default)]
pub struct LoadingState {
    pub indicator: LoadingIndicator,
}
