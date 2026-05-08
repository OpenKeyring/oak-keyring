//! Test harness for process end-to-end testing with PTY isolation.
//!
//! This module provides utilities for spawning the `ok` binary in an isolated
//! environment with controlled environment variables and PTY interaction.
//!
//! ## Panic detection strategy
//!
//! Per docs/research/test/pyt-panic.md:
//! - PTY tests verify screen content + exit code (not panic text matching)
//! - Exit code 101 = Rust panic; exit code 0 = clean shutdown
//! - Panic pattern matching in PTY output is unreliable (alternate screen
//!   swallows panic messages, stdout/stderr are multiplexed)

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use assert_cmd::cargo;
use expectrl::process::{Healthcheck, NonBlocking};
use expectrl::{session::OsSession, Expect, Session};
use strip_ansi_escapes::strip;
use tempfile::TempDir;

/// Test environment with isolated temporary directory and environment variables.
pub struct TestEnv {
    /// Temporary directory for isolated test environment
    pub temp_dir: TempDir,
    /// Path to the vault directory (OAK_VAULT_DIR)
    pub vault_dir: PathBuf,
}

impl TestEnv {
    /// Creates a new isolated test environment with a temporary directory.
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let temp_path = temp_dir.path();

        let data_home = temp_path.join("data");
        let config_home = temp_path.join("config");
        std::fs::create_dir_all(&data_home).expect("Failed to create XDG_DATA_HOME");
        std::fs::create_dir_all(&config_home).expect("Failed to create XDG_CONFIG_HOME");

        let vault_dir = temp_path.join("open-keyring");

        Self {
            temp_dir,
            vault_dir,
        }
    }

    pub fn home_path(&self) -> PathBuf {
        self.temp_dir.path().to_path_buf()
    }

    pub fn xdg_data_home(&self) -> PathBuf {
        self.temp_dir.path().join("data")
    }

    pub fn xdg_config_home(&self) -> PathBuf {
        self.temp_dir.path().join("config")
    }

    /// Builds a Command with the test environment variables set.
    pub fn command(&self) -> Command {
        let binary_path = cargo::cargo_bin("ok");
        let mut cmd = Command::new(&binary_path);

        cmd.env("HOME", self.home_path());
        cmd.env("XDG_DATA_HOME", self.xdg_data_home());
        cmd.env("XDG_CONFIG_HOME", self.xdg_config_home());
        cmd.env("OAK_VAULT_DIR", &self.vault_dir);

        cmd.env("TERM", "xterm-256color");
        cmd.env("COLUMNS", "120");
        cmd.env("LINES", "30");

        cmd
    }
}

impl Default for TestEnv {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawns the `ok` binary in a PTY session with the test environment.
pub fn spawn_ok(env: &TestEnv) -> Result<OsSession, expectrl::Error> {
    let cmd = env.command();
    Session::spawn(cmd)
}

/// Sends text input to the PTY session.
pub fn send_text<S>(session: &mut S, text: &str) -> std::io::Result<()>
where
    S: Write,
{
    write!(session, "{}", text)?;
    session.flush()
}

/// Sends a special key sequence to the PTY session.
pub fn send_key<S>(session: &mut S, key: &str) -> std::io::Result<()>
where
    S: Write,
{
    let bytes: &[u8] = match key {
        "Enter" => b"\r",
        "Esc" => b"\x1b",
        "Tab" => b"\t",
        "Space" => b" ",
        "Ctrl+C" => b"\x03",
        "Ctrl+D" => b"\x04",
        "Backspace" => b"\x7f",
        "Up" => b"\x1b[A",
        "Down" => b"\x1b[B",
        "Left" => b"\x1b[D",
        "Right" => b"\x1b[C",
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Unknown key: {}", key),
            ));
        }
    };

    session.write_all(bytes)?;
    session.flush()
}

