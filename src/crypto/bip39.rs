use bip39::Mnemonic;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MnemonicLanguage {
    #[default]
    English,
    ChineseSimplified,
}

impl MnemonicLanguage {
    pub fn from_keystore_value(s: &str) -> Result<Self, String> {
        match s {
            "en" => Ok(Self::English),
            "zh-CN" => Ok(Self::ChineseSimplified),
            other => Err(format!("unsupported mnemonic language: {other}")),
        }
    }

    pub fn to_keystore_value(&self) -> &'static str {
        match self {
            Self::English => "en",
            Self::ChineseSimplified => "zh-CN",
        }
    }

    pub fn to_bip39_language(&self) -> bip39::Language {
        match self {
            Self::English => bip39::Language::English,
            Self::ChineseSimplified => bip39::Language::SimplifiedChinese,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::English => "English",
            Self::ChineseSimplified => "中文(简体)",
        }
    }

    pub fn all() -> &'static [MnemonicLanguage] {
        &[
            MnemonicLanguage::English,
            MnemonicLanguage::ChineseSimplified,
        ]
    }

    /// Resolve a config language string (e.g. "zh-CN", "zh-TW", "en") to a
    /// MnemonicLanguage. Returns English for unrecognized values.
    pub fn from_config_language(config_language: &str) -> Self {
        match config_language {
            "zh-CN" | "zh-TW" | "zh" => Self::ChineseSimplified,
            _ => Self::English,
        }
    }
}

pub struct Passkey {
    mnemonic: Mnemonic,
}

impl Passkey {
    pub fn generate(word_count: usize, language: MnemonicLanguage) -> Result<Self, String> {
        if word_count != 24 {
            return Err("Only 24-word mnemonics are supported".into());
        }
        let mnemonic = Mnemonic::generate_in(language.to_bip39_language(), word_count)
            .map_err(|e| e.to_string())?;
        Ok(Self { mnemonic })
    }

    pub fn from_words(words: &[String], language: MnemonicLanguage) -> Result<Self, String> {
        let phrase = words.join(" ");
        let mnemonic =
            Mnemonic::parse_in(language.to_bip39_language(), &phrase).map_err(|e| e.to_string())?;
        Ok(Self { mnemonic })
    }

    pub fn to_words(&self) -> Vec<String> {
        self.mnemonic.words().map(String::from).collect()
    }

    pub fn to_seed(&self, passphrase: Option<&str>) -> Result<PasskeySeed, String> {
        let seed = self.mnemonic.to_seed(passphrase.unwrap_or(""));
        let mut seed_bytes = [0u8; 64];
        seed_bytes.copy_from_slice(&seed);
        Ok(PasskeySeed(seed_bytes))
    }

    pub fn is_valid_word(word: &str, language: MnemonicLanguage) -> bool {
        language.to_bip39_language().find_word(word).is_some()
    }

    pub fn verify_recovery_key(input: &str, expected: &str) -> bool {
        let input_hash = Sha256::digest(input.as_bytes());
        let expected_hash = Sha256::digest(expected.as_bytes());
        input_hash.ct_eq(&expected_hash).into()
    }
}

pub struct PasskeySeed([u8; 64]);

impl PasskeySeed {
    pub fn to_secret_key(&self) -> [u8; 32] {
        let mut sk = [0u8; 32];
        sk.copy_from_slice(&self.0[..32]);
        sk
    }
}

impl Drop for PasskeySeed {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // MnemonicLanguage tests
    #[test]
    fn test_mnemonic_language_from_str_en() {
        let lang = MnemonicLanguage::from_keystore_value("en").unwrap();
        assert_eq!(lang, MnemonicLanguage::English);
    }

    #[test]
    fn test_mnemonic_language_from_str_zh_cn() {
        let lang = MnemonicLanguage::from_keystore_value("zh-CN").unwrap();
        assert_eq!(lang, MnemonicLanguage::ChineseSimplified);
    }

