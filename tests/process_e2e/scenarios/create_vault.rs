//! E2E-02 scenario tests - vault creation workflow
//!
//! Tests the complete vault creation flow from first run to main screen.
//! Currently partial: navigates to recovery screen only.
//! Full implementation deferred due to PTY output buffering limitations
//! preventing reliable recovery word parsing.

use crate::harness::*;

#[test]
#[ignore = "Recovery word parsing via PTY is unstable; use create_test_vault() fixture instead"]
fn create_vault_from_first_run() {
    // Arrange: Create isolated test environment (empty vault directory)
    let env = TestEnv::new();
    assert!(
        !env.vault_dir.exists(),
        "Vault directory should not exist initially"
    );

    // Act: Spawn the ok binary (OAK_VAULT_DIR is set by TestEnv::command())
    let mut session = spawn_ok(&env).expect("Failed to spawn ok binary");

    // Act & Assert: Wait for the onboarding/welcome screen
    let welcome_patterns = ["OpenKeyring", "Welcome", "Create", "Import"];
    let mut found_pattern = None;

    for pattern in &welcome_patterns {
        match wait_screen_contains(&mut session, pattern, 10) {
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

    // Act: Select "Create new" option with Enter key
    let _ = send_key(&mut session, "Enter");
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Act: Confirm default vault path with Enter key
    let _ = send_key(&mut session, "Enter");
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Act & Assert: Wait for recovery phrase display screen
    // This screen shows the BIP39 mnemonic words
    // Look for specific text that indicates the recovery phrase screen (not help text)
    let recovery_patterns = ["Keep it safe", "24 words", "recovery phrase"];
    let mut found_recovery = None;

    for pattern in &recovery_patterns {
        match wait_screen_contains(&mut session, pattern, 15) {
            Ok(_) => {
                found_recovery = Some(pattern);
                break;
            }
            Err(_) => continue,
        }
    }

    let recovery_found = found_recovery.unwrap_or_else(|| {
        let screen = read_screen(&mut session);
        panic!(
            "No recovery phrase pattern found. Current screen:\n{}\n\
             Looked for patterns: {:?}",
            screen, recovery_patterns
        );
    });

    println!("Found recovery pattern: '{}'", recovery_found);

    // Wait for the screen to stabilize
    std::thread::sleep(std::time::Duration::from_secs(1));

    // NOTE: Recovery word parsing is currently unstable due to PTY buffering issues.
    // Full vault creation through TUI navigation is deferred.
    // Use create_test_vault() fixture for unlock tests (E2E-03/04).

    // Try to exit cleanly — ignore errors if process already exited
    let _ = send_key(&mut session, "Esc");
    std::thread::sleep(std::time::Duration::from_millis(500));
    let _ = send_key(&mut session, "Ctrl+C");
    std::thread::sleep(std::time::Duration::from_millis(500));

    println!("Test completed - basic navigation flow validated");
    println!("TODO: Implement full vault creation flow with recovery word parsing");
}

// Helper functions for recovery word parsing (not yet used)
// These are kept for future implementation when PTY buffering issues are resolved

/// Extracts mnemonic words from the recovery phrase display screen.
///
/// Parses the screen content to find numbered word patterns (e.g., "1. abandon", "2. ability").
/// Returns a vector of 24 words in order.
#[allow(dead_code)]
fn extract_mnemonic_words(screen: &str) -> Vec<String> {
    let mut words = Vec::new();

    // The screen typically shows words in a grid format:
    // 1. word1    2. word2    3. word3
    // 4. word4    5. word5    6. word6
    // ...

    // Look for patterns like "1. word", "2. word", etc.
    // We'll use a regex-like approach to find numbered words

    for line in screen.lines() {
        // Look for patterns like "1. word", "12. word", etc.
        let mut parts = line.split_whitespace();
        while let Some(part) = parts.next() {
            // Check if this part is a number followed by a period
            if part.ends_with('.') {
                let num_str = &part[..part.len() - 1];
                if let Ok(num) = num_str.parse::<usize>() {
                    // This is a numbered position, next token should be the word
                    if let Some(word) = parts.next() {
                        // Ensure we have enough capacity
                        if num > words.len() {
                            words.resize(num, String::new());
                        }
                        words[num - 1] = word.to_string();
                    }
                }
            }
        }
    }

    // Filter out empty strings and validate
    let words: Vec<String> = words.into_iter().filter(|w| !w.is_empty()).collect();

    // BIP39 typically uses 24 words
    if words.is_empty() {
        // Fallback: try to find words in different formats
        // Some screens might show "1 word1 2 word2" without periods
        extract_words_fallback(screen)
    } else {
        words
    }
}

/// Fallback method to extract words when the primary parsing fails.
///
/// Tries alternative patterns like "1 word1", "2 word2" (without periods).
#[allow(dead_code)]
fn extract_words_fallback(screen: &str) -> Vec<String> {
    let mut words = Vec::new();

    for line in screen.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let mut i = 0;

        while i < tokens.len() {
            // Try to parse as a number
            if let Ok(num) = tokens[i].parse::<usize>() {
                // Check if next token exists and looks like a word
                if i + 1 < tokens.len() {
                    let word = tokens[i + 1];
                    // Words are typically lowercase letters
                    if word.chars().all(|c| c.is_ascii_lowercase()) && word.len() > 2 {
                        if num > words.len() {
                            words.resize(num, String::new());
                        }
                        words[num - 1] = word.to_string();
                        i += 2; // Skip the word we just consumed
                        continue;
                    }
                }
            }
            i += 1;
        }
    }

    words
}

/// Parses the verification screen to find which word positions are requested.
///
/// The verification screen typically shows prompts like:
/// "Enter word #3:" or "Verify word 7"
///
/// Returns a vector of requested word positions (1-indexed).
#[allow(dead_code)]
fn parse_verification_positions(screen: &str) -> Vec<usize> {
    let mut positions = Vec::new();

    // Look for patterns like "word #3", "word 7", "#3:", etc.
    for line in screen.lines() {
        // Find all numbers in the line that might be word positions
        let tokens: Vec<&str> = line.split_whitespace().collect();
        for (i, token) in tokens.iter().enumerate() {
            // Check if this is a number
            if let Ok(num) = token.parse::<usize>() {
                // Check context - should be related to "word" or position
                // Look at previous and next tokens for context
                let prev = if i > 0 { tokens[i - 1] } else { "" };
                let next = if i + 1 < tokens.len() {
                    tokens[i + 1]
                } else {
                    ""
                };

                // Check if this looks like a word position reference
                let is_word_position = prev.contains("word")
                    || prev.contains("Word")
                    || next.contains("word")
                    || next.contains("Word")
                    || line.contains('#')
                    || line.contains("position")
                    || line.contains("Position");

                if is_word_position && num >= 1 && num <= 24 {
                    positions.push(num);
                }
            }
        }
    }

    // Remove duplicates while preserving order
    let mut unique_positions = Vec::new();
    for pos in positions {
        if !unique_positions.contains(&pos) {
            unique_positions.push(pos);
        }
    }

    unique_positions
}
