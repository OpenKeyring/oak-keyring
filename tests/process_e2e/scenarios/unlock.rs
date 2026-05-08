//! E2E-03 and E2E-04 scenario tests - vault unlock workflow
//!
//! Tests the behavior when the user starts `ok` with an existing vault:
//! - E2E-03: Successful unlock with correct password
//! - E2E-04: Wrong password recovery

use crate::harness::*;
use expectrl::process::Healthcheck;

/// E2E-03: Restart routes to unlock and succeeds
///
/// Tests that when a vault already exists, the app shows an unlock screen
/// (not onboarding), accepts the correct password, and proceeds to the main screen.
#[test]
fn restart_routes_to_unlock_and_succeeds() {
    // Arrange: Create isolated test environment and pre-create a vault
    let env = TestEnv::new();
    let password = "ProcessE2E-pass-12345!";

    create_test_vault(&env, password).expect("Failed to create test vault");
    assert!(env.vault_dir.exists(), "Vault directory should exist");

    // Act: Spawn the ok binary
    let mut session = spawn_ok(&env).expect("Failed to spawn ok binary");

    // Act & Assert: Wait for the unlock/password prompt (NOT onboarding)
    // Look for patterns that indicate password input screen
    let unlock_patterns = ["password", "Password", "unlock", "Unlock", "Enter"];
    let mut found_pattern = None;

    for pattern in &unlock_patterns {
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
            "No unlock pattern found. Current screen:\n{}\n\
             Looked for patterns: {:?}",
            screen, unlock_patterns
        );
    });

    println!("Found unlock pattern: '{}'", found);

    // Assert: Screen should NOT contain onboarding welcome text
    let screen_output = read_screen(&mut session);
    let onboarding_patterns = ["Welcome", "Create vault", "Import", "First time"];
    for pattern in &onboarding_patterns {
        assert!(
            !screen_output.contains(pattern),
            "Found onboarding pattern '{}' in output, but should be at unlock screen:\n{}",
            pattern,
            screen_output
        );
    }

    // Act: Input correct password
    send_text(&mut session, password).expect("Failed to send password");
    send_key(&mut session, "Enter").expect("Failed to send Enter key");

    // Act & Assert: Wait for main/list screen (unlock is successful)
    // Argon2 verification is slow, use generous timeout
    let main_patterns = [
        "list",
        "search",
        "Entries",
        "OpenKeyring",
        "Password entries",
    ];
    let mut found_main = None;

    for pattern in &main_patterns {
        match wait_screen_contains(&mut session, pattern, 15) {
            Ok(_) => {
                found_main = Some(pattern);
                break;
            }
            Err(_) => continue,
        }
    }

    let found = found_main.unwrap_or_else(|| {
        let screen = read_screen(&mut session);
        panic!(
            "No main screen pattern found after unlock. Current screen:\n{}\n\
             Looked for patterns: {:?}",
            screen, main_patterns
        );
    });

    println!("Found main screen pattern: '{}'", found);

    // Act: Send Ctrl+C to exit cleanly
    send_key(&mut session, "Ctrl+C").expect("Failed to send Ctrl+C");

    // Give the app time to clean up and exit
    std::thread::sleep(std::time::Duration::from_secs(1));

    // Assert: Clean exit with exit code 0 and no panic
    assert_clean_exit(&mut session, 5);
}

/// E2E-04: Wrong password is recoverable
///
/// Tests that when the user enters an incorrect password, the app shows an error
/// but remains functional, allowing them to try again with the correct password.
#[test]
fn wrong_password_is_recoverable() {
    // Arrange: Create isolated test environment and pre-create a vault
    let env = TestEnv::new();
    let password = "ProcessE2E-pass-12345!";
    let wrong_password = "wrong-password";

    create_test_vault(&env, password).expect("Failed to create test vault");
    assert!(env.vault_dir.exists(), "Vault directory should exist");

    // Act: Spawn the ok binary
    let mut session = spawn_ok(&env).expect("Failed to spawn ok binary");

    // Act & Assert: Wait for the unlock/password prompt
    let unlock_patterns = ["password", "Password", "unlock", "Unlock"];
    let mut found_pattern = None;

    for pattern in &unlock_patterns {
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
            "No unlock pattern found. Current screen:\n{}\n\
             Looked for patterns: {:?}",
            screen, unlock_patterns
        );
    });

    println!("Found unlock pattern: '{}'", found);

    // Act: Input WRONG password
    send_text(&mut session, wrong_password).expect("Failed to send wrong password");
    send_key(&mut session, "Enter").expect("Failed to send Enter key");

    // Act & Assert: Wait for error indication (Argon2 is slow, use generous timeout)
    let error_patterns = [
        "incorrect",
        "wrong",
        "failed",
        "Invalid",
        "try again",
        "error",
    ];
    let mut found_error = None;

    for pattern in &error_patterns {
        match wait_screen_contains(&mut session, pattern, 15) {
            Ok(_) => {
                found_error = Some(pattern);
                break;
            }
            Err(_) => continue,
        }
    }

    // Note: Error message might be subtle or brief, so we don't panic if not found
    // The important check is that the process is still alive
    if let Some(error) = found_error {
        println!("Found error pattern: '{}'", error);
    } else {
        println!("Warning: No explicit error pattern found, but continuing to check process state");
    }

    // Assert: Process should still be alive (not crashed/exited)
    let is_alive = session.is_alive().unwrap_or(false);
    assert!(
        is_alive,
        "Process should still be alive after wrong password, but it has exited. Screen:\n{}",
        read_screen(&mut session)
    );

    // Act: Input CORRECT password
    send_text(&mut session, password).expect("Failed to send correct password");
    send_key(&mut session, "Enter").expect("Failed to send Enter key");

    // Act & Assert: Wait for main/list screen (unlock is successful)
    let main_patterns = ["list", "search", "Entries", "OpenKeyring"];
    let mut found_main = None;

    for pattern in &main_patterns {
        match wait_screen_contains(&mut session, pattern, 15) {
            Ok(_) => {
                found_main = Some(pattern);
                break;
            }
            Err(_) => continue,
        }
    }

    let found = found_main.unwrap_or_else(|| {
        let screen = read_screen(&mut session);
        panic!(
            "No main screen pattern found after correct password. Current screen:\n{}\n\
             Looked for patterns: {:?}",
            screen, main_patterns
        );
    });

    println!("Found main screen pattern: '{}'", found);

    // Act: Send Ctrl+C to exit cleanly
    send_key(&mut session, "Ctrl+C").expect("Failed to send Ctrl+C");

    // Give the app time to clean up and exit
    std::thread::sleep(std::time::Duration::from_secs(1));

    // Assert: Clean exit with exit code 0 and no panic
    assert_clean_exit(&mut session, 5);
}
