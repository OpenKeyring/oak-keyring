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
| `executor/` | S7 | Command Executor — 逐 command 分文件 |
| `types/` | D1 | 核心数据模型 |
| `crypto/` | D2 | 加密层 (argon2/xchacha20/hkdf/bip39/keystore) |
| `security/` | — | 安全工具 — 安全内存处理、进程级安全 |
| `cloud/` | D4 | 云存储适配层 (11 providers) → 详见 `src/cloud/CLAUDE.md` |
| `sync/` | — | 同步引擎 → 详见 `src/sync/CLAUDE.md` |
| `services/vault/` | S1 | Vault Service (record/search/tag/trash/audit/history) |
| `services/sync.rs` | S2 | 同步编排 |
| `services/health.rs` | S3 | 密码健康检查 |
| `services/clipboard.rs` | S4 | 系统剪贴板 |
| `services/import_export/` | S5 | 导入/导出 (Bitwarden/KeePass/1Password/CSV/OKB) |
| `services/rotation.rs` | S6 | 密钥/密码轮转 |
| `db/` | D0 | 数据库层 (SQLite, schema + migrations) |
| `config/` | D3 | 配置管理 |
| `errors/` | — | 错误处理 (跨层共享, mapping + error codes) |
| `tui/` | U1-U11 | TUI 界面层 → 详见 `src/tui/CLAUDE.md` |
| `instance_lock.rs` | — | 单实例锁 |
| `paths.rs` | — | 应用路径管理 |

## Dependency Rules (模块依赖规则)

严格遵循单向依赖，禁止循环依赖：

```
tui/ → commands/ → executor/ → services/ → {db/, crypto/, types/, config/}
                                        ↘ errors/ (跨层共享)

services/sync.rs → sync/ → cloud/ → types/
services/import_export/ → parsers/ (bitwarden, keepass, csv, onepassword, okb)
```

1. **Data Layer 不依赖 Service/UI** — `db/`, `types/`, `crypto/`, `config/`, `cloud/`, `security/` 互不依赖 (crypto 可依赖 types，types 仅依赖 db)
2. **cloud/ 仅依赖 types/** — 云存储适配层不依赖 services/ 或 sync/
3. **sync/ 依赖 cloud/ 和 types/** — 同步引擎不依赖 services/
4. **Service Layer 不依赖 UI** — `services/` 仅依赖 Data Layer、cloud/、sync/ 和 `commands/`
5. **UI Layer 通过 Command/Message 与 Service 交互** — `tui/` 仅依赖 `commands/`，不直接调用 `services/`
6. **errors/ 跨层共享** — 所有层均可引用 `errors/`

## File Organization (文件组织规则)

- **`mod.rs`**: 仅包含模块声明和重导出 (`pub use`)，零业务逻辑。
- **多文件模块**: `module/{mod, [domain files], tests}.rs`
- **单文件模块 + 测试分离**: `file.rs`（业务逻辑）+ `file_test.rs`（测试），需在父 `mod.rs` 中声明 `#[cfg(test)] mod file_test;`
- **单文件模块 (无需分离)**: `file.rs`（业务逻辑 + 内联 `#[cfg(test)] mod tests { ... }`）

**分离触发条件**: 当文件超过 ~600 行，或测试代码占比超过 30% 时，应将测试提取到独立文件。

## Test Conventions

- **单元测试与业务代码分离为独立文件**，不允许大文件混合。
  - 目录模块：`module/{mod, domain_files, tests}.rs`
  - 单文件模块：`file.rs` + `file_test.rs`，触发时执行分离
- 集合测试: `tests/integration/` (12 个模块)
- 快照测试: `tests/snapshot_tests/`，快照文件在 `tests/snapshots/`，使用 `insta` (`.insta.toml`)
- 同步测试: `tests/sync_*_test.rs` (checkpoint/conflict/e2e/pipeline/retry)
- E2E 测试: `tests/e2e/`

## Version Management

- When preparing a user-visible feature merge, user-visible fix, build snapshot, or
  release candidate, use the project skill at
  `../.claude/skills/oak-keyring-version-management/SKILL.md`.
- Keep `Cargo.toml` `[package].version` as the only application version source.
- Do not use the application version for `schema_version`, `record.version`,
  `dek_version`, cloud schema, or import/export format compatibility.

## WorkTree

Git worktree directory: `./.worktrees`
REQUIRED: When using `subagent-driven-development` for development, it is MUST to follow the `using-git-worktree` skill to create a working directory (`./.worktrees`) for the work. Besides this, it is not allowed to create the working directory in any other location.
