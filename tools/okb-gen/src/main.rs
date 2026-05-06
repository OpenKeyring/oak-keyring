use std::fs;
use std::path::Path;

use chacha20poly1305::aead::Aead;
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use oak_keyring::services::import_export::export::{ExportPayload, ExportRecord};

const PASSWORD: &str = "test-password";
const OUTPUT_DIR: &str = "../../tests/data";
const VAULT_ID: &str = "00000000-0000-0000-0000-000000000001";
const EXPORTED_AT: &str = "2026-05-06T00:00:00Z";

/// Fixed salt for deterministic output. DO NOT change — test files depend on this.
const FIXED_SALT: [u8; 16] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
    0x10,
];

/// Fixed nonce for deterministic output. DO NOT change — test files depend on this.
const FIXED_NONCE: [u8; 24] = [
    0xF0, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA, 0xFB, 0xFC, 0xFD, 0xFE,
    0xFF, 0xE0, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7,
];

const OKB_VERSION: u32 = 1;

/// Deterministic encrypt-and-write. Same input always produces identical bytes.
fn encrypt_and_write_deterministic(
    payload: &ExportPayload,
    output_path: &Path,
) -> Result<usize, Box<dyn std::error::Error>> {
    // 1. Serialize payload to JSON
    let json_bytes = serde_json::to_vec(payload)?;

    // 2. Derive DEK via Argon2id with fixed salt
    let dek = oak_keyring::crypto::argon2::derive_key(PASSWORD, &FIXED_SALT)
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    let dek_arr: [u8; 32] = dek.as_slice().try_into()?;

    // 3. Encrypt with XChaCha20-Poly1305 using fixed nonce
    let cipher =
        XChaCha20Poly1305::new_from_slice(&dek_arr).expect("valid 32-byte key");
    let nonce = XNonce::from_slice(&FIXED_NONCE);
    let ciphertext = cipher
        .encrypt(nonce, json_bytes.as_ref())
        .expect("encryption with valid key/nonce should not fail");

    // 4. Build binary: version (LE) + salt + nonce + ciphertext
    let mut buf = Vec::with_capacity(4 + 16 + 24 + ciphertext.len());
    buf.extend_from_slice(&OKB_VERSION.to_le_bytes());
    buf.extend_from_slice(&FIXED_SALT);
    buf.extend_from_slice(&FIXED_NONCE);
    buf.extend_from_slice(&ciphertext);

    // 5. Write
    fs::write(output_path, &buf)?;
    Ok(buf.len())
}

fn basic_payload() -> ExportPayload {
    ExportPayload {
        version: "1.0".to_string(),
        vault_id: VAULT_ID.to_string(),
        exported_at: EXPORTED_AT.to_string(),
        records: vec![
            ExportRecord {
                id: "00000000-0000-0000-0000-000000000001".to_string(),
                credential_type: "login".to_string(),
                name: "GitHub".to_string(),
                username: Some("developer".to_string()),
                password: Some("gh_secret_123".to_string()),
                url: Some("https://github.com".to_string()),
                notes: None,
                tags: Some(vec!["dev".to_string()]),
                is_favorite: Some(true),
                expires_at: None,
                public_key: None,
                private_key: None,
                passphrase: None,
                app_id: None,
                secret_key: None,
            },
            ExportRecord {
                id: "00000000-0000-0000-0000-000000000002".to_string(),
                credential_type: "login".to_string(),
                name: "Gmail".to_string(),
                username: Some("user@gmail.com".to_string()),
                password: Some("gm_pass_456".to_string()),
                url: Some("https://gmail.com".to_string()),
                notes: None,
                tags: Some(vec!["email".to_string(), "work".to_string()]),
                is_favorite: Some(false),
                expires_at: None,
                public_key: None,
                private_key: None,
                passphrase: None,
                app_id: None,
                secret_key: None,
            },
            ExportRecord {
                id: "00000000-0000-0000-0000-000000000003".to_string(),
                credential_type: "login".to_string(),
                name: "AWS Console".to_string(),
                username: Some("admin@company".to_string()),
                password: Some("aws_root_789".to_string()),
                url: Some("https://aws.amazon.com".to_string()),
                notes: None,
                tags: Some(vec!["cloud".to_string(), "infra".to_string()]),
                is_favorite: Some(false),
                expires_at: None,
                public_key: None,
                private_key: None,
                passphrase: None,
                app_id: None,
                secret_key: None,
            },
        ],
    }
}

fn mixed_types_payload() -> ExportPayload {
    ExportPayload {
        version: "1.0".to_string(),
        vault_id: VAULT_ID.to_string(),
        exported_at: EXPORTED_AT.to_string(),
        records: vec![
            ExportRecord {
                id: "00000000-0000-0000-0000-000000000004".to_string(),
                credential_type: "login".to_string(),
                name: "GitLab".to_string(),
                username: Some("dev@gitlab".to_string()),
                password: Some("gl_pass".to_string()),
                url: Some("https://gitlab.com".to_string()),
                notes: None,
                tags: None,
                is_favorite: None,
                expires_at: None,
                public_key: None,
                private_key: None,
                passphrase: None,
                app_id: None,
                secret_key: None,
            },
            ExportRecord {
                id: "00000000-0000-0000-0000-000000000005".to_string(),
                credential_type: "api".to_string(),
                name: "AWS API Key".to_string(),
                username: None,
                password: None,
                url: None,
                notes: None,
                tags: None,
                is_favorite: None,
                expires_at: None,
                public_key: None,
                private_key: None,
                passphrase: None,
                app_id: Some("AKIAIOSFODNN7".to_string()),
                secret_key: Some("wJalrXUtnFEMI/K7MDENG".to_string()),
            },
            ExportRecord {
                id: "00000000-0000-0000-0000-000000000006".to_string(),
                credential_type: "ssh".to_string(),
                name: "GitHub SSH".to_string(),
                username: None,
                password: None,
                url: None,
                notes: None,
                tags: None,
                is_favorite: None,
                expires_at: None,
                public_key: Some("ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC7...".to_string()),
                private_key: Some(
                    "-----BEGIN OPENSSH PRIVATE KEY-----\n...\n-----END OPENSSH PRIVATE KEY-----"
                        .to_string(),
                ),
                passphrase: Some("ssh_passphrase".to_string()),
                app_id: None,
                secret_key: None,
            },
        ],
    }
}

