use arboard::Clipboard;
use std::sync::Mutex;

pub struct ClipboardService {
    clipboard: Mutex<Clipboard>,
}

impl ClipboardService {
    pub fn new() -> Result<Self, String> {
        let clipboard = Clipboard::new().map_err(|e| e.to_string())?;
        Ok(Self {
            clipboard: Mutex::new(clipboard),
        })
    }

    pub fn copy(&self, text: &str) -> Result<(), String> {
        let mut cb = self.clipboard.lock().map_err(|e| e.to_string())?;
        cb.set_text(text).map_err(|e| e.to_string())
    }

    pub fn clear(&self) -> Result<(), String> {
        let mut cb = self.clipboard.lock().map_err(|e| e.to_string())?;
        cb.set_text("").map_err(|e| e.to_string())
    }

    pub fn get_text(&self) -> Result<String, String> {
        let mut cb = self.clipboard.lock().map_err(|e| e.to_string())?;
        cb.get_text().map_err(|e| e.to_string())
    }
}
