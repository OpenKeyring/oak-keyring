#[test]
fn generator_preview_does_not_store_password_as_string_or_clone() {
    let source = include_str!("../src/tui/state/generator_state.rs");

    assert!(!source.contains("pub preview: String"));
    assert!(!source.contains("pw.expose().to_string()"));
    assert!(!source.contains("pub fn use_password(&mut self) -> String"));
    assert!(!source.contains("#[derive(Debug, Clone)]\npub struct GeneratorState"));
}

#[test]
fn edit_record_loading_does_not_rebuild_secret_fields_from_strings() {
    let source = include_str!("../src/tui/screens/edit_record.rs");

    assert!(!source.contains("password: Option<String>"));
    assert!(!source.contains("secret_key: Option<String>"));
    assert!(!source.contains("private_key: Option<String>"));
    assert!(!source.contains("passphrase: Option<String>"));
    assert!(!source.contains("password.expose().to_string()"));
    assert!(!source.contains("secret_key.expose().to_string()"));
}

#[test]
fn import_preview_reuses_validated_session_without_password_clone() {
    let source = include_str!("../src/tui/screens/import_export/screen.rs");

    assert!(!source.contains("SecureStr::new(s.to_string())"));
    assert!(source.contains("session_id"));
    assert!(source.contains("take_secure()"));
}

#[test]
fn okb_kdf_paths_use_locked_argon2_output() {
    let export_source = include_str!("../src/services/import_export/export.rs");
    let okb_source = include_str!("../src/services/import_export/parsers/okb.rs");

    assert!(!export_source.contains("argon2::derive_key(password.expose(), &salt)"));
    assert!(!okb_source.contains("crypto::argon2::derive_key(password.expose(), &salt)"));
    assert!(export_source.contains("derive_key_locked"));
    assert!(okb_source.contains("derive_key_locked"));
}
