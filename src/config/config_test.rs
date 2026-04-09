#[cfg(test)]
mod tests {
    use crate::config::{
        AnimationMode, AppConfig, ConfigError, HealthCheckFrequency, SyncMode, SyncProvider,
    };
    use crate::errors::service_error::ServiceError;

    #[test]
    fn default_values_match_spec() {
        let config = AppConfig::default_config();
        assert_eq!(config.general.auto_lock_seconds, 300);
        assert_eq!(config.general.clipboard_clear_seconds, 30);
        assert_eq!(config.general.trash_retention_days, 30);
        assert!(matches!(config.general.animation, AnimationMode::Auto));
        assert_eq!(config.general.language, "auto");
        assert!(matches!(config.sync.provider, SyncProvider::Disabled));
        assert!(matches!(config.sync.sync_mode, SyncMode::Auto));
        assert_eq!(config.sync.auto_interval_seconds, 600);
        assert!(config.security.health_check_enabled);
        assert!(matches!(
            config.security.health_check_frequency,
            HealthCheckFrequency::OnStartup
        ));
        assert!(config.security.audit_enabled);
        assert_eq!(config.security.audit_retention_days, 365);
        assert_eq!(config.password.length, 16);
        assert!(config.password.include_digits);
        assert!(config.password.include_uppercase);
        assert!(config.password.include_special);
    }

    #[test]
    fn load_nonexistent_returns_default() {
        let tmp = std::env::temp_dir().join(format!("ok_config_test_{}", uuid::Uuid::new_v4()));
        let config = AppConfig::load(&tmp).expect("load failed");
        assert_eq!(config.general.auto_lock_seconds, 300);
    }

    #[test]
    fn save_load_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("ok_config_test_{}", uuid::Uuid::new_v4()));
        let mut config = AppConfig::default_config();
        config.general.auto_lock_seconds = 900;
        config.sync.provider = SyncProvider::WebDav;
        config.sync.sync_mode = SyncMode::Manual;
        config.password.length = 24;

        config.save(&tmp).expect("save failed");
        let loaded = AppConfig::load(&tmp).expect("load failed");

        assert_eq!(loaded.general.auto_lock_seconds, 900);
        assert!(matches!(loaded.sync.provider, SyncProvider::WebDav));
        assert!(matches!(loaded.sync.sync_mode, SyncMode::Manual));
        assert_eq!(loaded.password.length, 24);
    }

    #[test]
    fn partial_config_uses_defaults_for_missing() {
        let tmp = std::env::temp_dir().join(format!("ok_config_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("config.toml"),
            "[general]\nauto_lock_seconds = 60\n",
        )
        .unwrap();

        let config = AppConfig::load(&tmp).expect("load failed");
        assert_eq!(config.general.auto_lock_seconds, 60);
        assert_eq!(config.general.clipboard_clear_seconds, 30); // default
        assert_eq!(config.password.length, 16); // default
    }

    #[test]
    fn malformed_toml_returns_err() {
        let tmp = std::env::temp_dir().join(format!("ok_config_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("config.toml"), "this is not toml {{{").unwrap();

        let result = AppConfig::load(&tmp);
        assert!(result.is_err());
    }

    #[test]
    fn sync_provider_all_variants() {
        let providers = [
            SyncProvider::Disabled,
            SyncProvider::ICloud,
            SyncProvider::GoogleDrive,
            SyncProvider::Dropbox,
            SyncProvider::OneDrive,
            SyncProvider::WebDav,
            SyncProvider::Sftp,
            SyncProvider::S3,
            SyncProvider::AliyunDrive,
            SyncProvider::AliyunOss,
            SyncProvider::TencentCos,
            SyncProvider::HuaweiObs,
            SyncProvider::Upyun,
        ];
        assert_eq!(providers.len(), 13);
    }

    #[test]
    fn config_error_implements_service_error() {
        let err = ConfigError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "not found",
        ));
        assert!(matches!(
            err.error_code(),
            crate::errors::ErrorCode::Config(_)
        ));

        let err = ConfigError::Parse("bad toml".into());
        assert!(matches!(
            err.error_code(),
            crate::errors::ErrorCode::Config(_)
        ));

        let err = ConfigError::Validation("invalid provider".into());
        assert!(matches!(
            err.error_code(),
            crate::errors::ErrorCode::Config(_)
        ));
    }

    #[test]
    fn malformed_toml_returns_parse_error() {
        let bad_toml = r#"
            [sync
            provider = "WebDav"
        "#;
        let result = AppConfig::from_toml(bad_toml);
        assert!(result.is_err(), "malformed TOML should return Err");
        let err = result.unwrap_err();
        assert!(
            matches!(err, ConfigError::Parse(_)),
            "should be Parse error"
        );
    }

    #[test]
    fn default_config_values_match_spec() {
        let config = AppConfig::default_config();
        assert!(config
            .general
            .vault_path
            .to_string_lossy()
            .contains("open-keyring"));
        assert!(matches!(config.sync.provider, SyncProvider::Disabled));
        assert!(matches!(config.sync.sync_mode, SyncMode::Auto));
        assert_eq!(config.sync.auto_interval_seconds, 600);
        assert!(matches!(
            config.security.health_check_frequency,
            HealthCheckFrequency::OnStartup
        ));
    }
}
