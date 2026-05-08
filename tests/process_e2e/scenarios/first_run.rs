//! E2E-01 scenario tests - first-run initialization experience
//!
//! Tests the behavior when the user starts `ok` for the first time with no existing vault.
//!
//! Strategy per docs/research/test/pyt-panic.md:
//! - PTY tests verify screen content and exit code (not panic text matching)
//! - Exit code 101 = Rust panic; exit code 0 = clean shutdown
//! - Panic pattern matching in PTY output is unreliable (alternate screen swallows it)

use crate::harness::*;

/// Verify `ok` starts without panicking when vault directory doesn't exist.
///
/// Spawns via PTY, confirms welcome screen renders, then exits cleanly (code 0).
/// Exit code 101 would indicate a Rust panic during startup.
#[test]
fn fresh_start_no_panic() {
    let env = TestEnv::new();
    assert!(!env.vault_dir.exists(), "Vault directory should not exist initially");

    let mut session = spawn_ok(&env).expect("Failed to spawn ok binary");

    // Verify welcome screen appears — this proves the TUI rendered successfully
    let welcome_patterns = ["OpenKeyring", "Welcome", "Create", "Import"];
    let mut found = None;
    for pattern in &welcome_patterns {
        if wait_screen_contains(&mut session, pattern, 5).is_ok() {
            found = Some(pattern);
            break;
        }
    }
    found.unwrap_or_else(|| {
        panic!(
            "No welcome pattern found. Screen:\n{}\nLooked for: {:?}",
            read_screen(&mut session),
            welcome_patterns
        )
    });

    // Exit cleanly — assert_clean_exit verifies exit code 0 and no signal
    let _ = send_key(&mut session, "Ctrl+C");
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert_clean_exit(&mut session, 5);
}
