use opendal::Operator;

use crate::cloud::ProviderAdapter;
use crate::config::sync::ProviderConfig;
use crate::errors::mapping::sync::SyncError;

#[derive(Debug, Default)]
pub struct S3Adapter;

impl S3Adapter {
    pub fn new() -> Self {
        Self
    }
}

impl ProviderAdapter for S3Adapter {
    fn create_operator(&self, config: &ProviderConfig) -> Result<Operator, SyncError> {
        match config {
            ProviderConfig::S3(s3_config) => {
                let mut builder = opendal::services::S3::default()
                    .bucket(&s3_config.bucket)
                    .access_key_id(&s3_config.access_key_id)
                    .secret_access_key(&s3_config.secret_access_key);

                if let Some(ref endpoint) = s3_config.endpoint {
                    builder = builder.endpoint(endpoint);
                }
                if let Some(ref region) = s3_config.region {
                    builder = builder.region(region);
                }
                if !s3_config.root_path.is_empty() {
                    builder = builder.root(&s3_config.root_path);
                }

                let operator = Operator::new(builder)
                    .map_err(|e| SyncError::ProviderError {
                        provider: "s3".to_string(),
                        message: format!("failed to create operator: {}", e),
                    })?
                    .finish();

                Ok(operator)
            }
            _ => Err(SyncError::ProviderError {
                provider: "s3".to_string(),
                message: "expected S3 config".to_string(),
            }),
        }
    }

    fn validate_config(&self, config: &ProviderConfig) -> Result<(), SyncError> {
        match config {
            ProviderConfig::S3(s3_config) => {
                if s3_config.bucket.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "bucket".to_string(),
                        reason: "bucket cannot be empty".to_string(),
                    });
                }
                if s3_config.access_key_id.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "access_key_id".to_string(),
                        reason: "access_key_id cannot be empty".to_string(),
                    });
                }
                if s3_config.secret_access_key.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "secret_access_key".to_string(),
                        reason: "secret_access_key cannot be empty".to_string(),
                    });
                }
                Ok(())
            }
            _ => Err(SyncError::ProviderError {
                provider: "s3".to_string(),
                message: "expected S3 config".to_string(),
            }),
        }
    }
}

#[derive(Debug, Default)]
pub struct AliyunOssAdapter;

impl AliyunOssAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl ProviderAdapter for AliyunOssAdapter {
    fn create_operator(&self, config: &ProviderConfig) -> Result<Operator, SyncError> {
        match config {
            ProviderConfig::AliyunOss(oss_config) => {
                let mut builder = opendal::services::S3::default()
                    .bucket(&oss_config.bucket)
                    .access_key_id(&oss_config.access_key_id)
                    .secret_access_key(&oss_config.access_key_secret)
                    .endpoint(&oss_config.endpoint);

                if !oss_config.root_path.is_empty() {
                    builder = builder.root(&oss_config.root_path);
                }

                let operator = Operator::new(builder)
                    .map_err(|e| SyncError::ProviderError {
                        provider: "aliyun_oss".to_string(),
                        message: format!("failed to create operator: {}", e),
                    })?
                    .finish();

                Ok(operator)
            }
            _ => Err(SyncError::ProviderError {
                provider: "aliyun_oss".to_string(),
                message: "expected AliyunOss config".to_string(),
            }),
        }
    }

    fn validate_config(&self, config: &ProviderConfig) -> Result<(), SyncError> {
        match config {
            ProviderConfig::AliyunOss(oss_config) => {
                if oss_config.endpoint.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "endpoint".to_string(),
                        reason: "endpoint cannot be empty".to_string(),
                    });
                }
                if oss_config.bucket.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "bucket".to_string(),
                        reason: "bucket cannot be empty".to_string(),
                    });
                }
                if oss_config.access_key_id.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "access_key_id".to_string(),
                        reason: "access_key_id cannot be empty".to_string(),
                    });
                }
                if oss_config.access_key_secret.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "access_key_secret".to_string(),
                        reason: "access_key_secret cannot be empty".to_string(),
                    });
                }
                Ok(())
            }
            _ => Err(SyncError::ProviderError {
                provider: "aliyun_oss".to_string(),
                message: "expected AliyunOss config".to_string(),
            }),
        }
    }
}

#[derive(Debug, Default)]
pub struct TencentCosAdapter;

impl TencentCosAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl ProviderAdapter for TencentCosAdapter {
    fn create_operator(&self, config: &ProviderConfig) -> Result<Operator, SyncError> {
        match config {
            ProviderConfig::TencentCos(cos_config) => {
                let mut builder = opendal::services::S3::default()
                    .bucket(&cos_config.bucket)
                    .access_key_id(&cos_config.secret_id)
                    .secret_access_key(&cos_config.secret_key)
                    .endpoint(&cos_config.endpoint);

                if !cos_config.root_path.is_empty() {
                    builder = builder.root(&cos_config.root_path);
                }

                let operator = Operator::new(builder)
                    .map_err(|e| SyncError::ProviderError {
                        provider: "tencent_cos".to_string(),
                        message: format!("failed to create operator: {}", e),
                    })?
                    .finish();

                Ok(operator)
            }
            _ => Err(SyncError::ProviderError {
                provider: "tencent_cos".to_string(),
                message: "expected TencentCos config".to_string(),
            }),
        }
    }

