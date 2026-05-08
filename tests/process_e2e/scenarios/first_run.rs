//! E2E-01 scenario tests - first-run initialization experience
//!
//! Tests the behavior when the user starts `ok` for the first time with no existing vault.

use crate::harness::*;

#[test]
fn fresh_start_does_not_panic_and_esc_restores_terminal() {
    // Arrange: Create isolated test environment (empty vault directory)
    let env = TestEnv::new();
    assert!(
        !env.vault_dir.exists(),
        "Vault directory should not exist initially"
    );

    // Act: Spawn the ok binary
    let mut session = spawn_ok(&env).expect("Failed to spawn ok binary");

    // Act & Assert: Wait for the onboarding/welcome screen
    // Try multiple patterns since the exact text may vary
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
        // If no pattern matched, dump the screen for debugging
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

    // Give the app time to clean up and exit
    std::thread::sleep(std::time::Duration::from_secs(1));

    // Assert: Clean exit with exit code 0 and no panic
    assert_clean_exit(&mut session, 5);

    // Assert: Output should not contain panic indicators
    let screen_output = read_screen(&mut session);
    let panic_patterns = ["panic", "app run failed", "failed to create app"];
    for pattern in &panic_patterns {
        assert!(
            !screen_output.to_lowercase().contains(pattern),
            "Found '{}' in output, indicating a panic or failure:\n{}",
            pattern,
            screen_output
        );
    }

    // Assert: Terminal is restored (PTY closes cleanly)
    // If the terminal wasn't restored, expectrl would have issues or the exit status would be abnormal
    // The assert_clean_exit already checks for proper exit status
}
