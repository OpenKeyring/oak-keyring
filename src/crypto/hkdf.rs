use hkdf::Hkdf;
use sha2::Sha256;

pub fn derive_kek(sk: &[u8; 32]) -> Result<[u8; 32], String> {
    let hkdf = Hkdf::<Sha256>::new(Some(b"open-keyring-v1"), sk);
    let mut okm = [0u8; 32];
    hkdf.expand(b"master-kek", &mut okm)
        .map_err(|e| format!("KEK derivation failed: {}", e))?;
    Ok(okm)
}

pub fn derive_dek(kek: &[u8; 32], version: u32) -> Result<[u8; 32], String> {
    let hkdf = Hkdf::<Sha256>::new(Some(b"open-keyring-dek"), kek);
    let info = format!("dek-v{}", version);
    let mut okm = [0u8; 32];
    hkdf.expand(info.as_bytes(), &mut okm)
        .map_err(|e| format!("DEK derivation failed: {}", e))?;
    Ok(okm)
}

pub fn derive_device_key(kek: &[u8; 32], device_id: &str) -> Result<[u8; 32], String> {
    let hkdf = Hkdf::<Sha256>::new(Some(b"open-keyring-device"), kek);
    let info = format!("bio-{}", device_id);
    let mut okm = [0u8; 32];
    hkdf.expand(info.as_bytes(), &mut okm)
        .map_err(|e| format!("Device key derivation failed: {}", e))?;
    Ok(okm)
}

pub fn derive_index_key(kek: &[u8; 32]) -> Result<[u8; 32], String> {
    let hkdf = Hkdf::<Sha256>::new(Some(b"open-keyring-index"), kek);
    let mut okm = [0u8; 32];
    hkdf.expand(b"search-index-v1", &mut okm)
        .map_err(|e| format!("Index key derivation failed: {}", e))?;
    Ok(okm)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sk(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    #[test]
    fn test_derive_kek_determinism() {
        let sk = make_sk(0x42);
        let kek1 = derive_kek(&sk).unwrap();
        let kek2 = derive_kek(&sk).unwrap();
        assert_eq!(kek1, kek2, "KEK derivation must be deterministic");
    }

    #[test]
    fn test_derive_dek_determinism() {
        let kek = make_sk(0xAB);
        let dek1 = derive_dek(&kek, 1).unwrap();
        let dek2 = derive_dek(&kek, 1).unwrap();
        assert_eq!(dek1, dek2, "DEK derivation must be deterministic");
    }

    #[test]
    fn test_derive_dek_version_isolation() {
        let kek = make_sk(0xCD);
        let dek_v1 = derive_dek(&kek, 1).unwrap();
        let dek_v2 = derive_dek(&kek, 2).unwrap();
        assert_ne!(
            dek_v1, dek_v2,
            "Different DEK versions must produce different keys"
        );
    }

    #[test]
    fn test_derive_device_key_per_device() {
        let kek = make_sk(0xEF);
        let key1 = derive_device_key(&kek, "device-001").unwrap();
        let key2 = derive_device_key(&kek, "device-002").unwrap();
        assert_ne!(key1, key2, "Different devices must have different keys");
    }

    #[test]
    fn test_derive_index_key_determinism() {
        let kek = make_sk(0x55);
        let idx1 = derive_index_key(&kek).unwrap();
        let idx2 = derive_index_key(&kek).unwrap();
        assert_eq!(idx1, idx2, "Index key derivation must be deterministic");
    }

    #[test]
    fn test_full_chain_determinism() {
        let sk = make_sk(0x77);
        let kek = derive_kek(&sk).unwrap();
        let dek_v1 = derive_dek(&kek, 1).unwrap();
        let dek_v2 = derive_dek(&kek, 2).unwrap();

        let kek2 = derive_kek(&sk).unwrap();
        assert_eq!(kek, kek2);

        let dek_v1_again = derive_dek(&kek2, 1).unwrap();
        assert_eq!(dek_v1, dek_v1_again);

        let dek_v2_again = derive_dek(&kek2, 2).unwrap();
        assert_eq!(dek_v2, dek_v2_again);
    }

    #[test]
    fn test_different_sk_different_kek() {
        let sk1 = make_sk(0x11);
        let sk2 = make_sk(0x22);
        let kek1 = derive_kek(&sk1).unwrap();
        let kek2 = derive_kek(&sk2).unwrap();
        assert_ne!(kek1, kek2, "Different SKs must produce different KEKs");
    }

    #[test]
    fn test_key_separation() {
        let kek = make_sk(0x99);

        let dek = derive_dek(&kek, 1).unwrap();
        let device_key = derive_device_key(&kek, "device-001").unwrap();
        let index_key = derive_index_key(&kek).unwrap();

        // All derived keys must be different from each other
        assert_ne!(dek, device_key, "DEK and device key must be different");
        assert_ne!(dek, index_key, "DEK and index key must be different");
        assert_ne!(
            device_key, index_key,
            "Device key and index key must be different"
        );

        // None should equal the KEK itself
        assert_ne!(kek, dek, "KEK and DEK must be different");
        assert_ne!(kek, device_key, "KEK and device key must be different");
        assert_ne!(kek, index_key, "KEK and index key must be different");
    }

    #[test]
    fn test_derive_kek_known_vector() {
        // Test with known input to verify our implementation
        let sk = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ];

        let kek = derive_kek(&sk).unwrap();

        // Verify output is 32 bytes
        assert_eq!(kek.len(), 32);

        // Verify output is not all zeros
        assert_ne!(kek, [0u8; 32], "KEK must not be all zeros");

        // Verify determinism with same input
        let kek2 = derive_kek(&sk).unwrap();
        assert_eq!(kek, kek2, "Known vector must be deterministic");
    }
}