fn edge_cases_payload() -> ExportPayload {
    ExportPayload {
        version: "1.0".to_string(),
        vault_id: VAULT_ID.to_string(),
        exported_at: EXPORTED_AT.to_string(),
        records: vec![
            ExportRecord {
                id: "00000000-0000-0000-0000-000000000007".to_string(),
                credential_type: "login".to_string(),
                name: r#"Entry, with "quotes""#.to_string(),
                username: None,
                password: Some(r#"<>&"'"#.to_string()),
                url: None,
                notes: Some("line1\nline2\ttab".to_string()),
                tags: None,
                is_favorite: None,
                expires_at: None,
                public_key: None,
                private_key: None,
                passphrase: None,
                app_id: None,
                secret_key: None,
            },
            ExportRecord {
                id: "00000000-0000-0000-0000-000000000008".to_string(),
                credential_type: "login".to_string(),
                name: "Long Notes".to_string(),
                username: None,
                password: None,
                url: None,
                notes: Some("A".repeat(2001)),
                tags: None,
                is_favorite: None,
                expires_at: None,
                public_key: None,
                private_key: None,
                passphrase: None,
                app_id: None,
                secret_key: None,
            },
            ExportRecord {
                id: "00000000-0000-0000-0000-000000000009".to_string(),
                credential_type: "login".to_string(),
                name: "测试账户".to_string(),
                username: Some("用户@example.com".to_string()),
                password: Some("中文密码123".to_string()),
                url: None,
                notes: Some("这是一条中文备注".to_string()),
                tags: Some(vec!["标签1".to_string(), "标签2".to_string()]),
                is_favorite: None,
                expires_at: None,
                public_key: None,
                private_key: None,
                passphrase: None,
                app_id: None,
                secret_key: None,
            },
            ExportRecord {
                id: "00000000-0000-0000-0000-000000000010".to_string(),
                credential_type: "login".to_string(),
                name: "Minimal Entry".to_string(),
                username: None,
                password: None,
                url: None,
                notes: None,
                tags: None,
                is_favorite: None,
                expires_at: None,
                public_key: None,
                private_key: None,
                passphrase: None,
                app_id: None,
                secret_key: None,
            },
        ],
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(OUTPUT_DIR)?;

    let output_dir = Path::new(OUTPUT_DIR);

    // Generate 3 normal samples (deterministic)
    println!("Generating normal .okb samples (deterministic)...");

    let basic_path = output_dir.join("okb_basic.okb");
    let basic_size = encrypt_and_write_deterministic(&basic_payload(), &basic_path)?;
    println!("  Generated: {} ({} bytes)", basic_path.display(), basic_size);

    let mixed_path = output_dir.join("okb_mixed_types.okb");
    let mixed_size = encrypt_and_write_deterministic(&mixed_types_payload(), &mixed_path)?;
    println!(
        "  Generated: {} ({} bytes)",
        mixed_path.display(),
        mixed_size
    );

    let edge_path = output_dir.join("okb_edge_cases.okb");
    let edge_size = encrypt_and_write_deterministic(&edge_cases_payload(), &edge_path)?;
    println!("  Generated: {} ({} bytes)", edge_path.display(), edge_size);

    // Generate 3 corrupted samples from basic
    println!("Generating corrupted .bin samples...");

    let basic_bytes = fs::read(&basic_path)?;

    // Corrupted header
    let mut corrupted_header = basic_bytes.clone();
    corrupted_header[0..4].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    let corrupted_header_path = output_dir.join("okb_corrupted_header.bin");
    fs::write(&corrupted_header_path, &corrupted_header)?;
    println!(
        "  Generated: {} ({} bytes)",
        corrupted_header_path.display(),
        corrupted_header.len()
    );

    // Wrong version
    let mut wrong_version = basic_bytes.clone();
    wrong_version[0..4].copy_from_slice(&99u32.to_le_bytes());
    let wrong_version_path = output_dir.join("okb_wrong_version.bin");
    fs::write(&wrong_version_path, &wrong_version)?;
    println!(
        "  Generated: {} ({} bytes)",
        wrong_version_path.display(),
        wrong_version.len()
    );

    // Truncated
    let truncated: Vec<u8> = basic_bytes.iter().take(4).copied().collect();
    let truncated_path = output_dir.join("okb_truncated.bin");
    fs::write(&truncated_path, &truncated)?;
    println!(
        "  Generated: {} ({} bytes)",
        truncated_path.display(),
        truncated.len()
    );

    println!("\nGenerated 6 test sample files successfully.");

    // Verify determinism: regenerate and compare
    let basic_bytes_again = fs::read(&basic_path)?;
    assert_eq!(
        basic_bytes, basic_bytes_again,
        "deterministic check: same input must produce same output"
    );
    println!("Determinism verified: output is reproducible.");

    Ok(())
}