/// Waits until the PTY output contains the specified pattern.
pub fn wait_screen_contains<P, S>(
    session: &mut Session<P, S>,
    pattern: &str,
    timeout_secs: u64,
) -> Result<(), String>
where
    Session<P, S>: Expect,
    S: NonBlocking + std::io::Read,
{
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(timeout_secs);

    loop {
        if start.elapsed() >= timeout {
            return Err(format!(
                "Timeout after {}s waiting for pattern: '{}'\nCurrent screen:\n{}",
                timeout_secs,
                pattern,
                read_screen(session)
            ));
        }

        match session.check(pattern) {
            Ok(_) => return Ok(()),
            Err(_) => {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

/// Reads and normalizes the current PTY output, stripping ANSI escape sequences.
pub fn read_screen<P, S>(session: &mut Session<P, S>) -> String
where
    S: NonBlocking + std::io::Read,
{
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];

    loop {
        match session.try_read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    }

    let stripped = strip(&buffer);
    String::from_utf8_lossy(&stripped).to_string()
}

/// Asserts that the process exits cleanly with exit code 0.
///
/// Per docs/research/test/pyt-panic.md, we check exit code rather than
/// parsing PTY output for panic text (which is unreliable due to alternate
/// screen buffering and stdout/stderr multiplexing).
pub fn assert_clean_exit<P, S>(session: &mut Session<P, S>, timeout_secs: u64)
where
    S: std::io::Read + std::io::Write + NonBlocking,
    P: Healthcheck<Status = expectrl::process::unix::WaitStatus>,
    <P as Healthcheck>::Status: std::fmt::Debug,
{
    let timeout = Duration::from_secs(timeout_secs);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        if !session.is_alive().unwrap_or(true) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    if session.is_alive().unwrap_or(false) {
        panic!(
            "Process still running after timeout, screen:\n{}",
            read_screen(session)
        );
    }

    // Check exit status — exit code 101 = Rust panic
    if let Ok(status) = session.get_process().get_status() {
        match status {
            expectrl::process::unix::WaitStatus::Exited(_, code) => {
                assert_eq!(
                    code, 0,
                    "Process exited with code {} (101 = panic), screen:\n{}",
                    code,
                    read_screen(session)
                );
            }
            expectrl::process::unix::WaitStatus::Signaled(_, signal, _) => {
                panic!(
                    "Process was killed by signal {:?}, screen:\n{}",
                    signal,
                    read_screen(session)
                );
            }
            other => {
                panic!(
                    "Process has unexpected status: {:?}, screen:\n{}",
                    other,
                    read_screen(session)
                );
            }
        }
    }
}

/// Creates a test vault programmatically using the service layer.
///
/// This allows pre-creating vaults for unlock tests without going through the TUI.
pub fn create_test_vault(env: &TestEnv, password: &str) -> Result<(), String> {
    use oak_keyring::crypto::argon2::Argon2Params;
    use oak_keyring::crypto::bip39::{MnemonicLanguage, Passkey};
    use oak_keyring::crypto::keystore::KeyStore;
    use oak_keyring::types::SecureStr;

    let passkey = Passkey::generate(24, MnemonicLanguage::English)
        .map_err(|e| format!("Failed to generate mnemonic: {}", e))?;

    let seed = passkey
        .to_seed(None)
        .map_err(|e| format!("Failed to derive seed: {}", e))?;

    let sk_bytes = seed.to_secret_key();
    let cmk = SecureStr::new(password.to_string());
    let vault_dir = &env.vault_dir;

    std::fs::create_dir_all(&vault_dir)
        .map_err(|e| format!("Failed to create vault directory: {}", e))?;

    KeyStore::initialize(
        &vault_dir,
        sk_bytes,
        &cmk,
        &Argon2Params::low(),
        MnemonicLanguage::English,
    )
    .map_err(|e| format!("Failed to initialize vault: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_creates_temp_dir() {
        let env = TestEnv::new();
        assert!(env.temp_dir.path().exists());
        assert!(env.xdg_data_home().exists());
        assert!(env.xdg_config_home().exists());
    }

    #[test]
    fn test_env_vault_dir_path() {
        let env = TestEnv::new();
        assert_eq!(env.vault_dir, env.temp_dir.path().join("open-keyring"));
    }
}
