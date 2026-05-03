#[cfg(test)]
mod tests {
    use crate::config::{
        AnimationMode, AppConfig, ConfigError, HealthCheckFrequency, ProviderConfig, SyncMode,
        SyncProvider,
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
            err.to_error_code(),
            crate::errors::ErrorCode::ConfigIoError
        ));

        let err = ConfigError::Parse("bad toml".into());
        assert!(matches!(
            err.to_error_code(),
            crate::errors::ErrorCode::ConfigParseError
        ));

        let err = ConfigError::Validation("invalid provider".into());
        assert!(matches!(
            err.to_error_code(),
            crate::errors::ErrorCode::ConfigValidationError
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

    #[test]
    fn provider_config_webdav_roundtrip() {
        let toml_str = r#"
[sync]
provider = "WebDav"
sync_mode = "Auto"
auto_interval_seconds = 600

[sync.provider_config.WebDav]
endpoint = "https://dav.example.com/dav/"
root_path = "/"
username = "user"
"#;
        let config = AppConfig::from_toml(toml_str).unwrap();
        assert!(matches!(config.sync.provider, SyncProvider::WebDav));
        match &config.sync.provider_config {
            Some(ProviderConfig::WebDav(c)) => {
                assert_eq!(c.endpoint, "https://dav.example.com/dav/");
                assert_eq!(c.username.as_deref(), Some("user"));
            }
            other => panic!("expected WebDav, got {:?}", other),
        }
    }

    #[test]
    fn provider_config_disabled_is_none() {
        let toml_str = r#"
[sync]
provider = "Disabled"
"#;
        let config = AppConfig::from_toml(toml_str).unwrap();
        assert!(matches!(config.sync.provider, SyncProvider::Disabled));
        assert!(config.sync.provider_config.is_none());
    }

    #[test]
    fn provider_config_s3_roundtrip() {
        let toml_str = r#"
[sync]
provider = "S3"

[sync.provider_config.S3]
bucket = "my-bucket"
access_key_id = "AKIA_EXAMPLE"
secret_access_key = "secret123"
root_path = "/"
"#;
        let config = AppConfig::from_toml(toml_str).unwrap();
        match &config.sync.provider_config {
            Some(ProviderConfig::S3(c)) => {
                assert_eq!(c.bucket, "my-bucket");
                assert_eq!(c.access_key_id, "AKIA_EXAMPLE");
                assert!(c.endpoint.is_none());
            }
            other => panic!("expected S3, got {:?}", other),
        }
    }

    #[test]
    fn validation_rejects_provider_mismatch() {
        let toml_str = r#"
[sync]
provider = "WebDav"

[sync.provider_config.S3]
bucket = "test"
access_key_id = "key"
secret_access_key = "secret"
"#;
        let config = AppConfig::from_toml(toml_str).unwrap();
        let result = crate::config::validation::validate(&config);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)));
    }

    #[test]
    fn validation_accepts_matching_provider() {
        let toml_str = r#"
[sync]
provider = "WebDav"

[sync.provider_config.WebDav]
endpoint = "https://dav.example.com"
root_path = "/"
"#;
        let config = AppConfig::from_toml(toml_str).unwrap();
        let result = crate::config::validation::validate(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn validation_accepts_disabled_without_provider_config() {
        let config = AppConfig::default_config();
        let result = crate::config::validation::validate(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn validation_accepts_disabled_with_stale_provider_config() {
        let toml_str = r#"
[sync]
provider = "Disabled"

[sync.provider_config.WebDav]
endpoint = "https://dav.example.com"
root_path = "/"
"#;
        let config = AppConfig::from_toml(toml_str).unwrap();
        let result = crate::config::validation::validate(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn config_manager_trait_exists() {
        fn _assert_trait<T: crate::config::ConfigManager>() {}
    }

    #[test]
    fn config_watcher_trait_exists() {
        fn _assert_trait<T: crate::config::ConfigWatcher>() {}
    }

    #[test]
    fn service_notification_trait_exists() {
        fn _assert_trait<T: crate::config::ServiceNotification>() {}
    }

    #[test]
    fn provider_config_icloud_roundtrip() {
        let toml_str = r#"
[sync]
provider = "ICloud"

[sync.provider_config]
ICloud = {}
"#;
        let config = AppConfig::from_toml(toml_str).unwrap();
        assert!(matches!(config.sync.provider, SyncProvider::ICloud));
        assert!(matches!(
            config.sync.provider_config,
            Some(ProviderConfig::ICloud)
        ));
    }

    #[test]
    #[allow(deprecated)]
    fn provider_config_google_drive_roundtrip() {
        let toml_str = r#"
[sync]
provider = "GoogleDrive"

[sync.provider_config.GoogleDrive]
client_id = "id"
client_secret = "secret"
refresh_token = "token"
root_path = "/keyring"
"#;
        let config = AppConfig::from_toml(toml_str).unwrap();
        match &config.sync.provider_config {
            Some(ProviderConfig::GoogleDrive(c)) => {
                // client_id is deprecated but still deserializable
                assert_eq!(c.client_id, "id");
                assert_eq!(c.root_path, "/keyring");
                // refresh_token is #[serde(skip)] -- ignored during deserialization
                assert!(c.refresh_token.is_empty());
                assert!(c.access_token.is_empty());
            }
            other => panic!("expected GoogleDrive, got {:?}", other),
        }
    }

    #[test]
    fn provider_config_dropbox_roundtrip() {
        let toml_str = r#"
[sync]
provider = "Dropbox"

[sync.provider_config.Dropbox]
client_id = "id"
client_secret = "secret"
refresh_token = "token"
"#;
        let config = AppConfig::from_toml(toml_str).unwrap();
        match &config.sync.provider_config {
            Some(ProviderConfig::Dropbox(c)) => {
                assert_eq!(c.client_id, "id");
                assert_eq!(c.root_path, "/");
            }
            other => panic!("expected Dropbox, got {:?}", other),
        }
    }

    #[test]
    fn provider_config_onedrive_roundtrip() {
        let toml_str = r#"
[sync]
provider = "OneDrive"

[sync.provider_config.OneDrive]
client_id = "id"
client_secret = "secret"
refresh_token = "token"
root_path = "/"
"#;
        let config = AppConfig::from_toml(toml_str).unwrap();
        assert!(matches!(
            config.sync.provider_config,
            Some(ProviderConfig::OneDrive(_))
        ));
    }

    #[test]
    fn provider_config_sftp_roundtrip() {
        let toml_str = r#"
[sync]
provider = "Sftp"

[sync.provider_config.Sftp]
server = "user@host.example.com"
ssh_key_path = "/home/user/.ssh/id_ed25519"
root_path = "/backup"
host_check = "Accept"
"#;
        let config = AppConfig::from_toml(toml_str).unwrap();
        match &config.sync.provider_config {
            Some(ProviderConfig::Sftp(c)) => {
                assert_eq!(c.server, "user@host.example.com");
                assert_eq!(c.ssh_key_path, "/home/user/.ssh/id_ed25519");
                assert!(matches!(c.host_check, crate::config::SftpHostCheck::Accept));
            }
            other => panic!("expected Sftp, got {:?}", other),
        }
    }

    #[test]
    fn provider_config_aliyun_drive_roundtrip() {
        let toml_str = r#"
[sync]
provider = "AliyunDrive"

[sync.provider_config.AliyunDrive]
client_id = "id"
client_secret = "secret"
refresh_token = "token"
drive_type = "Backup"
root_path = "/"
"#;
        let config = AppConfig::from_toml(toml_str).unwrap();
        match &config.sync.provider_config {
            Some(ProviderConfig::AliyunDrive(c)) => {
                assert!(matches!(
                    c.drive_type,
                    crate::config::AliyunDriveType::Backup
                ));
            }
            other => panic!("expected AliyunDrive, got {:?}", other),
        }
    }

    #[test]
    fn provider_config_aliyun_oss_roundtrip() {
        let toml_str = r#"
[sync]
provider = "AliyunOss"

[sync.provider_config.AliyunOss]
endpoint = "https://oss-cn-hangzhou.aliyuncs.com"
bucket = "my-bucket"
access_key_id = "key"
access_key_secret = "secret"
root_path = "/"
"#;
        let config = AppConfig::from_toml(toml_str).unwrap();
        assert!(matches!(
            config.sync.provider_config,
            Some(ProviderConfig::AliyunOss(_))
        ));
    }

    #[test]
    fn provider_config_tencent_cos_roundtrip() {
        let toml_str = r#"
[sync]
provider = "TencentCos"

[sync.provider_config.TencentCos]
endpoint = "https://cos.ap-guangzhou.myqcloud.com"
bucket = "my-bucket-1250000000"
secret_id = "id"
secret_key = "key"
root_path = "/"
"#;
        let config = AppConfig::from_toml(toml_str).unwrap();
        assert!(matches!(
            config.sync.provider_config,
            Some(ProviderConfig::TencentCos(_))
        ));
    }

    #[test]
    fn provider_config_huawei_obs_roundtrip() {
        let toml_str = r#"
[sync]
provider = "HuaweiObs"

[sync.provider_config.HuaweiObs]
endpoint = "https://obs.cn-north-4.myhuaweicloud.com"
bucket = "my-bucket"
access_key_id = "key"
secret_access_key = "secret"
root_path = "/"
"#;
        let config = AppConfig::from_toml(toml_str).unwrap();
        assert!(matches!(
            config.sync.provider_config,
            Some(ProviderConfig::HuaweiObs(_))
        ));
    }

    #[test]
    fn provider_config_upyun_roundtrip() {
        let toml_str = r#"
[sync]
provider = "Upyun"

[sync.provider_config.Upyun]
bucket = "my-bucket"
operator = "operator-name"
operator_password = "password"
root_path = "/"
"#;
        let config = AppConfig::from_toml(toml_str).unwrap();
        match &config.sync.provider_config {
            Some(ProviderConfig::Upyun(c)) => {
                assert_eq!(c.operator, "operator-name");
            }
            other => panic!("expected Upyun, got {:?}", other),
        }
    }

    #[test]
    fn full_config_save_load_roundtrip_with_provider() {
        let tmp = std::env::temp_dir().join(format!("ok_config_test_{}", uuid::Uuid::new_v4()));
        let mut config = AppConfig::default_config();
        config.sync.provider = SyncProvider::WebDav;
        config.sync.provider_config = Some(ProviderConfig::WebDav(crate::config::WebDavConfig {
            endpoint: "https://dav.example.com".into(),
            root_path: "/keyring".into(),
            username: Some("user".into()),
            password: Some("pass".into()),
            bearer_token: None,
        }));

        config.save(&tmp).expect("save failed");
        let loaded = AppConfig::load(&tmp).expect("load failed");

        match &loaded.sync.provider_config {
            Some(ProviderConfig::WebDav(c)) => {
                assert_eq!(c.endpoint, "https://dav.example.com");
                assert_eq!(c.username.as_deref(), Some("user"));
            }
            other => panic!("expected WebDav, got {:?}", other),
        }
    }

    #[test]
    fn full_config_save_load_roundtrip_icloud() {
        let tmp = std::env::temp_dir().join(format!("ok_config_test_{}", uuid::Uuid::new_v4()));
        let mut config = AppConfig::default_config();
        config.sync.provider = SyncProvider::ICloud;
        config.sync.provider_config = Some(ProviderConfig::ICloud);

        config.save(&tmp).expect("save failed");
        let loaded = AppConfig::load(&tmp).expect("load failed");

        assert!(matches!(loaded.sync.provider, SyncProvider::ICloud));
        assert!(matches!(
            loaded.sync.provider_config,
            Some(ProviderConfig::ICloud)
        ));
    }

    #[test]
    fn vault_path_uses_platform_default() {
        let config = AppConfig::default_config();
        let path_str = config.general.vault_path.to_string_lossy();
        assert!(
            path_str.contains("open-keyring"),
            "default vault_path should contain 'open-keyring', got: {}",
            path_str
        );
    }

    #[test]
    fn sftp_host_check_serialization_roundtrip() {
        use crate::config::SftpHostCheck;
        for variant in [
            SftpHostCheck::Strict,
            SftpHostCheck::Accept,
            SftpHostCheck::Add,
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            let deserialized: SftpHostCheck = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, variant);
        }
    }

    #[test]
    fn aliyun_drive_type_serialization_roundtrip() {
        use crate::config::AliyunDriveType;
        for variant in [
            AliyunDriveType::Default,
            AliyunDriveType::Backup,
            AliyunDriveType::Resource,
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            let deserialized: AliyunDriveType = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, variant);
        }
    }

    #[cfg(unix)]
    #[test]
    fn saved_config_has_600_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = std::env::temp_dir().join(format!("ok_config_test_{}", uuid::Uuid::new_v4()));
        let config = AppConfig::default_config();
        config.save(&tmp).expect("save failed");
        let meta = std::fs::metadata(tmp.join("config.toml")).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 600 permissions, got {:o}", mode);
    }

    #[test]
    fn unknown_fields_in_toml_are_tolerated() {
        let toml_str = r#"
[general]
auto_lock_seconds = 300
unknown_future_field = "should be ignored"

[some_unknown_section]
foo = "bar"
"#;
        let config = AppConfig::from_toml(toml_str).unwrap();
        assert_eq!(config.general.auto_lock_seconds, 300);
    }
}
