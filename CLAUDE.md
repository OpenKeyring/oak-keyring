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
```

三层依赖关系：
- **Data Layer (D0-D3)**: 基础数据结构、加密、配置 — 无外部业务依赖
- **Service Layer (S0-S5)**: 核心业务逻辑 — 仅依赖 Data Layer
- **UI Layer (U1-U11)**: TUI 界面 — 仅通过 Command/Message 与 Service 交互

## Project Directory Structure

```
oak-keyring/
├── src/
│   ├── main.rs                          # 应用入口
│   ├── lib.rs                           # 库导出 (re-export)
│   ├── app/
│   │   ├── mod.rs                       # App struct, run(), 事件循环
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
│   ├── executor/                        # S5: Command Executor (UI→Service 桥梁)
│   │   ├── mod.rs                       # CommandExecutor + new() + run()
│   │   ├── execute.rs                   # 主分发 + pre_check + post_hook
│   │   ├── record.rs                    # Record 操作
│   │   ├── clipboard.rs                 # 剪贴板操作
│   │   ├── sync.rs                      # 同步操作
│   │   ├── health.rs                    # 健康检查操作
│   │   ├── import_export.rs             # 导入/导出操作
│   │   ├── vault.rs                     # Vault 解锁/锁定/初始化
│   │   ├── config.rs                    # 配置加载/保存
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
│   ├── services/                        # S1-S4: 业务服务层
│   │   ├── mod.rs
│   │   ├── vault.rs                     # S1: Vault Service (SQLite CRUD + 加解密)
│   │   ├── sync.rs                      # S2: Sync Service (云端同步)
│   │   ├── health.rs                    # S3: Health Service (密码分析)
│   │   └── clipboard.rs                 # S4: Clipboard Service (系统剪贴板)
│   │
│   ├── db/                              # D0: 数据库层
│   │   ├── mod.rs                       # 数据库初始化
│   │   ├── schema.rs                    # 表定义
│   │   ├── models.rs                    # 数据库行模型
│   │   └── queries.rs                   # SQL 查询构建
│   │
│   ├── config/                          # D3: 配置管理
│   │   ├── mod.rs                       # AppConfig + load/save
│   │   ├── general.rs                   # GeneralConfig
│   │   ├── sync.rs                      # SyncConfig, SyncProvider
│   │   ├── security.rs                  # SecurityConfig
│   │   └── password.rs                  # PasswordDefaultsConfig
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
│   │       └── data.rs
│   │
│   └── tui/                             # U1-U11: TUI 界面层
│       ├── mod.rs
│       ├── screens/
│       │   ├── mod.rs                   # Screen trait + ScreenContext
│       │   ├── unlock.rs                # U1: 解锁屏幕
│       │   ├── onboarding.rs            # U1: 引导屏幕
│       │   ├── main/                    # U2: 主布局
│       │   │   ├── mod.rs               # MainScreen (三栏布局)
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
│       │   │       └── error_dialog.rs
│       │   ├── create_record.rs         # U7: 创建记录
│       │   ├── edit_record.rs           # U7: 编辑记录
│       │   ├── password_generator.rs    # U6: 密码生成器
│       │   ├── config_screen.rs         # U8: 配置屏幕
│       │   ├── import_export.rs         # U9: 导入/导出
│       │   ├── audit_log.rs             # U10: 审计日志
│       │   └── sync_conflict.rs         # U10: 同步冲突
│       │
│       ├── state/                       # UI 状态管理
│       │   ├── mod.rs                   # AppState, AppPhase, ScreenStates
│       │   ├── focus.rs                 # FocusState
│       │   ├── notification.rs          # NotificationState
│       │   ├── loading.rs               # LoadingState
│       │   └── animation.rs             # AnimationState
│       │
│       ├── animation/                   # 动画系统
│       │   ├── mod.rs                   # AnimationLevel + 检测
│       │   ├── effects.rs               # tachyonfx Effect 构建
│       │   └── transitions.rs           # 页面过渡编排
│       │
│       ├── components/                  # U11: 通用 UI 组件
│       │   ├── spinner.rs
│       │   ├── progress_bar.rs
│       │   ├── empty_state.rs
│       │   └── inline_validation.rs
│       │
│       ├── traits/                      # UI trait 定义
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
│   ├── integration/                     # 集成测试
│   │   ├── executor_command_test.rs
│   │   ├── vault_db_crypto_test.rs
│   │   └── error_propagation_test.rs
│   ├── snapshots/                       # insta 快照文件
│   ├── snapshot_tests/                  # 快照测试源码 (screens/, components/)
│   └── e2e/                             # 端到端测试
│
├── .insta.toml                          # insta 配置
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
| S0 | `src/commands/` | Command/Message 类型 |
| S1 | `src/services/vault.rs` | Vault Service |
| S2 | `src/services/sync.rs` | Sync Service |
| S3 | `src/services/health.rs` | Health Service |
| S4 | `src/services/clipboard.rs` | Clipboard Service |
| S5 | `src/executor/` | Command Executor |
| U1 | `src/tui/screens/unlock.rs`, `onboarding.rs` | 入口屏幕 |
| U2 | `src/tui/screens/main/` | 主布局 |
| U3 | `src/tui/screens/main/list.rs` | 密码列表 |
| U4 | `src/tui/screens/main/detail.rs` | 密码详情 |
| U5 | `src/tui/screens/main/overlay/` | Overlay 系统 |
| U6 | `src/tui/screens/password_generator.rs` | 密码生成器 |
| U7 | `src/tui/screens/create_record.rs`, `edit_record.rs` | 创建/编辑表单 |
| U8 | `src/tui/screens/config_screen.rs` | 配置屏幕 |
| U9 | `src/tui/screens/import_export.rs` | 导入/导出 |
| U10 | `src/tui/screens/audit_log.rs`, `sync_conflict.rs` | 审计/同步 UI |
| U11 | `src/tui/components/`, `state/`, `animation/` | 跨层 UI |

