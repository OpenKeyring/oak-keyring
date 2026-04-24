# oak-keyring

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

## Module Summary

| Module | Spec | Description |
|--------|------|-------------|
| `app/` | — | App struct, run(), 事件循环, signal 处理 |
| `commands/` | S0 | Command/Message 类型定义 |
| `executor/` | S7 | Command Executor (record/clipboard/sync/health/import_export/rotation/vault/config/cancellation/timer) |
| `types/` | D1 | 核心数据模型 (sensitive/credential/record/tag/history/audit/rotation/sync) |
| `crypto/` | D2 | 加密层 (argon2/xchacha20/hkdf/bip39/password/strength/payload) |
| `cloud/` | D4 | 云存储适配层 — adapters: iCloud/S3/SFTP/OAuth2 (11 providers) → 详见 `src/cloud/CLAUDE.md` |
| `sync/` | — | 同步引擎 (state_machine/pipeline/conflict/checkpoint/lock/task/retry/nonce_validator/watcher) → 详见 `src/sync/CLAUDE.md` |
| `services/vault/` | S1 | Vault Service (record/search/tag/trash/audit/history/metadata) |
| `services/sync.rs` | S2 | 同步编排 |
| `services/health.rs` | S3 | 密码健康检查 |
| `services/clipboard.rs` | S4 | 系统剪贴板 |
| `services/import_export/` | S5 | 导入/导出 — parsers: Bitwarden/KeePass/1Password/CSV/OKB |
| `services/rotation.rs` | S6 | 密钥/密码轮转 |
| `db/` | D0 | 数据库层 (schema/models/queries) |
| `config/` | D3 | 配置管理 (manager/error/general/sync/security/password/notification/validation/watcher) |
| `errors/` | — | 错误处理 (跨层共享, mapping: vault/sync/crypto/clipboard/data/health/import_export/rotation) |
| `tui/` | U1-U11 | TUI 界面层 → 详见 `src/tui/CLAUDE.md` |

## Key Dependencies

| Category | Crates |
|----------|--------|
| Database | `rusqlite` v0.34 |
| Crypto | `argon2` v0.5, `chacha20poly1305` v0.10, `hkdf` v0.12, `sha2` v0.10, `sha1` v0.10, `bip39` v2.0 |
| Security | `zeroize` v1.8, `subtle` v2.5, `rand` v0.9, `hex` v0.4 |
| Async | `tokio` v1, `tokio-util` v0.7 |
| TUI | `ratatui` v0.30, `crossterm` v0.29, `tachyonfx` v0.20.1 |
| Serialization | `serde` v1, `serde_json` v1, `toml` v0.8, `base64` v0.22 |
| Import Parsers | `csv` v1, `keepass` v0.7, `zip` v2 |
| Cloud | `opendal` v0.55, `ureq` v3, `notify` v7 |
| i18n | `rust-i18n` v3, `sys-locale` v0.3 |
| Other | `arboard` v3, `thiserror` v2, `tracing` v0.1, `uuid` v1, `chrono` v0.4, `dirs` v6 |
| Dev | `insta` v1, `mockall` v0.13, `tempfile` v3, `anyhow` v1 |

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

- 单元测试放在实现文件旁 (`#[cfg(test)] mod tests` 或独立 `*_test.rs`)
- 集合测试: `tests/integration/` (12 个模块)
- 快照测试: `tests/snapshot_tests/`，快照文件在 `tests/snapshots/`，使用 `insta` (`.insta.toml`)
- 同步测试: `tests/sync_*_test.rs` (checkpoint/conflict/e2e/pipeline/retry)
- E2E 测试: `tests/e2e/`

## WorkTree

Git worktree directory: `./.worktrees`
REQUIRED: When using `subagent-driven-development` for development, it is MUST to follow the `using-git-worktree` skill to create a working directory (`./.worktrees`) for the work. Besides this, it is not allowed to create the working directory in any other location.
