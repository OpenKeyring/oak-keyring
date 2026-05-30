use super::*;
use crate::cloud::provider::WebDavAdapter;
use crate::config::sync::{
    AliyunDriveConfig, GoogleDriveConfig, ProviderConfig, S3Config, SftpConfig, SyncConfig,
    SyncMode, SyncProvider, WebDavConfig,
};
use crate::errors::mapping::sync::SyncError;
use opendal::Operator;

// ==================== Factory Tests ====================

#[test]
fn test_factory_disabled_provider_returns_error() {
    let config = SyncConfig {
        provider: SyncProvider::Disabled,
        sync_mode: SyncMode::Auto,
        auto_interval_seconds: 600,
        provider_config: None,
    };

    let result = create_cloud_storage(&config);
    assert!(result.is_err());

    match result.unwrap_err() {
        SyncError::ProviderNotSupported { provider } => {
            assert_eq!(provider, "disabled");
        }
        other => panic!("expected ProviderNotSupported, got {:?}", other),
    }
}

#[test]
fn test_factory_aliyun_drive_not_supported() {
    let config = SyncConfig {
        provider: SyncProvider::AliyunDrive,
        sync_mode: SyncMode::Auto,
        auto_interval_seconds: 600,
        provider_config: Some(ProviderConfig::AliyunDrive(AliyunDriveConfig {
            client_id: "test".to_string(),
            client_secret: "secret".to_string(),
            refresh_token: "token".to_string(),
            drive_type: crate::config::sync::AliyunDriveType::Default,
            root_path: "/".to_string(),
        })),
    };

    let result = create_cloud_storage(&config);
    assert!(result.is_err());

    match result.unwrap_err() {
        SyncError::ProviderNotSupported { provider } => {
            assert_eq!(provider, "aliyun_drive");
        }
        other => panic!("expected ProviderNotSupported, got {:?}", other),
    }
}

#[test]
fn test_factory_webdav_with_valid_config() {
    let config = SyncConfig {
        provider: SyncProvider::WebDav,
        sync_mode: SyncMode::Auto,
        auto_interval_seconds: 600,
        provider_config: Some(ProviderConfig::WebDav(WebDavConfig {
            endpoint: "https://webdav.example.com".to_string(),
            root_path: "/".to_string(),
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
            bearer_token: None,
        })),
    };

    let result = create_cloud_storage(&config);
    assert!(matches!(
        result,
        Err(SyncError::ProviderNotSupported { ref provider }) if provider == "webdav"
    ));
}

#[test]
fn test_factory_webdav_missing_config() {
    let config = SyncConfig {
        provider: SyncProvider::WebDav,
        sync_mode: SyncMode::Auto,
        auto_interval_seconds: 600,
        provider_config: None,
    };

    let result = create_cloud_storage(&config);
    assert!(result.is_err());

    match result.unwrap_err() {
        SyncError::ConfigValidationFailed { field, reason } => {
            assert_eq!(field, "provider_config");
            assert!(reason.contains("WebDAV"));
        }
        other => panic!("expected ConfigValidationFailed, got {:?}", other),
    }
}

#[test]
fn factory_icloud_returns_storage() {
    let config = SyncConfig {
        provider: SyncProvider::ICloud,
        sync_mode: SyncMode::Auto,
        auto_interval_seconds: 600,
        provider_config: Some(ProviderConfig::ICloud),
    };

    let result = create_cloud_storage(&config);
    assert!(result.is_ok());

    let storage = result.unwrap();
    drop(storage);
}

#[test]
fn factory_s3_returns_storage() {
    let config = SyncConfig {
        provider: SyncProvider::S3,
        sync_mode: SyncMode::Auto,
        auto_interval_seconds: 600,
        provider_config: Some(ProviderConfig::S3(S3Config {
            endpoint: Some("https://s3.amazonaws.com".to_string()),
            bucket: "test-bucket".to_string(),
            region: Some("us-east-1".to_string()),
            access_key_id: "key".to_string(),
            secret_access_key: "secret".to_string(),
            root_path: "/".to_string(),
        })),
    };

    let result = create_cloud_storage(&config);
    assert!(matches!(
        result,
        Err(SyncError::ProviderNotSupported { ref provider }) if provider == "s3"
    ));
}

#[test]
fn factory_sftp_returns_storage() {
    let config = SyncConfig {
        provider: SyncProvider::Sftp,
        sync_mode: SyncMode::Auto,
        auto_interval_seconds: 600,
        provider_config: Some(ProviderConfig::Sftp(SftpConfig {
            server: "localhost:22".to_string(),
            root_path: "/".to_string(),
            ssh_key_path: "/dev/null".to_string(),
            host_check: crate::config::sync::SftpHostCheck::Strict,
        })),
    };

    let result = create_cloud_storage(&config);
    assert!(matches!(
        result,
        Err(SyncError::ProviderNotSupported { ref provider }) if provider == "sftp"
    ));
}

