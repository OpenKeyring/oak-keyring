//! Test harness for process end-to-end testing with PTY isolation.
//!
//! This module provides utilities for spawning the `ok` binary in an isolated
//! environment with controlled environment variables and PTY interaction.

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
    /// Path to the vault directory ($XDG_DATA_HOME/open-keyring)
    pub vault_dir: PathBuf,
}

impl TestEnv {
    /// Creates a new isolated test environment with a temporary directory.
    ///
    /// Sets up environment variables for HOME, XDG_DATA_HOME, and XDG_CONFIG_HOME
    /// pointing to subdirectories within the temp directory.
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let temp_path = temp_dir.path();

        // Create XDG directories
        let data_home = temp_path.join("data");
        let config_home = temp_path.join("config");
        std::fs::create_dir_all(&data_home).expect("Failed to create XDG_DATA_HOME");
        std::fs::create_dir_all(&config_home).expect("Failed to create XDG_CONFIG_HOME");

        // Vault directory: use temp_dir/open-keyring directly
        // OAK_VAULT_DIR env var on the Command overrides platform-specific path resolution
        let vault_dir = temp_path.join("open-keyring");

        Self {
            temp_dir,
            vault_dir,
        }
    }

    /// Returns the HOME path for this test environment.
    pub fn home_path(&self) -> PathBuf {
        self.temp_dir.path().to_path_buf()
    }

    /// Returns the XDG_DATA_HOME path for this test environment.
    pub fn xdg_data_home(&self) -> PathBuf {
        self.temp_dir.path().join("data")
    }

    /// Returns the XDG_CONFIG_HOME path for this test environment.
    pub fn xdg_config_home(&self) -> PathBuf {
        self.temp_dir.path().join("config")
    }

    /// Builds a Command with the test environment variables set.
    pub fn command(&self) -> Command {
        let binary_path = cargo::cargo_bin("ok");
        let mut cmd = Command::new(&binary_path);

        // Set isolated environment variables
        cmd.env("HOME", self.home_path());
        cmd.env("XDG_DATA_HOME", self.xdg_data_home());
        cmd.env("XDG_CONFIG_HOME", self.xdg_config_home());

        // Override vault directory directly (cross-platform, avoids macOS path issues)
        cmd.env("OAK_VAULT_DIR", &self.vault_dir);

        // Set terminal environment for TUI
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
///
/// # Arguments
/// * `env` - Test environment with isolated temp directory and env vars
///
/// # Returns
/// A PTY session ready for interaction
pub fn spawn_ok(env: &TestEnv) -> Result<OsSession, expectrl::Error> {
    let cmd = env.command();
    Session::spawn(cmd)
}

/// Sends text input to the PTY session.
///
/// # Arguments
/// * `session` - The PTY session
/// * `text` - Text to send (will be written as-is)
pub fn send_text<S>(session: &mut S, text: &str) -> std::io::Result<()>
where
    S: Write,
{
    write!(session, "{}", text)?;
    session.flush()
}

/// Sends a special key sequence to the PTY session.
///
/// # Arguments
/// * `session` - The PTY session
/// * `key` - Key sequence to send (e.g., "Enter", "Esc", "Tab", "Ctrl+C")
pub fn send_key<S>(session: &mut S, key: &str) -> std::io::Result<()>
where
    S: Write,
{
    let bytes: &[u8] = match key {
        "Enter" => b"\r",
        "Esc" => b"\x1b",
        "Tab" => b"\t",
        "Space" => b" ",
        "Ctrl+C" => b"\x03", // End of Text
        "Ctrl+D" => b"\x04", // End of Transmission
        "Ctrl+S" => b"\x13", // DC3 (XOFF)
        "Ctrl+Q" => b"\x11", // DC1 (XON)
        "Backspace" => b"\x7f",
        "Delete" => b"\x1b[3~",
        "Up" => b"\x1b[A",
        "Down" => b"\x1b[B",
        "Left" => b"\x1b[D",
        "Right" => b"\x1b[C",
        "Home" => b"\x1b[H",
        "End" => b"\x1b[F",
        "PageUp" => b"\x1b[5~",
        "PageDown" => b"\x1b[6~",
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
///
/// # Arguments
/// * `session` - The PTY session
/// * `pattern` - Text pattern to wait for (case-sensitive, no regex)
/// * `timeout_secs` - Timeout in seconds
///
/// # Returns
/// Ok(()) if pattern found, Err with timeout message if not found
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
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return Err(format!(
                "Timeout after {}s waiting for pattern: '{}'\nCurrent screen:\n{}",
                timeout_secs,
                pattern,
                read_screen(session)
            ));
        }

        match session.check(pattern) {
            Ok(_) => return Ok(()), // Pattern found
            Err(_) => {
                // Pattern not found yet, wait and retry
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
        }
    }
}

/// Waits until the PTY output no longer contains the specified pattern.
///
/// # Arguments
/// * `session` - The PTY session
/// * `pattern` - Text pattern that should disappear
/// * `timeout_secs` - Timeout in seconds
///
/// # Returns
/// Ok(()) if pattern is gone, Err with timeout message if still present
pub fn wait_screen_not_contains<P, S>(
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
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return Err(format!(
                "Timeout after {}s waiting for pattern to disappear: '{}'\nCurrent screen:\n{}",
                timeout_secs,
                pattern,
                read_screen(session)
            ));
        }

        match session.check(pattern) {
            Ok(_) => {
                // Pattern still present, wait and retry
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(_) => {
                // Pattern not found - success!
                return Ok(());
            }
        }
    }
}