## Key Dependencies

| 用途 | Crate |
|------|-------|
| 数据库 | `rusqlite` |
| KDF | `argon2` v0.5 |
| AEAD | `chacha20poly1305` |
| 密钥派生 | `hkdf` v0.12, `sha2` v0.10 |
| CSPRNG | `rand` v0.9 |
| 内存清零 | `zeroize` v1.8 |
| 常量比较 | `subtle` |
| 助记词 | `bip39` v2.0 |
| 异步运行时 | `tokio` |
| TUI 框架 | `ratatui` |
| 终端后端 | `crossterm` |
| 动画效果 | `tachyonfx` v0.20.1 |
| 序列化 | `serde`, `serde_json` |
| 配置文件 | `toml` |
| Base64 | `base64` |
| 国际化 | `rust-i18n` v3 |
| 剪贴板 | `arboard` |
| 错误处理 | `thiserror` |
| 日志 | `tracing`, `tracing-subscriber` |
| UUID | `uuid` |
| 时间 | `chrono` |
| 路径 | `dirs` |
| 快照测试 | `insta` |
| Mock | `mockall` |
| 临时文件 | `tempfile` (dev) |

## Dependency Rules (模块依赖规则)

严格遵循单向依赖，禁止循环依赖：

```
tui/ → commands/ → executor/ → services/ → {db/, crypto/, types/, config/}
                                       ↘ errors/ (跨层共享)
```

1. **Data Layer 不依赖 Service/UI** — `db/`, `types/`, `crypto/`, `config/` 互不依赖 (crypto 可依赖 types，types 仅依赖 db)
2. **Service Layer 不依赖 UI** — `services/` 仅依赖 Data Layer 和 `commands/`
3. **UI Layer 通过 Command/Message 与 Service 交互** — `tui/` 仅依赖 `commands/`，不直接调用 `services/`
4. **errors/ 跨层共享** — 所有层均可引用 `errors/`

## Test Conventions

- 单元测试放在实现文件旁 (如 `vault.rs` 旁的 `vault_test.rs`，通过 `#[cfg(test)] mod tests` 或独立文件)
- 集成测试放在 `tests/integration/`
- 快照测试放在 `tests/snapshot_tests/`，快照文件在 `tests/snapshots/`
- E2E 测试放在 `tests/e2e/`
- 使用 `insta` 快照测试 (`.insta.toml`)

## WorkTree

Git worktree directory: `.worktrees`
NOT ALLOWED to create a work branch in any other directory. If the .worktrees directory does not exist, stop working and tell me to create it.
