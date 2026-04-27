pub mod adapters;
pub mod metadata;
pub mod oauth2;
pub mod provider;
pub mod providers;
pub mod record;
pub mod schema;
pub mod storage;
pub mod validation;

pub use adapters::{
    AliyunDriveAdapter, AliyunOssAdapter, DropboxAdapter, GoogleDriveAdapter, HuaweiObsAdapter,
    ICloudAdapter, OneDriveAdapter, S3Adapter, SftpAdapter, TencentCosAdapter, UpyunAdapter,
};
pub use metadata::{CloudMetadata, DeviceInfo, RecordVersionInfo};
pub use provider::{create_cloud_storage, provider_name, ProviderAdapter};
pub use record::{AadFields, CloudRecord, ConflictPayload, RecordMetadata};
pub use schema::*;
pub use storage::CloudStorage;
pub use validation::{compute_checksum, validate_aad, validate_uuid};
