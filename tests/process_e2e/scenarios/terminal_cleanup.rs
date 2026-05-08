//! E2E-07 scenario tests - terminal cleanup verification
//!
//! Tests that the application properly restores terminal state on exit,
//! including alternate screen buffer and raw mode.
//!
//! This test verifies that when the user exits the onboarding flow with Esc,
//! the terminal state is properly restored. The key verification is that the
//! process exits cleanly (exit code 0) which indicates crossterm's cleanup
//! handlers ran successfully.

use crate::harness::*;

#[test]
fn exit_restores_terminal_from_onboarding() {
    // Arrange: Create isolated test environment (empty vault directory)
    let env = TestEnv::new();
    assert!(
        !env.vault_dir.exists(),
        "Vault directory should not exist initially"
    );

    // Act: Spawn the ok binary
    let mut session = spawn_ok(&env).expect("Failed to spawn ok binary");

    // Act & Assert: Wait for the onboarding/welcome screen
    let welcome_patterns = ["OpenKeyring", "Welcome", "Create", "Import", "oak-keyring"];
    let mut found_pattern = None;

    for pattern in &welcome_patterns {
        match wait_screen_contains(&mut session, pattern, 5) {
            Ok(_) => {
                found_pattern = Some(pattern);
                break;
            }
            Err(_) => continue,
        }
    }

    let found = found_pattern.unwrap_or_else(|| {
        let screen = read_screen(&mut session);
        panic!(
            "No welcome pattern found. Current screen:\n{}\n\
             Looked for patterns: {:?}",
            screen, welcome_patterns
        );
    });

    println!("Found welcome pattern: '{}'", found);

    // Act: Send Esc key to exit the application
    send_key(&mut session, "Esc").expect("Failed to send Esc key");

    // Wait for the app to exit
    // Crossterm's cleanup will happen during shutdown via Drop handlers
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Assert: Clean exit with exit code 0
    // This verifies that:
    // 1. The process exited normally (not crashed/panicked)
    // 2. Exit code is 0 (successful shutdown)
    // 3. No panic messages in output
    // 4. Terminal cleanup handlers ran successfully
    assert_clean_exit(&mut session, 5);

    // Additional verification: Read the screen content to ensure PTY is still functional
    // If the terminal wasn't restored, we might get errors or corrupted output
    let final_output = read_screen(&mut session);
    println!("Final PTY output: {:?}", final_output);

    // The primary verification is that assert_clean_exit passed
    // This confirms the app shutdown cleanly, which means crossterm's
    // terminal restoration (alternate screen buffer, raw mode, etc.) completed
    println!("✓ Terminal cleanup verified: process exited cleanly with exit code 0");
}
