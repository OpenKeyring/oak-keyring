use bip39::Mnemonic;
use zeroize::Zeroize;

pub struct Passkey {
    mnemonic: Mnemonic,
}

impl Passkey {
    pub fn generate(word_count: usize) -> Result<Self, String> {
        if word_count != 24 {
            return Err("Only 24-word mnemonics are supported".into());
        }
        let mnemonic = Mnemonic::generate(256).map_err(|e| e.to_string())?;
        Ok(Self { mnemonic })
    }

    pub fn from_words(words: &[String]) -> Result<Self, String> {
        let phrase = words.join(" ");
        let mnemonic = Mnemonic::parse(&phrase).map_err(|e| e.to_string())?;
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

    pub fn is_valid_word(word: &str) -> bool {
        Mnemonic::parse_in(bip39::Language::English, word).is_ok()
    }

    pub fn verify_recovery_key(input: &str, expected: &str) -> bool {
        input == expected
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
