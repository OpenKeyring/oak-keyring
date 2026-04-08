use hkdf::Hkdf;
use sha2::Sha256;

use crate::crypto::CryptoError;

pub fn derive_kek(sk: &[u8; 32]) -> Result<[u8; 32], CryptoError> {
    let hkdf = Hkdf::<Sha256>::new(Some(b"open-keyring-v1"), sk);
    let mut okm = [0u8; 32];
    hkdf.expand(b"master-kek", &mut okm)
        .map_err(|_| CryptoError::DerivationFailed)?;
    Ok(okm)
}

pub fn derive_dek(kek: &[u8; 32], version: u32) -> Result<[u8; 32], CryptoError> {
    let hkdf = Hkdf::<Sha256>::new(Some(b"open-keyring-dek"), kek);
    let info = format!("dek-v{}", version);
    let mut okm = [0u8; 32];
    hkdf.expand(info.as_bytes(), &mut okm)
        .map_err(|_| CryptoError::DerivationFailed)?;
    Ok(okm)
}

pub fn derive_device_key(kek: &[u8; 32], device_id: &str) -> Result<[u8; 32], CryptoError> {
    let hkdf = Hkdf::<Sha256>::new(Some(b"open-keyring-device"), kek);
    let info = format!("bio-{}", device_id);
    let mut okm = [0u8; 32];
    hkdf.expand(info.as_bytes(), &mut okm)
        .map_err(|_| CryptoError::DerivationFailed)?;
    Ok(okm)
}

pub fn derive_index_key(kek: &[u8; 32]) -> Result<[u8; 32], CryptoError> {
    let hkdf = Hkdf::<Sha256>::new(Some(b"open-keyring-index"), kek);
    let mut okm = [0u8; 32];
    hkdf.expand(b"search-index-v1", &mut okm)
        .map_err(|_| CryptoError::DerivationFailed)?;
    Ok(okm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_kek_determinism() {
        let sk = [42u8; 32];
        let kek1 = derive_kek(&sk).unwrap();
        let kek2 = derive_kek(&sk).unwrap();
        assert_eq!(kek1, kek2, "same SK must produce same KEK");
    }

    #[test]
    fn test_derive_dek_determinism() {
        let kek = [99u8; 32];
        let dek1 = derive_dek(&kek, 1).unwrap();
        let dek2 = derive_dek(&kek, 1).unwrap();
        assert_eq!(dek1, dek2, "same KEK + version must produce same DEK");
    }

    #[test]
    fn test_derive_dek_version_isolation() {
        let kek = [99u8; 32];
        let dek_v1 = derive_dek(&kek, 1).unwrap();
        let dek_v2 = derive_dek(&kek, 2).unwrap();
        let dek_v3 = derive_dek(&kek, 3).unwrap();
        assert_ne!(
            dek_v1, dek_v2,
            "different versions must produce different DEKs"
        );
        assert_ne!(
            dek_v2, dek_v3,
            "different versions must produce different DEKs"
        );
        assert_ne!(
            dek_v1, dek_v3,
            "different versions must produce different DEKs"
        );
    }

    #[test]
    fn test_derive_device_key_per_device() {
        let kek = [55u8; 32];
        let dk1 = derive_device_key(&kek, "device-alpha").unwrap();
        let dk2 = derive_device_key(&kek, "device-beta").unwrap();
        let dk3 = derive_device_key(&kek, "device-gamma").unwrap();
        assert_ne!(
            dk1, dk2,
            "different device_ids must produce different Device Keys"
        );
        assert_ne!(
            dk2, dk3,
            "different device_ids must produce different Device Keys"
        );
        assert_ne!(
            dk1, dk3,
            "different device_ids must produce different Device Keys"
        );
    }

    #[test]
    fn test_derive_index_key_determinism() {
        let kek = [77u8; 32];
        let ik1 = derive_index_key(&kek).unwrap();
        let ik2 = derive_index_key(&kek).unwrap();
        assert_eq!(ik1, ik2, "Index Key derivation must be deterministic");
    }

    #[test]
    fn test_full_chain_determinism() {
        let sk = [0u8; 32];
        let kek1 = derive_kek(&sk).unwrap();
        let dek1 = derive_dek(&kek1, 1).unwrap();

        let kek2 = derive_kek(&sk).unwrap();
        let dek2 = derive_dek(&kek2, 1).unwrap();

        assert_eq!(kek1, kek2, "KEK must be deterministic");
        assert_eq!(
            dek1, dek2,
            "full chain SK → KEK → DEK_v1 must be deterministic"
        );
    }

    #[test]
    fn test_different_sk_different_kek() {
        let sk_a = [0u8; 32];
        let sk_b = [1u8; 32];
        let kek_a = derive_kek(&sk_a).unwrap();
        let kek_b = derive_kek(&sk_b).unwrap();
        assert_ne!(kek_a, kek_b, "different SKs must produce different KEKs");
    }

    #[test]
    fn test_key_separation() {
        let sk = [0u8; 32];
        let kek = derive_kek(&sk).unwrap();
        let dek = derive_dek(&kek, 1).unwrap();
        let device_key = derive_device_key(&kek, "test-device").unwrap();
        let index_key = derive_index_key(&kek).unwrap();

        assert_ne!(kek, dek, "KEK must differ from DEK");
        assert_ne!(kek, device_key, "KEK must differ from DeviceKey");
        assert_ne!(kek, index_key, "KEK must differ from IndexKey");
        assert_ne!(dek, device_key, "DEK must differ from DeviceKey");
        assert_ne!(dek, index_key, "DEK must differ from IndexKey");
        assert_ne!(device_key, index_key, "DeviceKey must differ from IndexKey");
    }

    #[test]
    fn test_derive_kek_known_vector() {
        let sk = [0u8; 32];
        let kek = derive_kek(&sk).unwrap();
        insta::assert_debug_snapshot!("derive_kek_known_vector", kek);
    }
}