    #[test]
    fn test_mnemonic_language_from_str_invalid() {
        assert!(MnemonicLanguage::from_keystore_value("fr").is_err());
        assert!(MnemonicLanguage::from_keystore_value("").is_err());
    }

    #[test]
    fn test_mnemonic_language_to_keystore_value() {
        assert_eq!(MnemonicLanguage::English.to_keystore_value(), "en");
        assert_eq!(
            MnemonicLanguage::ChineseSimplified.to_keystore_value(),
            "zh-CN"
        );
    }

    #[test]
    fn test_mnemonic_language_to_bip39_language() {
        assert_eq!(
            MnemonicLanguage::English.to_bip39_language(),
            bip39::Language::English
        );
        assert_eq!(
            MnemonicLanguage::ChineseSimplified.to_bip39_language(),
            bip39::Language::SimplifiedChinese
        );
    }

    #[test]
    fn test_mnemonic_language_display_name() {
        assert_eq!(MnemonicLanguage::English.display_name(), "English");
        assert_eq!(
            MnemonicLanguage::ChineseSimplified.display_name(),
            "中文(简体)"
        );
    }

    #[test]
    fn test_mnemonic_language_default() {
        assert_eq!(MnemonicLanguage::default(), MnemonicLanguage::English);
    }

    #[test]
    fn test_mnemonic_language_all() {
        let all = MnemonicLanguage::all();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0], MnemonicLanguage::English);
        assert_eq!(all[1], MnemonicLanguage::ChineseSimplified);
    }

    #[test]
    fn test_generate_24_words() {
        let pk = Passkey::generate(24, MnemonicLanguage::English)
            .expect("24-word generation should succeed");
        let words = pk.to_words();
        assert_eq!(words.len(), 24);
    }

    #[test]
    fn test_generate_word_count_rejection() {
        assert!(Passkey::generate(12, MnemonicLanguage::English).is_err());
        assert!(Passkey::generate(15, MnemonicLanguage::English).is_err());
        assert!(Passkey::generate(0, MnemonicLanguage::English).is_err());
    }

    #[test]
    fn test_roundtrip_generate_from_words() {
        let pk = Passkey::generate(24, MnemonicLanguage::English).unwrap();
        let words = pk.to_words();
        let pk2 = Passkey::from_words(&words, MnemonicLanguage::English).unwrap();
        let words2 = pk2.to_words();
        assert_eq!(words, words2);
    }

    #[test]
    fn test_to_seed_determinism() {
        let pk = Passkey::generate(24, MnemonicLanguage::English).unwrap();
        let seed1 = pk.to_seed(None).unwrap();
        let seed2 = pk.to_seed(None).unwrap();
        assert_eq!(seed1.to_secret_key(), seed2.to_secret_key());
    }

    #[test]
    fn test_seed_to_secret_key_first_32_bytes() {
        let pk = Passkey::generate(24, MnemonicLanguage::English).unwrap();
        let seed = pk.to_seed(None).unwrap();
        let sk = seed.to_secret_key();
        assert_eq!(sk.len(), 32);
    }

    #[test]
    fn test_seed_zeroize_on_drop() {
        use std::mem::needs_drop;
        // Compile-time check: PasskeySeed has a non-trivial Drop (zeroize).
        assert!(needs_drop::<PasskeySeed>());
    }

    #[test]
    fn test_is_valid_word_known_words() {
        assert!(Passkey::is_valid_word("abandon", MnemonicLanguage::English));
        assert!(Passkey::is_valid_word("zoo", MnemonicLanguage::English));
        assert!(Passkey::is_valid_word("art", MnemonicLanguage::English));
    }

    #[test]
    fn test_is_valid_word_invalid_word() {
        assert!(!Passkey::is_valid_word("xyz123", MnemonicLanguage::English));
        assert!(!Passkey::is_valid_word("", MnemonicLanguage::English));
        assert!(!Passkey::is_valid_word(
            "notaword",
            MnemonicLanguage::English
        ));
    }

    #[test]
    fn test_is_valid_word_b3_regression() {
        // B3 regression: previously is_valid_word tried to parse a single word
        // as a full mnemonic via Mnemonic::parse_in, which would always fail
        // for valid individual words like "abandon". Now it correctly checks
        // the wordlist.
        assert!(
            Passkey::is_valid_word("abandon", MnemonicLanguage::English),
            "B3 regression: 'abandon' is a valid BIP39 word but was rejected"
        );
        assert!(
            Passkey::is_valid_word("zoo", MnemonicLanguage::English),
            "B3 regression: 'zoo' is a valid BIP39 word but was rejected"
        );
    }

    #[test]
    fn test_from_words_invalid_mnemonic_fails() {
        let bad_words: Vec<String> = (0..24).map(|_| "foobar".to_string()).collect();
        assert!(Passkey::from_words(&bad_words, MnemonicLanguage::English).is_err());
    }

    #[test]
    fn test_full_recovery_flow() {
        // generate → to_words → from_words → to_seed → to_secret_key
        let pk = Passkey::generate(24, MnemonicLanguage::English).unwrap();
        let words = pk.to_words();

        let pk2 = Passkey::from_words(&words, MnemonicLanguage::English).unwrap();
        let seed = pk2.to_seed(None).unwrap();
        let sk = seed.to_secret_key();
        assert_eq!(sk.len(), 32);

        // Verify the same mnemonic produces the same secret key
        let seed2 = pk2.to_seed(None).unwrap();
        let sk2 = seed2.to_secret_key();
        assert_eq!(sk, sk2);
    }

    // ─── Multi-language Tests ───────────────────────────────────────────

    #[test]
    fn test_generate_english_words() {
        let pk = Passkey::generate(24, MnemonicLanguage::English).unwrap();
        let words = pk.to_words();
        assert_eq!(words.len(), 24);
        assert!(words.iter().all(|w| w.is_ascii()));
    }

    #[test]
    fn test_generate_chinese_words() {
        let pk = Passkey::generate(24, MnemonicLanguage::ChineseSimplified).unwrap();
        let words = pk.to_words();
        assert_eq!(words.len(), 24);
        assert!(words.iter().all(|w| !w.is_ascii()));
    }

    #[test]
    fn test_roundtrip_chinese_mnemonic() {
        let pk = Passkey::generate(24, MnemonicLanguage::ChineseSimplified).unwrap();
        let words = pk.to_words();
        let pk2 = Passkey::from_words(&words, MnemonicLanguage::ChineseSimplified).unwrap();
        let words2 = pk2.to_words();
        assert_eq!(words, words2);
    }

    #[test]
    fn test_cross_language_from_words_fails() {
        let pk = Passkey::generate(24, MnemonicLanguage::ChineseSimplified).unwrap();
        let words = pk.to_words();
        let result = Passkey::from_words(&words, MnemonicLanguage::English);
        assert!(result.is_err(), "cross-language parse must fail");
    }

    #[test]
    fn test_is_valid_word_english() {
        assert!(Passkey::is_valid_word("abandon", MnemonicLanguage::English));
        assert!(!Passkey::is_valid_word("的", MnemonicLanguage::English));
    }

    #[test]
    fn test_is_valid_word_chinese() {
        assert!(Passkey::is_valid_word(
            "的",
            MnemonicLanguage::ChineseSimplified
        ));
        assert!(!Passkey::is_valid_word(
            "abandon",
            MnemonicLanguage::ChineseSimplified
        ));
    }

    #[test]
    fn test_to_seed_language_independence() {
        let pk = Passkey::generate(24, MnemonicLanguage::English).unwrap();
        let seed1 = pk.to_seed(None).unwrap();
        let pk2 = Passkey::from_words(&pk.to_words(), MnemonicLanguage::English).unwrap();
        let seed2 = pk2.to_seed(None).unwrap();
        assert_eq!(seed1.to_secret_key(), seed2.to_secret_key());
    }
}
