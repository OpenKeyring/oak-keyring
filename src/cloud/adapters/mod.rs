//! Cloud provider adapters module.
//!
//! Individual adapter implementations for each cloud storage provider.

mod icloud;
mod oauth2;
mod unsupported;

pub use icloud::ICloudAdapter;
pub use oauth2::{AliyunDriveAdapter, DropboxAdapter, GoogleDriveAdapter, OneDriveAdapter};
pub use unsupported::SftpAdapter;
pub use unsupported::{
    AliyunOssAdapter, HuaweiObsAdapter, S3Adapter, TencentCosAdapter, UpyunAdapter,
};
