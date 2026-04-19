# oak-keyring

oak (橡木) 终端 TUI 密码管理器，基于 Rust 开发。

## Binary Target

- `ok` — 主 TUI 密码管理器 (`src/main.rs`)

## Quick Start

```bash
cargo build                          # 编译
cargo run                            # 运行 (bin: ok)
cargo test                           # 全部测试
cargo test --test integration        # 集成测试
cargo fmt && cargo clippy -- -D warnings  # 格式化 + lint
```

## Architecture Overview

采用 TEA (The Elm Architecture) + Command Pattern 分层架构：

```
UI Layer (tui/)  →  Command/Message (commands/)  →  Executor (executor/)  →  Services (services/)  →  Data Layer (db/, crypto/, types/, config/)
                                                                                       ↕                                     ↕
                                                                                  Sync Engine (sync/)  ←→  Cloud Layer (cloud/)
                                                                                                       ↘ errors/ (跨层共享)
```

三层依赖关系：
- **Data Layer (D0-D4)**: 基础数据结构、加密、配置、云存储适配 — 无外部业务依赖
- **Service Layer (S0-S7)**: 核心业务逻辑 — 仅依赖 Data Layer 和 Sync Engine
- **UI Layer (U1-U11)**: TUI 界面 — 仅通过 Command/Message 与 Service 交互

## Project Directory Structure