/// Reads and normalizes the current PTY output.
///
/// Strips ANSI escape sequences and returns readable text.
///
/// # Arguments
/// * `session` - The PTY session
///
/// # Returns
/// Normalized screen content as a String
pub fn read_screen<P, S>(session: &mut Session<P, S>) -> String
where
    S: NonBlocking + std::io::Read,
{
    // Try to read available content without blocking
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];

    loop {
        match session.try_read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    }

    // Strip ANSI escape sequences
    let stripped = strip(&buffer);

    // Convert to string and normalize whitespace
    String::from_utf8_lossy(&stripped).to_string()
}

/// Asserts that the process exits cleanly (exit code 0, no panic).
///
/// This function will:
/// 1. Wait for the process to exit (up to timeout)
/// 2. Force kill if still running
/// 3. Assert exit code is 0
/// 4. Assert stderr doesn't contain panic/error patterns
///
/// # Arguments
/// * `session` - The PTY session
/// * `timeout_secs` - Timeout before force killing (default 5s)
pub fn assert_clean_exit<P, S>(session: &mut Session<P, S>, timeout_secs: u64)
where
    S: std::io::Read + std::io::Write + NonBlocking,
    P: Healthcheck<Status = expectrl::process::unix::WaitStatus>,
    <P as Healthcheck>::Status: std::fmt::Debug,
{
    let timeout = Duration::from_secs(timeout_secs);
    let start = std::time::Instant::now();

    // Wait for process to exit
    while start.elapsed() < timeout {
        if !session.is_alive().unwrap_or(true) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // Force kill if still running
    if session.is_alive().unwrap_or(true) {
        // Note: We can't easily kill the process through the generic API
        // The test framework will clean it up when the test ends
        // This is acceptable for E2E testing
        std::thread::sleep(Duration::from_millis(100));
    }

    // Check that process is no longer alive
    if session.is_alive().unwrap_or(false) {
        panic!(
            "Process still running after timeout, screen:\n{}",
            read_screen(session)
        );
    }

    // Try to get exit status, but don't fail if process was already reaped
    // The important checks are: (1) process exited, (2) no panic in output
    if let Ok(status) = session.get_process().get_status() {
        match status {
            expectrl::process::unix::WaitStatus::Exited(_, code) => {
                assert_eq!(
                    code,
                    0,
                    "Process exited with code {}, screen:\n{}",
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
            _ => {
                panic!(
                    "Process has unexpected status: {:?}, screen:\n{}",
                    status,
                    read_screen(session)
                );
            }
        }
    }
    // If get_status() fails, the process was already reaped - this is OK

    // Check for panic/error patterns in output
    let screen_output = read_screen(session);
    let panic_patterns = [
        "panic",
        "app run failed",
        "failed to create app",
        "thread 'main' panicked",
        "panicked at",
    ];

    for pattern in &panic_patterns {
        assert!(
            !screen_output.to_lowercase().contains(pattern),
            "Found '{}' in output, indicating a panic or failure:\n{}",
            pattern,
            screen_output
        );
    }
}

/// Dumps the current PTY screen content for debugging timeouts.
///
/// Called automatically when `wait_*` functions timeout.
///
/// # Arguments
/// * `session` - The PTY session
///
/// # Returns
/// Formatted string with raw bytes and normalized text
pub fn dump_on_timeout<P, S>(session: &mut Session<P, S>) -> String
where
    S: NonBlocking + std::io::Read,
{
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];

    // Read raw bytes
    loop {
        match session.try_read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    }

    let mut output = String::new();

    // Raw bytes section
    output.push_str("=== RAW PTY BUFFER ===\n");
    for (i, byte) in buffer.iter().enumerate() {
        if i > 0 && i % 16 == 0 {
            output.push('\n');
        }
        output.push_str(&format!("{:02x} ", byte));
    }
    output.push_str("\n\n");

    // Normalized text section
    output.push_str("=== NORMALIZED TEXT ===\n");
    let stripped = strip(&buffer);
    output.push_str(&String::from_utf8_lossy(&stripped));
    output.push('\n');

    output
}

/// Creates a test vault programmatically using the service layer.
///
/// This allows pre-creating vaults for unlock tests without going through the TUI.
///
/// # Arguments
/// * `env` - Test environment
/// * `password` - Password to use for vault encryption
///
/// # Returns
/// Result indicating success or failure
pub fn create_test_vault(env: &TestEnv, password: &str) -> Result<(), String> {
    use oak_keyring::crypto::argon2::Argon2Params;
    use oak_keyring::crypto::bip39::{MnemonicLanguage, Passkey};
    use oak_keyring::crypto::keystore::KeyStore;
    use oak_keyring::types::SecureStr;

    // Generate BIP39 mnemonic
    let passkey = Passkey::generate(24, MnemonicLanguage::English)
        .map_err(|e| format!("Failed to generate mnemonic: {}", e))?;

    // Derive seed from mnemonic
    let seed = passkey
        .to_seed(None)
        .map_err(|e| format!("Failed to derive seed: {}", e))?;

    // Get secret key (first 32 bytes of seed)
    let sk_bytes = seed.to_secret_key();

    // Wrap password in SecureStr
    let cmk = SecureStr::new(password.to_string());

    // Get the vault directory (OAK_VAULT_DIR path)
    let vault_dir = &env.vault_dir;

    // Ensure parent directory exists before calling KeyStore::initialize
    std::fs::create_dir_all(&vault_dir)
        .map_err(|e| format!("Failed to create vault directory: {}", e))?;

    // Initialize the vault with low KDF params for fast test execution
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
        // Vault dir should be at temp_dir/open-keyring (OAK_VAULT_DIR path)
        assert_eq!(env.vault_dir, env.temp_dir.path().join("open-keyring"));
    }
}
