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
    if !(3..=12).contains(&word_count) {
        return Err("Word count must be between 3 and 12".into());
    }
    let mut words = Vec::with_capacity(word_count);
    for _ in 0..word_count {
        let idx = (OsRng.next_u32() as usize) % WORDS.len();
        words.push(WORDS[idx]);
    }
    Ok(SecureStr::new(words.join("-")))
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