```
oak-keyring/
├── src/
│   ├── main.rs                          # 应用入口
│   ├── lib.rs                           # 库导出 (re-export 12 模块 + t! macro)
│   ├── app/
│   │   ├── mod.rs                       # App struct, run(), 事件循环
│   │   ├── signal.rs                    # Unix signal 处理
│   │   ├── update.rs                    # App::update() 路由
│   │   └── view.rs                      # App::view() 渲染入口
│   │
│   ├── commands/                        # S0: Command/Message 类型定义
│   │   ├── mod.rs
│   │   ├── command.rs                   # Command enum
│   │   ├── result.rs                    # CommandResult enum
│   │   ├── message.rs                   # Message enum (TEA 事件)
│   │   └── types.rs                     # RecordFilter, RecordSort, FieldSelector 等
│   │
│   ├── executor/                        # S7: Command Executor (UI→Service 桥梁)
│   │   ├── mod.rs                       # CommandExecutor + new() + run()
│   │   ├── execute.rs                   # 主分发 + pre_check + post_hook
│   │   ├── record.rs                    # Record 操作
│   │   ├── clipboard.rs                 # 剪贴板操作
│   │   ├── sync.rs                      # 同步操作
│   │   ├── health.rs                    # 健康检查操作
│   │   ├── import_export.rs             # 导入/导出操作
│   │   ├── rotation.rs                  # 密钥轮转操作
│   │   ├── vault.rs                     # Vault 解锁/锁定/初始化
│   │   ├── config.rs                    # 配置 Command 定义
│   │   ├── config_impl.rs              # 配置操作实现
│   │   ├── cancellation.rs              # 任务取消管理
│   │   └── timer.rs                     # 定时器管理
│   │
│   ├── types/                           # D1: 核心数据模型
│   │   ├── mod.rs
│   │   ├── sensitive.rs                 # SecureString<T> (zeroize)
│   │   ├── credential.rs                # CredentialType, EncryptedPayload
│   │   ├── record.rs                    # StoredRecord, DecryptedRecord, TuiRecord
│   │   ├── tag.rs                       # Tag
│   │   ├── history.rs                   # PasswordHistory
│   │   ├── audit.rs                     # AuditEntry, AuditOperation
│   │   ├── rotation.rs                  # Rotation 相关类型
│   │   └── sync.rs                      # SyncStatus, SyncState, SyncStats
│   │
│   ├── crypto/                          # D2: 加密层 (无外部业务依赖)
│   │   ├── mod.rs                       # re-export CryptoManager 等
│   │   ├── crypto_manager.rs            # CryptoManager 实现
│   │   ├── keystore.rs                  # KeyStore (wrapped SK 持久化)
│   │   ├── argon2.rs                    # Argon2id KDF
│   │   ├── xchacha20.rs                 # XChaCha20-Poly1305 AEAD
│   │   ├── hkdf.rs                      # HKDF 密钥派生链
│   │   ├── bip39.rs                     # BIP39 助记词生成/恢复
│   │   ├── password.rs                  # 密码生成器
│   │   ├── strength.rs                  # 密码强度评估
│   │   └── payload.rs                   # Record payload 加解密
│   │
│   ├── cloud/                           # D4: 云存储适配层
│   │   ├── mod.rs                       # re-export adapters, storage, schema 等
│   │   ├── provider.rs                  # ProviderAdapter trait, create_cloud_storage()
│   │   ├── storage.rs                   # CloudStorage 抽象
│   │   ├── schema.rs                    # 云端数据 Schema
│   │   ├── record.rs                    # CloudRecord, RecordMetadata, AadFields
│   │   ├── metadata.rs                  # CloudMetadata, DeviceInfo
│   │   ├── validation.rs               # checksum, AAD/UUID 验证
│   │   └── adapters/                    # 各云存储适配器
│   │       ├── mod.rs
│   │       ├── icloud.rs                # iCloud
│   │       ├── s3_compatible.rs         # S3/AliyunOss/TencentCos/HuaweiObs/Upyun
│   │       ├── sftp.rs                  # SFTP
│   │       └── oauth2.rs               # OAuth2 (GoogleDrive/OneDrive/Dropbox)
│   │
│   ├── sync/                            # 同步引擎
│   │   ├── mod.rs                       # re-export 核心 sync 类型
│   │   ├── state_machine.rs             # SyncStateMachine, SyncState, SyncTrigger
│   │   ├── pipeline.rs                  # SyncPipeline (多阶段: Detect/PullMetadata/Push/Resolve)
│   │   ├── conflict.rs                  # ConflictManager, ResolutionStrategy, ResolveOutcome
│   │   ├── checkpoint.rs                # SyncCheckpoint, PendingConflict
│   │   ├── lock.rs                      # SyncLock, LockFileData
│   │   ├── task.rs                      # SyncTask, SyncCommand, SyncEvent, SyncReport
│   │   ├── retry.rs                     # RetryPolicy, BackoffTimer
│   │   ├── nonce_validator.rs           # NonceValidator, IdentityAction
│   │   └── watcher.rs                   # SyncWatcher, WatchEventKind
│   │
│   ├── services/                        # S1-S6: 业务服务层
│   │   ├── mod.rs
│   │   ├── vault/                       # S1: Vault Service (SQLite CRUD + 加解密)
│   │   │   ├── mod.rs
│   │   │   ├── record.rs                # 记录 CRUD
│   │   │   ├── search.rs                # 搜索/过滤
│   │   │   ├── tag.rs                   # 标签管理
│   │   │   ├── trash.rs                 # 回收站
│   │   │   ├── audit.rs                 # 审计记录
│   │   │   ├── history.rs               # 密码历史
│   │   │   └── metadata.rs              # 元数据管理
│   │   ├── sync.rs                      # S2: 同步编排 (使用 sync/ + cloud/)
│   │   ├── health.rs                    # S3: 密码健康检查
│   │   ├── clipboard.rs                 # S4: 系统剪贴板
│   │   ├── import_export/               # S5: 导入/导出
│   │   │   ├── mod.rs
│   │   │   ├── export.rs                # 导出逻辑
│   │   │   ├── parser.rs                # 解析器分发
│   │   │   ├── mapping.rs               # 字段映射
│   │   │   ├── duplicate.rs             # 重复检测
│   │   │   ├── validation.rs            # 导入验证
│   │   │   ├── types.rs                 # 导入/导出类型
│   │   │   └── parsers/                 # 各格式解析器
│   │   │       ├── mod.rs
│   │   │       ├── bitwarden.rs         # Bitwarden JSON
│   │   │       ├── csv.rs               # 通用 CSV
│   │   │       ├── keepass.rs           # KeePass .kdbx
│   │   │       ├── okb.rs               # oak-keyring backup
│   │   │       └── onepassword.rs       # 1Password .1pux
│   │   └── rotation.rs                  # S6: 密钥/密码轮转
│   │
│   ├── db/                              # D0: 数据库层
│   │   ├── mod.rs                       # 数据库初始化
│   │   ├── schema.rs                    # 表定义
│   │   ├── models.rs                    # 数据库行模型
│   │   └── queries.rs                   # SQL 查询构建
│   │
│   ├── config/                          # D3: 配置管理
│   │   ├── mod.rs                       # AppConfig + load/save
│   │   ├── manager.rs                   # ConfigManager (配置文件读写)
│   │   ├── error.rs                     # ConfigError
│   │   ├── general.rs                   # GeneralConfig
│   │   ├── sync.rs                      # SyncConfig, SyncProvider
│   │   ├── security.rs                  # SecurityConfig
│   │   ├── password.rs                  # PasswordDefaultsConfig
│   │   ├── notification.rs              # NotificationConfig
│   │   ├── validation.rs                # 配置验证逻辑
│   │   ├── watcher.rs                   # 配置文件变更监控
│   │   └── config_test.rs               # 配置单元测试
│   │
│   ├── errors/                          # 错误处理 (跨层)
│   │   ├── mod.rs                       # 导出 ErrorCode, ErrorLevel, ErrorContext
│   │   ├── code.rs                      # ErrorCode enum
│   │   ├── context.rs                   # ErrorContext struct
│   │   ├── level.rs                     # ErrorLevel enum
│   │   ├── service_error.rs             # ServiceError trait
│   │   └── mapping/                     # 各服务错误映射
│   │       ├── mod.rs
│   │       ├── vault.rs
│   │       ├── sync.rs
│   │       ├── crypto.rs
│   │       ├── clipboard.rs
│   │       ├── data.rs
│   │       ├── health.rs
│   │       ├── import_export.rs
│   │       └── rotation.rs
│   │
│   └── tui/                             # U1-U11: TUI 界面层
│       ├── mod.rs
│       ├── terminal.rs                  # 终端 setup/restore
│       ├── theme.rs                     # 主题/颜色定义
│       ├── screens/
│       │   ├── mod.rs                   # Screen trait + ScreenContext
│       │   ├── unlock.rs                # U1: 解锁屏幕
│       │   ├── onboarding.rs            # U1: 引导屏幕
│       │   ├── recovery_key.rs          # U1: 恢复密钥
│       │   ├── set_password.rs          # U1: 设置主密码
│       │   ├── change_master_password.rs # U1: 修改主密码
│       │   ├── main/                    # U2: 主布局
│       │   │   ├── mod.rs               # MainScreen
│       │   │   ├── layout.rs            # 三栏布局
│       │   │   ├── sidebar.rs           # SidebarPanel
│       │   │   ├── list.rs              # U3: ListPanel
│       │   │   ├── detail.rs            # U4: DetailPanel
│       │   │   ├── status_bar.rs        # StatusBar
│       │   │   └── overlay/             # U5: Overlay System
│       │   │       ├── mod.rs           # OverlayManager
│       │   │       ├── help.rs
│       │   │       ├── confirm.rs
│       │   │       ├── password_history.rs
│       │   │       ├── batch_tag.rs
│       │   │       ├── error_dialog.rs
│       │   │       └── generator.rs     # 内联密码生成器
│       │   ├── form/                    # U7: 表单通用逻辑
│       │   │   ├── mod.rs
│       │   │   ├── render.rs            # 表单渲染
│       │   │   └── validation.rs        # 表单验证
│       │   ├── create_record.rs         # U7: 创建记录
│       │   ├── edit_record.rs           # U7: 编辑记录
│       │   ├── password_generator.rs    # U6: 密码生成器
│       │   ├── config_screen.rs         # U8: 配置屏幕入口
│       │   ├── config_screen/           # U8: 配置屏幕子模块
│       │   │   ├── mod.rs
│       │   │   └── config/
│       │   │       ├── mod.rs
│       │   │       ├── about.rs
│       │   │       ├── general.rs
│       │   │       ├── security.rs
│       │   │       ├── sync.rs
│       │   │       └── render.rs
│       │   ├── import_export.rs         # U9: 导入/导出
│       │   ├── audit_log.rs             # U10: 审计日志
│       │   └── sync_conflict.rs         # U10: 同步冲突
│       │
│       ├── state/                       # UI 状态管理
│       │   ├── mod.rs                   # AppState, AppPhase, ScreenStates
│       │   ├── focus.rs                 # FocusState
│       │   ├── notification.rs          # NotificationState
│       │   ├── loading.rs               # LoadingState
│       │   ├── animation.rs             # AnimationState
│       │   ├── overlay_state.rs         # OverlayState
│       │   ├── main_state.rs            # MainState
│       │   ├── list_state.rs            # ListState
│       │   ├── detail_state.rs          # DetailState
│       │   ├── form_state.rs            # FormState
│       │   ├── generator_state.rs       # GeneratorState
│       │   ├── config_state.rs          # ConfigState
│       │   ├── audit_state.rs           # AuditState
│       │   ├── sync_ui_state.rs         # SyncUiState
│       │   └── tag_management.rs        # TagManagementState
│       │
│       ├── animation/                   # 动画系统
│       │   ├── mod.rs                   # AnimationLevel + 检测
│       │   ├── effects.rs               # tachyonfx Effect 构建
│       │   └── transitions.rs           # 页面过渡编排
│       │
│       ├── components/                  # U11: 通用 UI 组件
│       │   ├── mod.rs
│       │   ├── spinner.rs
│       │   ├── progress_bar.rs
│       │   ├── empty_state.rs
│       │   ├── inline_validation.rs
│       │   ├── text_input.rs
│       │   ├── dropdown.rs
│       │   ├── length_slider.rs
│       │   ├── strength_bar.rs
│       │   ├── tag_input.rs
│       │   ├── generator_panel.rs
│       │   ├── sync_indicator.rs
│       │   └── vault_path_dialog.rs
│       │
│       ├── traits/                      # UI trait 定义
│       │   ├── mod.rs
│       │   ├── screen.rs                # Screen trait
│       │   └── component.rs             # Component trait
│       │
│       └── i18n/                        # 国际化
│           └── mod.rs                   # i18n 初始化、语言切换、locale 规范化
│
├── locales/                             # i18n 翻译文件
│   ├── en.yml
│   └── zh-CN.yml
│
├── tests/                               # 测试目录
│   ├── main_integration_test.rs         # 集成测试入口
│   ├── integration/                     # 集成测试 (12 个模块)
│   │   ├── main.rs
│   │   ├── audit_sync_test.rs
│   │   ├── clipboard_test.rs
│   │   ├── db_lifecycle_test.rs
│   │   ├── error_propagation_test.rs
│   │   ├── executor_command_test.rs
│   │   ├── executor_dispatch_test.rs
│   │   ├── health_test.rs
│   │   ├── i18n_test.rs
│   │   ├── rotation_test.rs
│   │   ├── ui_entry_test.rs
│   │   ├── vault_db_crypto_test.rs
│   │   └── vault_service_test.rs
│   ├── snapshot_tests/                  # 快照测试源码
│   │   ├── main.rs
│   │   ├── mod.rs
│   │   └── screens/
│   │       ├── mod.rs
│   │       ├── detail_test.rs
│   │       └── snapshots/               # insta 快照文件
│   ├── e2e/                             # 端到端测试
│   │   └── mod.rs
│   ├── sync_checkpoint_test.rs          # 同步检查点测试
│   ├── sync_conflict_test.rs            # 同步冲突测试
│   ├── sync_e2e_test.rs                 # 同步端到端测试
│   ├── sync_pipeline_test.rs            # 同步管道测试
│   └── sync_retry_test.rs               # 同步重试测试
│
├── .insta.toml                          # insta 配置
├── build.rs                             # build script (监听 locales/ 变更)
├── Cargo.toml
└── CLAUDE.md
```

