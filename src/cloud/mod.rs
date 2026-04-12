pub mod metadata;
pub mod provider;
pub mod record;
pub mod schema;
pub mod storage;
pub mod validation;

pub use metadata::{CloudMetadata, DeviceInfo, RecordVersionInfo};
pub use provider::{create_cloud_storage, provider_name, ProviderAdapter};
pub use record::{AadFields, CloudRecord, ConflictPayload, RecordMetadata};
pub use schema::*;
pub use storage::CloudStorage;
pub use validation::{compute_checksum, validate_aad, validate_uuid};