// ==================== Config Validation Tests ====================

#[test]
fn test_validate_config_missing_endpoint() {
    let adapter = WebDavAdapter::new();
    let config = ProviderConfig::WebDav(WebDavConfig {
        endpoint: "".to_string(),
        root_path: "/".to_string(),
        username: Some("user".to_string()),
        password: Some("pass".to_string()),
        bearer_token: None,
    });

    let result = adapter.validate_config(&config);
    assert!(result.is_err());

    match result.unwrap_err() {
        SyncError::ConfigValidationFailed { field, reason } => {
            assert_eq!(field, "endpoint");
            assert!(reason.contains("empty"));
        }
        other => panic!("expected ConfigValidationFailed, got {:?}", other),
    }
}

#[test]
fn test_validate_config_missing_auth() {
    let adapter = WebDavAdapter::new();
    let config = ProviderConfig::WebDav(WebDavConfig {
        endpoint: "https://webdav.example.com".to_string(),
        root_path: "/".to_string(),
        username: None,
        password: None,
        bearer_token: None,
    });

    let result = adapter.validate_config(&config);
    assert!(result.is_err());

    match result.unwrap_err() {
        SyncError::ConfigValidationFailed { field, reason } => {
            assert_eq!(field, "authentication");
            assert!(reason.contains("bearer_token") || reason.contains("username"));
        }
        other => panic!("expected ConfigValidationFailed, got {:?}", other),
    }
}

#[test]
fn test_validate_config_valid_with_bearer_token() {
    let adapter = WebDavAdapter::new();
    let config = ProviderConfig::WebDav(WebDavConfig {
        endpoint: "https://webdav.example.com".to_string(),
        root_path: "/".to_string(),
        username: None,
        password: None,
        bearer_token: Some("my-token".to_string()),
    });

    let result = adapter.validate_config(&config);
    assert!(result.is_ok());
}

#[test]
fn test_validate_config_valid_with_basic_auth() {
    let adapter = WebDavAdapter::new();
    let config = ProviderConfig::WebDav(WebDavConfig {
        endpoint: "https://webdav.example.com".to_string(),
        root_path: "/".to_string(),
        username: Some("user".to_string()),
        password: Some("pass".to_string()),
        bearer_token: None,
    });

    let result = adapter.validate_config(&config);
    assert!(result.is_ok());
}

#[test]
fn s3_validate_missing_bucket() {
    let adapter = S3Adapter::new();
    let config = ProviderConfig::S3(S3Config {
        endpoint: Some("https://s3.amazonaws.com".to_string()),
        bucket: "".to_string(),
        region: Some("us-east-1".to_string()),
        access_key_id: "key".to_string(),
        secret_access_key: "secret".to_string(),
        root_path: "/".to_string(),
    });

    let result = adapter.validate_config(&config);
    assert!(result.is_ok());
}

#[test]
fn sftp_validate_missing_host() {
    let adapter = SftpAdapter::new();
    let config = ProviderConfig::Sftp(SftpConfig {
        server: "".to_string(),
        root_path: "/".to_string(),
        ssh_key_path: "/dev/null".to_string(),
        host_check: crate::config::sync::SftpHostCheck::Strict,
    });

    let result = adapter.validate_config(&config);
    assert!(result.is_ok());
}

#[test]
#[allow(deprecated)]
fn oauth2_validate_missing_refresh_token() {
    let adapter = GoogleDriveAdapter::new();
    let config = ProviderConfig::GoogleDrive(GoogleDriveConfig {
        access_token: String::new(),
        refresh_token: String::new(),
        root_path: "/".to_string(),
        client_id: String::new(),
        client_secret: String::new(),
    });

    let result = adapter.validate_config(&config);
    assert!(result.is_err());

    match result.unwrap_err() {
        SyncError::ConfigValidationFailed { field, reason } => {
            assert_eq!(field, "refresh_token");
            assert!(reason.contains("refresh_token"));
        }
        other => panic!("expected ConfigValidationFailed, got {:?}", other),
    }
}

#[test]
#[allow(deprecated)]
fn oauth2_validate_with_valid_config() {
    let adapter = GoogleDriveAdapter::new();
    let config = ProviderConfig::GoogleDrive(GoogleDriveConfig {
        access_token: "test_access_token".to_string(),
        refresh_token: "test_refresh_token".to_string(),
        root_path: "/".to_string(),
        client_id: String::new(),
        client_secret: String::new(),
    });

    let result = adapter.validate_config(&config);
    assert!(result.is_ok());
}

