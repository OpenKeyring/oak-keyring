use bip39::rand::seq::SliceRandom;
use chacha20poly1305::aead::{rand_core::RngCore, OsRng};

use crate::types::sensitive::SecureStr;

const LOWERCASE: &[u8] = b"abcdefghijkmnpqrstuvwxyz";
const UPPERCASE: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
const DIGITS: &[u8] = b"23456789";
const SPECIAL: &[u8] = b"!#$*+-=?@^_~";

fn random_from_slice(choices: &[u8]) -> u8 {
    let idx = (OsRng.next_u32() as usize) % choices.len();
    choices[idx]
}

pub fn generate_random_password(length: usize) -> Result<SecureStr, String> {
    if !(4..=128).contains(&length) {
        return Err("Password length must be between 4 and 128".into());
    }
    let mut charset = Vec::from(LOWERCASE);
    charset.extend_from_slice(UPPERCASE);
    charset.extend_from_slice(DIGITS);
    charset.extend_from_slice(SPECIAL);

    let mut password = String::with_capacity(length);
    for _ in 0..length {
        password.push(random_from_slice(&charset) as char);
    }
    Ok(SecureStr::new(password))
}

pub fn generate_random_password_with_policy(
    length: usize,
    min_digits: u8,
    min_special: u8,
    min_lowercase: u8,
    min_uppercase: u8,
) -> Result<SecureStr, String> {
    if !(4..=128).contains(&length) {
        return Err("Password length must be between 4 and 128".into());
    }
    let required = min_digits as usize
        + min_special as usize
        + min_lowercase as usize
        + min_uppercase as usize;
    if required > length {
        return Err("Policy requirements exceed password length".into());
    }

    let mut password = Vec::with_capacity(length);
    for _ in 0..min_lowercase {
        password.push(random_from_slice(LOWERCASE));
    }
    for _ in 0..min_uppercase {
        password.push(random_from_slice(UPPERCASE));
    }
    for _ in 0..min_digits {
        password.push(random_from_slice(DIGITS));
    }
    for _ in 0..min_special {
        password.push(random_from_slice(SPECIAL));
    }

    let mut charset = Vec::from(LOWERCASE);
    charset.extend_from_slice(UPPERCASE);
    charset.extend_from_slice(DIGITS);
    charset.extend_from_slice(SPECIAL);

    for _ in required..length {
        password.push(random_from_slice(&charset));
    }

    password.shuffle(&mut OsRng);
    let password = String::from_utf8(password).map_err(|e| e.to_string())?;
    Ok(SecureStr::new(password))
}

const WORDS: &[&str] = &[
    "apple", "brave", "cloud", "dream", "eagle", "flame", "grape", "heart", "ivory", "jewel",
    "kite", "lemon", "maple", "noble", "ocean", "pearl", "quest", "river", "stone", "tiger",
    "ultra", "vivid", "whale", "xenon", "yacht", "zebra", "amber", "blaze", "coral", "delta",
    "ember", "frost",
];

pub fn generate_memorable_password(word_count: usize) -> Result<SecureStr, String> {
    generate_memorable_password_with_options(word_count, "-", false)
}

pub fn generate_memorable_password_with_separator(
    word_count: usize,
    separator: &str,
) -> Result<SecureStr, String> {
    generate_memorable_password_with_options(word_count, separator, false)
}

pub fn generate_memorable_password_with_options(
    word_count: usize,
    separator: &str,
    capitalize: bool,
) -> Result<SecureStr, String> {
    if !(3..=12).contains(&word_count) {
        return Err("Word count must be between 3 and 12".into());
    }
    let mut words = Vec::with_capacity(word_count);
    for _ in 0..word_count {
        let idx = (OsRng.next_u32() as usize) % WORDS.len();
        let word = if capitalize {
            capitalize_word(WORDS[idx])
        } else {
            WORDS[idx].to_string()
        };
        words.push(word);
    }
    Ok(SecureStr::new(words.join(separator)))
}

