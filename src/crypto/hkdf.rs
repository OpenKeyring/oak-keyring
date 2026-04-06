use hkdf::Hkdf;
use sha2::Sha256;

pub fn derive_kek(sk: &[u8; 32]) -> [u8; 32] {
    let hkdf = Hkdf::<Sha256>::new(Some(b"open-keyring-v1"), sk);
    let mut okm = [0u8; 32];
    hkdf.expand(b"master-kek", &mut okm)
        .expect("KEK derivation failed");
    okm
}

pub fn derive_dek(kek: &[u8; 32], version: u32) -> [u8; 32] {
    let hkdf = Hkdf::<Sha256>::new(Some(b"open-keyring-dek"), kek);
    let info = format!("dek-v{}", version);
    let mut okm = [0u8; 32];
    hkdf.expand(info.as_bytes(), &mut okm)
        .expect("DEK derivation failed");
    okm
}

pub fn derive_device_key(kek: &[u8; 32], device_id: &str) -> [u8; 32] {
    let hkdf = Hkdf::<Sha256>::new(Some(b"open-keyring-device"), kek);
    let info = format!("bio-{}", device_id);
    let mut okm = [0u8; 32];
    hkdf.expand(info.as_bytes(), &mut okm)
        .expect("Device key derivation failed");
    okm
}

pub fn derive_index_key(kek: &[u8; 32]) -> [u8; 32] {
    let hkdf = Hkdf::<Sha256>::new(Some(b"open-keyring-index"), kek);
    let mut okm = [0u8; 32];
    hkdf.expand(b"search-index-v1", &mut okm)
        .expect("Index key derivation failed");
    okm
}
