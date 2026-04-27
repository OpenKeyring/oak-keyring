//! Cross-platform browser opening.

/// Return the platform-specific command to open a URL.
pub fn browser_command() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["open"]
    } else if cfg!(target_os = "linux") {
        &["xdg-open"]
    } else if cfg!(target_os = "windows") {
        &["cmd", "/c", "start"]
    } else {
        &["open"]
    }
}

/// Open a URL in the system default browser.
/// Returns Ok(()) if the process was spawned, Err otherwise.
pub fn open_url(url: &str) -> Result<(), String> {
    let cmd = browser_command();
    std::process::Command::new(cmd[0])
        .args(&cmd[1..])
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法打开浏览器: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_command_returns_open_for_macos() {
        if cfg!(target_os = "macos") {
            let cmds = browser_command();
            assert_eq!(cmds[0], "open");
        }
    }

    #[test]
    fn browser_command_returns_xdg_open_for_linux() {
        if cfg!(target_os = "linux") {
            let cmds = browser_command();
            assert_eq!(cmds[0], "xdg-open");
        }
    }
}
