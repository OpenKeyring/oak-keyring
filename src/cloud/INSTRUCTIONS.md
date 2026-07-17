# Cloud Layer (D4)

An OpenDAL-based cloud storage abstraction layer providing a unified read/write interface.

## ProviderAdapter Trait

```rust
trait ProviderAdapter: Send + Sync {
    fn create_operator(&self, config: &ProviderConfig) -> Result<Operator, SyncError>;
    fn validate_config(&self, config: &ProviderConfig) -> Result<(), SyncError>;
    fn normalize_path(&self, base: &str, file: &str) -> String;  // optional
    fn needs_watcher(&self) -> bool { false }                     // only iCloud returns true
    fn refresh_auth(&self, operator: &mut Operator) -> Result<()>; // optional
}
```

## Adapter Files and Provider Mapping

| Adapter File | Providers Covered |
|-------------|----------------|
| `s3_compatible.rs` | S3, Aliyun OSS, Tencent COS, Huawei OBS (via different endpoint configurations) |
| `oauth2.rs` | Google Drive, Dropbox (OAuth2 PKCE / client credentials) |
| `icloud.rs` | iCloud Drive (local filesystem ~/Library/Mobile Documents) |
| `sftp.rs` | SFTP (SSH key authentication) |

## Adapter Support Status

| Adapter | Status | Authentication Method |
|---------|------|----------|
| **S3** | ✅ | access_key_id + secret_access_key |
| **Aliyun OSS** | ✅ | access_key_id + access_key_secret + endpoint |
| **Tencent COS** | ✅ | secret_id + secret_key + endpoint |
| **Huawei OBS** | ✅ | access_key_id + secret_access_key + endpoint |
| **WebDAV** | ✅ | bearer_token or username + password |
| **iCloud** | ✅ | Local filesystem (~/Library/Mobile Documents) |
| **SFTP** | ✅ | SSH key; port 22 auto-filled |
| **Dropbox** | ⚠️ | client_id + client_secret + refresh_token; `create_operator()` available, browser OAuth2 authorization flow to be implemented |
| **Google Drive** | ✅ | OAuth2 PKCE (drive.file scope, built-in credentials, browser authorization); root_path defaults to `.oak-keyring/`; tokens stored in TokenStore |
| OneDrive | ❌ Deferred | `create_operator()` returns ProviderNotSupported; plans to use a browser OAuth2 flow |
| Aliyun Drive | ❌ Deferred | `create_operator()` returns ProviderNotSupported; implementation deferred |
| Upyun | ❌ Deferred | `create_operator()` returns ProviderNotSupported; implementation deferred |

The S3-compatible series is implemented via `opendal::services::S3` configured with different endpoints.

## CloudStorage

- **Atomic write**: upload → `.tmp.{uuid}` → rename, preventing corruption on interruption
- **Idempotent delete**: returns Ok even when the file does not exist
- **Batch download**: `batch_download_records()` uses `JoinSet` for concurrency

## Cloud Data Format

```
/
├── .metadata.json          # CloudMetadata
├── .sync.lock              # distributed lock
└── records/
    └── {uuid}.json         # CloudRecord
```

**CloudRecord**: id, version(≥1), encrypted_data(Base64), nonce(Base64), dek_version(≥1), aad, metadata, deleted

**CloudMetadata**: version=1, schema="open-keyring-v1", metadata_version(monotonically increasing), vault_identity_token, devices[], records{}

**Validation rules**: AAD record_id and dek_version must match the CloudRecord fields; checksum = SHA-256(encrypted_data decoded)

## iCloud Special Handling

```rust
// path construction
dirs::document_dir()/../Library/Mobile Documents/{user_path}
// needs_watcher() = true — uses notify crate to watch file changes
```

## Extending a New Adapter

1. Create a new file under `adapters/`
2. Implement the `ProviderAdapter` trait
3. Export it in `adapters/mod.rs`
4. Add a match branch in `provider.rs` within `create_cloud_storage()` and `provider_name()`

## File Organization

- **`mod.rs`**: module declarations + re-exports ONLY. Zero business logic.
- **Tests**: standalone `*_test.rs` files alongside the source file (e.g. `provider.rs` + `provider_test.rs`)
5. Add the corresponding ProviderConfig variant in `config::sync.rs`
