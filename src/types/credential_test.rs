#[cfg(test)]
mod tests {
    use crate::types::credential::{CredentialType, DataError};
    use crate::types::record::{DecryptedRecord, TuiRecord};
    use crate::types::sensitive::SecureStr;
    use crate::types::sync::SyncStatus;

    #[test]
    fn credential_type_db_str_roundtrip() {
        for ct in [
            CredentialType::Login,
            CredentialType::Api,
            CredentialType::Ssh,
        ] {
            let db_str = ct.to_db_str();
            let parsed = CredentialType::from_db_str(db_str).expect("roundtrip failed");
            assert_eq!(ct, parsed);
        }
    }

    #[test]
    fn credential_type_from_db_str_unknown_returns_err() {
        let result = CredentialType::from_db_str("unknown");
        assert!(matches!(result, Err(DataError::InvalidCredentialType(_))));
    }

    #[test]
    fn credential_type_list_prefix() {
        assert_eq!(CredentialType::Login.list_prefix(), "[L]");
        assert_eq!(CredentialType::Api.list_prefix(), "[A]");
        assert_eq!(CredentialType::Ssh.list_prefix(), "[S]");
    }

    #[test]
    fn secure_string_clone_panics() {
        let s = SecureStr::new("secret".to_string());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = s.clone();
        }));
        assert!(result.is_err());
    }

    #[test]
    fn secure_string_debug_redacted() {
        let s = SecureStr::new("secret".to_string());
        let debug = format!("{:?}", s);
        assert_eq!(debug, "***REDACTED***");
    }

    #[test]
    fn secure_string_display_redacted() {
        let s = SecureStr::new("secret".to_string());
        let display = format!("{}", s);
        assert_eq!(display, "***REDACTED***");
    }

    #[test]
    fn secure_string_get_returns_value() {
        let s = SecureStr::new("hello".to_string());
        assert_eq!(s.get(), "hello");
    }

    #[test]
    fn secure_string_into_inner_returns_value() {
        let s = SecureStr::new("hello".to_string());
        let inner = s.into_inner();
        assert_eq!(inner, "hello");
    }

    #[test]
    fn decrypted_record_id() {
        let id = uuid::Uuid::new_v4();
        let rec = DecryptedRecord::Login {
            id,
            is_favorite: false,
            expires_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            version: 1,
            deleted: false,
            deleted_at: None,
            tags: vec![],
            name: "Test".into(),
            username: "user".into(),
            password: SecureStr::new("pass".into()),
            url: None,
            notes: None,
        };
        assert_eq!(rec.id(), id);
    }

    #[test]
    fn decrypted_record_name() {
        let rec = DecryptedRecord::Api {
            id: uuid::Uuid::new_v4(),
            is_favorite: false,
            expires_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            version: 1,
            deleted: false,
            deleted_at: None,
            tags: vec![],
            name: "My API".into(),
            app_id: "app123".into(),
            secret_key: SecureStr::new("key".into()),
            url: None,
            notes: None,
        };
        assert_eq!(rec.name(), "My API");
    }

    #[test]
    fn decrypted_record_credential_type() {
        let rec = DecryptedRecord::Ssh {
            id: uuid::Uuid::new_v4(),
            is_favorite: false,
            expires_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            version: 1,
            deleted: false,
            deleted_at: None,
            tags: vec![],
            name: "Server".into(),
            public_key: "ssh-rsa ...".into(),
            private_key: None,
            passphrase: None,
            notes: None,
        };
        assert_eq!(rec.credential_type(), CredentialType::Ssh);
    }

    #[test]
    fn tui_record_partial_eq() {
        let now = chrono::Utc::now();
        let a = TuiRecord {
            id: uuid::Uuid::new_v4(),
            credential_type: CredentialType::Login,
            name: "Test".into(),
            subtitle: "user".into(),
            is_favorite: false,
            is_expired: false,
            expires_at: None,
            has_weak_password: false,
            created_at: now,
            updated_at: now,
            deleted: false,
            deleted_at: None,
            tags: vec!["tag1".into()],
            sync_status: Some(SyncStatus::Synced),
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn tui_record_no_secure_fields() {
        // Verify TuiRecord doesn't contain any SecureStr fields
        // This is a compile-time check — if it compiles, it passes
        let _record: TuiRecord = TuiRecord {
            id: uuid::Uuid::new_v4(),
            credential_type: CredentialType::Login,
            name: "Test".into(),
            subtitle: "user".into(),
            is_favorite: false,
            is_expired: false,
            expires_at: None,
            has_weak_password: false,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted: false,
            deleted_at: None,
            tags: vec![],
            sync_status: None,
        };
    }
}
