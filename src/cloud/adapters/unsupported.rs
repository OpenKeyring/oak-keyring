use opendal::Operator;

use crate::cloud::ProviderAdapter;
use crate::config::sync::ProviderConfig;
use crate::errors::mapping::sync::SyncError;

macro_rules! unsupported_adapter {
    ($name:ident, $provider:literal) => {
        #[derive(Debug, Default)]
        pub struct $name;

        impl $name {
            pub fn new() -> Self {
                Self
            }
        }

        impl ProviderAdapter for $name {
            fn create_operator(&self, _config: &ProviderConfig) -> Result<Operator, SyncError> {
                Err(SyncError::ProviderNotSupported {
                    provider: $provider.to_string(),
                })
            }

            fn validate_config(&self, _config: &ProviderConfig) -> Result<(), SyncError> {
                Ok(())
            }
        }
    };
}

unsupported_adapter!(S3Adapter, "s3");
unsupported_adapter!(AliyunOssAdapter, "aliyun_oss");
unsupported_adapter!(TencentCosAdapter, "tencent_cos");
unsupported_adapter!(HuaweiObsAdapter, "huawei_obs");
unsupported_adapter!(UpyunAdapter, "upyun");
unsupported_adapter!(SftpAdapter, "sftp");