    fn validate_config(&self, config: &ProviderConfig) -> Result<(), SyncError> {
        match config {
            ProviderConfig::TencentCos(cos_config) => {
                if cos_config.endpoint.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "endpoint".to_string(),
                        reason: "endpoint cannot be empty".to_string(),
                    });
                }
                if cos_config.bucket.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "bucket".to_string(),
                        reason: "bucket cannot be empty".to_string(),
                    });
                }
                if cos_config.secret_id.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "secret_id".to_string(),
                        reason: "secret_id cannot be empty".to_string(),
                    });
                }
                if cos_config.secret_key.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "secret_key".to_string(),
                        reason: "secret_key cannot be empty".to_string(),
                    });
                }
                Ok(())
            }
            _ => Err(SyncError::ProviderError {
                provider: "tencent_cos".to_string(),
                message: "expected TencentCos config".to_string(),
            }),
        }
    }
}

#[derive(Debug, Default)]
pub struct HuaweiObsAdapter;

impl HuaweiObsAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl ProviderAdapter for HuaweiObsAdapter {
    fn create_operator(&self, config: &ProviderConfig) -> Result<Operator, SyncError> {
        match config {
            ProviderConfig::HuaweiObs(obs_config) => {
                let mut builder = opendal::services::S3::default()
                    .bucket(&obs_config.bucket)
                    .access_key_id(&obs_config.access_key_id)
                    .secret_access_key(&obs_config.secret_access_key)
                    .endpoint(&obs_config.endpoint);

                if !obs_config.root_path.is_empty() {
                    builder = builder.root(&obs_config.root_path);
                }

                let operator = Operator::new(builder)
                    .map_err(|e| SyncError::ProviderError {
                        provider: "huawei_obs".to_string(),
                        message: format!("failed to create operator: {}", e),
                    })?
                    .finish();

                Ok(operator)
            }
            _ => Err(SyncError::ProviderError {
                provider: "huawei_obs".to_string(),
                message: "expected HuaweiObs config".to_string(),
            }),
        }
    }

    fn validate_config(&self, config: &ProviderConfig) -> Result<(), SyncError> {
        match config {
            ProviderConfig::HuaweiObs(obs_config) => {
                if obs_config.endpoint.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "endpoint".to_string(),
                        reason: "endpoint cannot be empty".to_string(),
                    });
                }
                if obs_config.bucket.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "bucket".to_string(),
                        reason: "bucket cannot be empty".to_string(),
                    });
                }
                if obs_config.access_key_id.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "access_key_id".to_string(),
                        reason: "access_key_id cannot be empty".to_string(),
                    });
                }
                if obs_config.secret_access_key.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "secret_access_key".to_string(),
                        reason: "secret_access_key cannot be empty".to_string(),
                    });
                }
                Ok(())
            }
            _ => Err(SyncError::ProviderError {
                provider: "huawei_obs".to_string(),
                message: "expected HuaweiObs config".to_string(),
            }),
        }
    }
}

#[derive(Debug, Default)]
pub struct UpyunAdapter;

impl UpyunAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl ProviderAdapter for UpyunAdapter {
    fn create_operator(&self, config: &ProviderConfig) -> Result<Operator, SyncError> {
        match config {
            ProviderConfig::Upyun(_) => Err(SyncError::ProviderNotSupported {
                provider: "upyun".to_string(),
            }),
            _ => Err(SyncError::ProviderError {
                provider: "upyun".to_string(),
                message: "expected Upyun config".to_string(),
            }),
        }
    }

    fn validate_config(&self, config: &ProviderConfig) -> Result<(), SyncError> {
        match config {
            ProviderConfig::Upyun(upyun_config) => {
                if upyun_config.bucket.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "bucket".to_string(),
                        reason: "bucket cannot be empty".to_string(),
                    });
                }
                if upyun_config.operator.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "operator".to_string(),
                        reason: "operator cannot be empty".to_string(),
                    });
                }
                if upyun_config.operator_password.trim().is_empty() {
                    return Err(SyncError::ConfigValidationFailed {
                        field: "operator_password".to_string(),
                        reason: "operator_password cannot be empty".to_string(),
                    });
                }
                Ok(())
            }
            _ => Err(SyncError::ProviderError {
                provider: "upyun".to_string(),
                message: "expected Upyun config".to_string(),
            }),
        }
    }
}