## Spec ↔ Module Mapping

| Spec | Module | Description |
|------|--------|-------------|
| D0 | `src/db/` | 数据库 Schema |
| D1 | `src/types/` | 核心数据模型 |
| D2 | `src/crypto/` | 加密层 |
| D3 | `src/config/` | 配置管理 |
| D4 | `src/cloud/` | 云存储适配层 (iCloud/S3/SFTP/OAuth2) |
| S0 | `src/commands/` | Command/Message 类型 |
| S1 | `src/services/vault/` | Vault Service (record/search/tag/trash/audit/history) |
| S2 | `src/services/sync.rs` + `src/sync/` | 同步编排 + 同步引擎 (状态机/管道/冲突) |
| S3 | `src/services/health.rs` | Health Service |
| S4 | `src/services/clipboard.rs` | Clipboard Service |
| S5 | `src/services/import_export/` | 导入/导出 (Bitwarden/KeePass/1Password/CSV/OKB) |
| S6 | `src/services/rotation.rs` | 密钥/密码轮转 |
| S7 | `src/executor/` | Command Executor |
| U1 | `src/tui/screens/unlock.rs`, `onboarding.rs`, `recovery_key.rs`, `set_password.rs`, `change_master_password.rs` | 入口/认证屏幕 |
| U2 | `src/tui/screens/main/` | 主布局 |
| U3 | `src/tui/screens/main/list.rs` | 密码列表 |
| U4 | `src/tui/screens/main/detail.rs` | 密码详情 |
| U5 | `src/tui/screens/main/overlay/` | Overlay 系统 |
| U6 | `src/tui/screens/password_generator.rs` | 密码生成器 |
| U7 | `src/tui/screens/create_record.rs`, `edit_record.rs`, `form/` | 创建/编辑表单 |
| U8 | `src/tui/screens/config_screen.rs`, `config_screen/` | 配置屏幕 |
| U9 | `src/tui/screens/import_export.rs` | 导入/导出 |
| U10 | `src/tui/screens/audit_log.rs`, `sync_conflict.rs` | 审计/同步 UI |
| U11 | `src/tui/components/`, `state/`, `animation/` | 跨层 UI |

