use std::fmt;

use zeroize::Zeroize;

use crate::types::SecureStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryWordsError {
    InvalidWordCount { expected: usize, actual: usize },
    EmptyWord { index: usize },
}

pub struct RecoveryWords {
    inner: Vec<String>,
}

impl RecoveryWords {
    pub const WORD_COUNT: usize = 24;

    pub fn new(mut words: Vec<String>) -> Result<Self, RecoveryWordsError> {
        if words.len() != Self::WORD_COUNT {
            let actual = words.len();
            zeroize_words(&mut words);
            return Err(RecoveryWordsError::InvalidWordCount {
                expected: Self::WORD_COUNT,
                actual,
            });
        }

        if let Some(index) = words.iter().position(String::is_empty) {
            zeroize_words(&mut words);
            return Err(RecoveryWordsError::EmptyWord { index });
        }

        Ok(Self { inner: words })
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn as_slice(&self) -> &[String] {
        &self.inner
    }

    pub fn word(&self, index: usize) -> Option<&str> {
        self.inner.get(index).map(String::as_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.inner.iter().map(String::as_str)
    }

    pub fn to_phrase_secure(&self) -> SecureStr {
        SecureStr::new(self.inner.join(" "))
    }

    pub fn duplicate_for_command(&self) -> Result<Self, RecoveryWordsError> {
        // Audited escape hatch: preserves retry-capable command dispatch without exposing a raw Vec boundary.
        Self::new(self.inner.clone())
    }
}

fn zeroize_words(words: &mut [String]) {
    words.iter_mut().for_each(String::zeroize);
}

impl Zeroize for RecoveryWords {
    fn zeroize(&mut self) {
        zeroize_words(&mut self.inner);
        self.inner.clear();
    }
}

impl Drop for RecoveryWords {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl fmt::Debug for RecoveryWords {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecoveryWords")
            .field("len", &self.inner.len())
            .field("words", &"***REDACTED***")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words() -> Vec<String> {
        (0..RecoveryWords::WORD_COUNT)
            .map(|i| format!("word{i}"))
            .collect()
    }

    #[test]
    fn recovery_words_requires_exactly_24_words() {
        let short = vec!["abandon".to_string(); RecoveryWords::WORD_COUNT - 1];
        let long = vec!["abandon".to_string(); RecoveryWords::WORD_COUNT + 1];

        assert_eq!(
            RecoveryWords::new(short).err(),
            Some(RecoveryWordsError::InvalidWordCount {
                expected: RecoveryWords::WORD_COUNT,
                actual: RecoveryWords::WORD_COUNT - 1,
            })
        );
        assert_eq!(
            RecoveryWords::new(long).err(),
            Some(RecoveryWordsError::InvalidWordCount {
                expected: RecoveryWords::WORD_COUNT,
                actual: RecoveryWords::WORD_COUNT + 1,
            })
        );
    }

    #[test]
    fn recovery_words_rejects_empty_word() {
        let mut words = words();
        words[7].clear();

        assert_eq!(
            RecoveryWords::new(words).err(),
            Some(RecoveryWordsError::EmptyWord { index: 7 })
        );
    }

    #[test]
    fn recovery_words_redacts_debug() {
        let recovery_words = RecoveryWords::new(words()).unwrap();
        let debug = format!("{recovery_words:?}");

        assert!(debug.contains("RecoveryWords"));
        assert!(debug.contains("len"));
        assert!(debug.contains("24"));
        assert!(!debug.contains("word0"));
        assert!(!debug.contains("word23"));
    }

    #[test]
    fn recovery_words_zeroize_clears_words_in_place() {
        let mut words = vec!["secret".to_string(), "phrase".to_string()];

        zeroize_words(&mut words);

        assert_eq!(words, vec![String::new(), String::new()]);
    }

    #[test]
    fn recovery_words_has_drop_glue() {
        assert!(std::mem::needs_drop::<RecoveryWords>());
    }

    #[test]
    fn duplicate_for_command_creates_a_second_protected_owner() {
        let recovery_words = RecoveryWords::new(words()).unwrap();
        let duplicate = recovery_words.duplicate_for_command().unwrap();

        assert_eq!(recovery_words.len(), RecoveryWords::WORD_COUNT);
        assert_eq!(duplicate.len(), RecoveryWords::WORD_COUNT);
        assert_eq!(recovery_words.as_slice(), duplicate.as_slice());
        assert_eq!(duplicate.word(0), Some("word0"));
    }
}