fn capitalize_word(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

pub fn generate_pin(length: usize) -> Result<SecureStr, String> {
    if !(4..=16).contains(&length) {
        return Err("PIN length must be between 4 and 16".into());
    }
    let mut pin = String::with_capacity(length);
    for _ in 0..length {
        pin.push(random_from_slice(DIGITS) as char);
    }
    Ok(SecureStr::new(pin))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAFE_CHARSET: &str =
        "abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789!#$*+-=?@^_~";

    #[test]
    fn test_random_password_length() {
        for len in [4, 8, 16, 64, 128] {
            let pw = generate_random_password(len).unwrap();
            assert_eq!(pw.get().len(), len);
        }
    }

    #[test]
    fn test_random_password_range_4_128() {
        assert!(generate_random_password(4).is_ok());
        assert!(generate_random_password(128).is_ok());
        assert!(generate_random_password(3).is_err());
        assert!(generate_random_password(129).is_err());
    }

    #[test]
    fn test_random_password_chars_from_safe_charset() {
        let pw = generate_random_password(128).unwrap();
        for c in pw.get().chars() {
            assert!(SAFE_CHARSET.contains(c), "char '{c}' not in safe charset");
        }
        let ambiguous = ['I', 'O', 'l', '0', '1'];
        for c in pw.get().chars() {
            assert!(!ambiguous.contains(&c), "ambiguous char '{c}' found");
        }
    }

    #[test]
    fn test_random_password_different_each_time() {
        let a = generate_random_password(64).unwrap();
        let b = generate_random_password(64).unwrap();
        assert_ne!(a.get(), b.get(), "two random passwords should differ");
    }

    #[test]
    fn test_policy_password_meets_requirements() {
        let pw = generate_random_password_with_policy(20, 2, 2, 2, 2).unwrap();
        let s = pw.get();
        assert_eq!(s.len(), 20);

        let digit_count = s.chars().filter(|c| c.is_ascii_digit()).count();
        let special_count = s.chars().filter(|c| "!#$*+-=?@^_~".contains(*c)).count();
        let lower_count = s.chars().filter(|c| c.is_ascii_lowercase()).count();
        let upper_count = s.chars().filter(|c| c.is_ascii_uppercase()).count();

        assert!(digit_count >= 2, "digits: {digit_count} < 2");
        assert!(special_count >= 2, "specials: {special_count} < 2");
        assert!(lower_count >= 2, "lowercase: {lower_count} < 2");
        assert!(upper_count >= 2, "uppercase: {upper_count} < 2");
    }

    #[test]
    fn test_policy_password_exceeds_length_fails() {
        let result = generate_random_password_with_policy(8, 3, 3, 3, 3);
        assert!(result.is_err());
    }

    #[test]
    fn test_memorable_password_word_count() {
        let pw = generate_memorable_password(4).unwrap();
        let words: Vec<&str> = pw.get().split('-').collect();
        assert_eq!(words.len(), 4);
    }

    #[test]
    fn test_memorable_password_range_3_12() {
        assert!(generate_memorable_password(3).is_ok());
        assert!(generate_memorable_password(12).is_ok());
        assert!(generate_memorable_password(2).is_err());
        assert!(generate_memorable_password(13).is_err());
    }

    #[test]
    fn test_pin_digits_only() {
        let pin = generate_pin(8).unwrap();
        for c in pin.get().chars() {
            assert!(c.is_ascii_digit(), "PIN char '{c}' is not a digit");
        }
        let valid_digits: Vec<char> = "23456789".chars().collect();
        for c in pin.get().chars() {
            assert!(valid_digits.contains(&c), "digit '{c}' not in 2-9 range");
        }
    }

    #[test]
    fn test_pin_range_4_16() {
        assert!(generate_pin(4).is_ok());
        assert!(generate_pin(16).is_ok());
        assert!(generate_pin(3).is_err());
        assert!(generate_pin(17).is_err());
    }

    #[test]
    fn test_memorable_password_capitalize_false() {
        let pw = generate_memorable_password_with_options(4, "-", false).unwrap();
        let words: Vec<&str> = pw.get().split('-').collect();
        assert_eq!(words.len(), 4);
        // All words should be lowercase
        for word in words {
            assert_eq!(
                word,
                word.to_lowercase(),
                "word '{word}' should be lowercase"
            );
        }
    }

    #[test]
    fn test_memorable_password_capitalize_true() {
        let pw = generate_memorable_password_with_options(4, "-", true).unwrap();
        let words: Vec<&str> = pw.get().split('-').collect();
        assert_eq!(words.len(), 4);
        // Each word should start with uppercase
        for word in words {
            let first_char = word.chars().next().unwrap();
            assert!(
                first_char.is_uppercase(),
                "first char of '{word}' should be uppercase"
            );
            // Rest should be lowercase
            let rest: String = word.chars().skip(1).collect();
            assert_eq!(
                rest,
                rest.to_lowercase(),
                "rest of '{word}' should be lowercase"
            );
        }
    }

    #[test]
    fn test_memorable_password_backward_compatible() {
        // Original function should still work (default capitalize=false)
        let pw = generate_memorable_password(4).unwrap();
        let words: Vec<&str> = pw.get().split('-').collect();
        assert_eq!(words.len(), 4);
        // All words should be lowercase (backward compatible)
        for word in words {
            assert_eq!(
                word,
                word.to_lowercase(),
                "word '{word}' should be lowercase"
            );
        }
    }

    #[test]
    fn test_memorable_password_separator_with_capitalize() {
        let pw = generate_memorable_password_with_options(3, "_", true).unwrap();
        let words: Vec<&str> = pw.get().split('_').collect();
        assert_eq!(words.len(), 3);
        // Check that separator is used correctly with capitalization
        assert!(pw.get().contains('_'));
        assert!(!pw.get().contains('-'));
        // Each word should be capitalized
        for word in words {
            let first_char = word.chars().next().unwrap();
            assert!(first_char.is_uppercase());
        }
    }

    #[test]
    fn test_capitalize_word_helper() {
        assert_eq!(capitalize_word("apple"), "Apple");
        assert_eq!(capitalize_word("brave"), "Brave");
        assert_eq!(capitalize_word("a"), "A");
        assert_eq!(capitalize_word(""), "");
        assert_eq!(capitalize_word("abc"), "Abc");
    }
}