## Key Dependencies

| 用途 | Crate |
|------|-------|
| 数据库 | `rusqlite` v0.34 |
| KDF | `argon2` v0.5 |
| AEAD | `chacha20poly1305` v0.10 |
| 密钥派生 | `hkdf` v0.12, `sha2` v0.10 |
| SHA-1 | `sha1` v0.10 |
| CSPRNG | `rand` v0.9 |
| 内存清零 | `zeroize` v1.8 |
| 常量比较 | `subtle` v2.5 |
| 助记词 | `bip39` v2.0 |
| Hex 编码 | `hex` v0.4 |
| 异步运行时 | `tokio` v1 |
| 异步工具 | `tokio-util` v0.7 |
| TUI 框架 | `ratatui` v0.30 |
| 终端后端 | `crossterm` v0.29 |
| 动画效果 | `tachyonfx` v0.20.1 |
| 序列化 | `serde` v1, `serde_json` v1 |
| 配置文件 | `toml` v0.8 |
| Base64 | `base64` v0.22 |
| CSV 解析 | `csv` v1 |
| KeePass 解析 | `keepass` v0.7 |
| ZIP 解压 | `zip` v2 |
| HTTP 客户端 | `ureq` v3 |
| 国际化 | `rust-i18n` v3, `sys-locale` v0.3 |
| 剪贴板 | `arboard` v3 |
| 错误处理 | `thiserror` v2 |
| 日志 | `tracing` v0.1, `tracing-subscriber` v0.3 |
| UUID | `uuid` v1 |
| 时间 | `chrono` v0.4 |
| 路径 | `dirs` v6 |
| 云存储抽象 | `opendal` v0.55 |
| 文件监控 | `notify` v7 |
| 快照测试 | `insta` v1 (dev) |
| Mock | `mockall` v0.13 (dev) |
| 临时文件 | `tempfile` v3 (dev) |
| 错误上下文 | `anyhow` v1 (dev) |