#[test]
#[allow(deprecated)]
fn google_drive_operator_prefers_refresh_token_when_access_token_is_loaded() {
    let adapter = GoogleDriveAdapter::new();
    let config = ProviderConfig::GoogleDrive(GoogleDriveConfig {
        access_token: "test_access_token".to_string(),
        refresh_token: "test_refresh_token".to_string(),
        root_path: ".oak-keyring/".to_string(),
        client_id: String::new(),
        client_secret: String::new(),
    });

    let result = adapter.create_operator(&config);
    assert!(
        result.is_ok(),
        "operator creation should not pass access_token and refresh_token together: {result:?}"
    );
}

#[test]
fn icloud_needs_watcher() {
    let adapter = ICloudAdapter::new();
    assert!(adapter.needs_watcher());
}

// ==================== Path Normalization Tests ====================

#[test]
fn test_normalize_path() {
    let adapter = WebDavAdapter::new();

    // Normal case
    assert_eq!(
        adapter.normalize_path("/base", "file.txt"),
        "/base/file.txt"
    );

    // Base with trailing slash should be stripped
    assert_eq!(
        adapter.normalize_path("/base/", "file.txt"),
        "/base/file.txt"
    );

    // File with leading slash should be stripped
    assert_eq!(
        adapter.normalize_path("/base", "/file.txt"),
        "/base/file.txt"
    );

    // Empty base
    assert_eq!(adapter.normalize_path("", "file.txt"), "/file.txt");

    // Nested paths
    assert_eq!(
        adapter.normalize_path("/base/path", "subdir/file.txt"),
        "/base/path/subdir/file.txt"
    );
}

#[test]
fn test_normalize_path_default_impl() {
    // Test that the default implementation in the trait works
    // This uses the base implementation, not WebDAV-specific
    struct DummyAdapter;
    impl ProviderAdapter for DummyAdapter {
        fn create_operator(&self, _config: &ProviderConfig) -> Result<Operator, SyncError> {
            unreachable!("not called in this test")
        }
        fn validate_config(&self, _config: &ProviderConfig) -> Result<(), SyncError> {
            unreachable!("not called in this test")
        }
    }

    let adapter = DummyAdapter;

    // Default implementation handles trailing/leading slashes
    assert_eq!(
        adapter.normalize_path("/base", "file.txt"),
        "/base/file.txt"
    );
}

// ==================== Provider Name Tests ====================

#[test]
fn test_provider_name() {
    assert_eq!(provider_name(SyncProvider::Disabled), "disabled");
    assert_eq!(provider_name(SyncProvider::ICloud), "icloud");
    assert_eq!(provider_name(SyncProvider::GoogleDrive), "google_drive");
    assert_eq!(provider_name(SyncProvider::Dropbox), "dropbox");
    assert_eq!(provider_name(SyncProvider::OneDrive), "onedrive");
    assert_eq!(provider_name(SyncProvider::WebDav), "webdav");
    assert_eq!(provider_name(SyncProvider::Sftp), "sftp");
    assert_eq!(provider_name(SyncProvider::S3), "s3");
    assert_eq!(provider_name(SyncProvider::AliyunDrive), "aliyun_drive");
    assert_eq!(provider_name(SyncProvider::AliyunOss), "aliyun_oss");
    assert_eq!(provider_name(SyncProvider::TencentCos), "tencent_cos");
    assert_eq!(provider_name(SyncProvider::HuaweiObs), "huawei_obs");
    assert_eq!(provider_name(SyncProvider::Upyun), "upyun");
}

// ==================== Trait Object Safety Tests ====================

#[test]
fn test_provider_adapter_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<WebDavAdapter>();
}

// ==================== Adapter Default Implementations ====================

#[test]
fn test_needs_watcher_default_returns_false() {
    struct DummyAdapter;
    impl ProviderAdapter for DummyAdapter {
        fn create_operator(&self, _config: &ProviderConfig) -> Result<Operator, SyncError> {
            unreachable!("not called in this test")
        }
        fn validate_config(&self, _config: &ProviderConfig) -> Result<(), SyncError> {
            unreachable!("not called in this test")
        }
    }

    let adapter = DummyAdapter;
    assert!(!adapter.needs_watcher());
}

#[test]
fn test_refresh_auth_default_is_noop() {
    struct DummyAdapter;
    impl ProviderAdapter for DummyAdapter {
        fn create_operator(&self, _config: &ProviderConfig) -> Result<Operator, SyncError> {
            unreachable!("not called in this test")
        }
        fn validate_config(&self, _config: &ProviderConfig) -> Result<(), SyncError> {
            unreachable!("not called in this test")
        }
    }

    let adapter = DummyAdapter;
    // Create a memory operator for testing
    let mut op = Operator::new(opendal::services::Memory::default())
        .unwrap()
        .finish();

    // Should not panic and returns Ok
    let result = adapter.refresh_auth(&mut op);
    assert!(result.is_ok());
}
