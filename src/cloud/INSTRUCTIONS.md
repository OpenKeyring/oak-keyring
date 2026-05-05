# Cloud Layer (D4)

基于 OpenDAL 的云存储抽象层，提供统一的读写接口。

## ProviderAdapter Trait

```rust
trait ProviderAdapter: Send + Sync {
    fn create_operator(&self, config: &ProviderConfig) -> Result<Operator, SyncError>;
    fn validate_config(&self, config: &ProviderConfig) -> Result<(), SyncError>;
    fn normalize_path(&self, base: &str, file: &str) -> String;  // 可选
    fn needs_watcher(&self) -> bool { false }                     // 仅 iCloud true
    fn refresh_auth(&self, operator: &mut Operator) -> Result<()>; // 可选
}
```

## Adapter 支持状态

| Adapter | 状态 | 认证方式 |
|---------|------|----------|
| **S3** | ✅ | access_key_id + secret_access_key |
| **Aliyun OSS** | ✅ | access_key_id + access_key_secret + endpoint |
| **Tencent COS** | ✅ | secret_id + secret_key + endpoint |
| **Huawei OBS** | ✅ | access_key_id + secret_access_key + endpoint |
| **WebDAV** | ✅ | bearer_token 或 username + password |
| **iCloud** | ✅ | 本地文件系统 (~/Library/Mobile Documents) |
| **SFTP** | ✅ | SSH key，自动补端口 22 |
| **Dropbox** | ⚠️ | client_id + client_secret + refresh_token; `create_operator()` 可用，浏览器 OAuth2 授权流程待实现 |
| **Google Drive** | ✅ | OAuth2 PKCE (drive.file scope, built-in credentials, browser authorization); root_path defaults to `.oak-keyring/`; tokens stored in TokenStore |
| OneDrive | ❌ Deferred | `create_operator()` 返回 ProviderNotSupported；计划采用浏览器 OAuth2 流程 |
| Aliyun Drive | ❌ Deferred | `create_operator()` 返回 ProviderNotSupported；延后实现 |
| Upyun | ❌ Deferred | `create_operator()` 返回 ProviderNotSupported；延后实现 |

S3 兼容系列均通过 `opendal::services::S3` 配置不同 endpoint 实现。

## CloudStorage

- **原子写入**: upload → `.tmp.{uuid}` → rename，防中断损坏
- **幂等删除**: 文件不存在也返回 Ok
- **批量下载**: `batch_download_records()` 使用 `JoinSet` 并发

## Cloud Data Format

```
/
├── .metadata.json          # CloudMetadata
├── .sync.lock              # 分布式锁
└── records/
    └── {uuid}.json         # CloudRecord
```

**CloudRecord**: id, version(≥1), encrypted_data(Base64), nonce(Base64), dek_version(≥1), aad, metadata, deleted

**CloudMetadata**: version=1, schema="open-keyring-v1", metadata_version(单调递增), vault_identity_token, devices[], records{}

**验证规则**: AAD record_id 和 dek_version 必须匹配 CloudRecord 字段；checksum = SHA-256(encrypted_data decoded)

## iCloud 特殊处理

```rust
// 路径构造
dirs::document_dir()/../Library/Mobile Documents/{user_path}
// needs_watcher() = true — 使用 notify crate 监控文件变更
```

## 扩展新 Adapter

1. 在 `adapters/` 创建新文件
2. 实现 `ProviderAdapter` trait
3. 在 `adapters/mod.rs` 导出
4. 在 `provider.rs` 的 `create_cloud_storage()` 和 `provider_name()` 添加匹配分支

## File Organization

- **`mod.rs`**: module declarations + re-exports ONLY. Zero business logic.
- **Tests**: standalone `*_test.rs` files alongside the source file (e.g. `provider.rs` + `provider_test.rs`)
5. 在 `config::sync.rs` 添加对应的 ProviderConfig variant
