//! Cloud provider adapters module.
//!
//! Individual adapter implementations for each cloud storage provider.

mod icloud;
mod oauth2;
mod s3_compatible;
mod sftp;

pub use icloud::ICloudAdapter;
pub use oauth2::{AliyunDriveAdapter, DropboxAdapter, GoogleDriveAdapter, OneDriveAdapter};
pub use s3_compatible::{
    AliyunOssAdapter, HuaweiObsAdapter, S3Adapter, TencentCosAdapter, UpyunAdapter,
};
pub use sftp::SftpAdapter;