## Dependency Rules (模块依赖规则)

严格遵循单向依赖，禁止循环依赖：

```
tui/ → commands/ → executor/ → services/ → {db/, crypto/, types/, config/}
                                        ↘ errors/ (跨层共享)

services/sync.rs → sync/ → cloud/ → types/
services/import_export/ → parsers/ (bitwarden, keepass, csv, onepassword, okb)
```

1. **Data Layer 不依赖 Service/UI** — `db/`, `types/`, `crypto/`, `config/`, `cloud/` 互不依赖 (crypto 可依赖 types，types 仅依赖 db)
2. **cloud/ 仅依赖 types/** — 云存储适配层不依赖 services/ 或 sync/
3. **sync/ 依赖 cloud/ 和 types/** — 同步引擎不依赖 services/
4. **Service Layer 不依赖 UI** — `services/` 仅依赖 Data Layer、cloud/、sync/ 和 `commands/`
5. **UI Layer 通过 Command/Message 与 Service 交互** — `tui/` 仅依赖 `commands/`，不直接调用 `services/`
6. **errors/ 跨层共享** — 所有层均可引用 `errors/`

## Test Conventions

- 单元测试放在实现文件旁 (如 `vault.rs` 旁的 `vault_test.rs`，通过 `#[cfg(test)] mod tests` 或独立文件)
- 集成测试放在 `tests/integration/`
- 快照测试放在 `tests/snapshot_tests/`，快照文件在 `tests/snapshots/`
- E2E 测试放在 `tests/e2e/`
- 同步测试放在 `tests/` 根目录 (`sync_*_test.rs`)
- 使用 `insta` 快照测试 (`.insta.toml`)

## WorkTree

Git worktree directory: `./.worktrees`
REQUIRED: When using `subagent-driven-development` for development, it is MUST to follow the `using-git-worktree` skill to create a working directory (`./.worktrees`) for the work. Besides this, it is not allowed to create the working directory in any other location.
