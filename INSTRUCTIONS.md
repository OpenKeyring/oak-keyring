# oak-keyring

`oak-keyring` is the active Rust CLI/TUI implementation for Open-Keyring.

## Binary Target

- `ok` - main TUI password manager (`src/main.rs`)

## Quick Start

```bash
cargo build
cargo run
cargo test
cargo fmt
cargo clippy -- -D warnings
```

## Architecture Map

The application uses a TEA-style TUI plus a command execution boundary:

```text
tui/ -> commands/ -> executor/ -> services/ -> {db/, crypto/, types/, config/}
                                      |              |
                                      v              v
                                   errors/     sync/ -> cloud/
```

Key rules:

- TUI code emits commands and consumes messages/results; it must not directly call services.
- Executor code owns command dispatch, pre-checks, service calls, and result conversion.
- Services do not depend on TUI.
- `cloud/` depends on cloud/types abstractions, not UI or services.
- `sync/` coordinates sync state, pipeline, retry, conflict, and cloud access.
- `errors/` is shared across layers for structured error propagation.

## Service Architecture Rules

**Trait-based services:** New executor-facing services must define traits. Concrete implementations use `XxxServiceImpl` naming (e.g., `trait Vault` → `struct VaultServiceImpl`).

**Test injection boundaries:** Executor orchestration uses trait objects where tests need injection. The `ExecutorBuilder` is the primary test injection boundary. Service fields use `Box<dyn Trait>` for exclusive ownership or `Arc<dyn Trait>` for shared access according to ownership needs:
- `vault: Box<dyn Vault>` – mutated by executor handlers
- `health: Arc<dyn Health>` – cloned into background work
- `clipboard: Arc<dyn Clipboard>` – shared with config reload adapter
- `import_export: Box<dyn ImportExport>` – stateful sessions, executor-owned
- `sync: Option<Box<dyn Sync>>` – optional runtime, consumed during shutdown

**Async trait compatibility:** Service traits use `BoxFuture<'_, Result<T, E>>` return types for async methods. Native `async fn` in traits is NOT dyn-compatible in Rust 1.93. The BoxFuture pattern enables both trait objects AND mockall automock:
```rust
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
```

**Mockall usage:** Traits use `#[cfg_attr(test, mockall::automock)]` for test-only mock generation. Production code must not depend on mock types.

**Callback parameter structs:** Generic callbacks become boxed callback parameter structs where needed for trait object compatibility.

**Sync shutdown:** Sync shutdown uses a trait-object-compatible method such as `shutdown_box` to enable dynamic dispatch through `dyn Sync`.

**Concrete infrastructure stays concrete:** Config manager, service notifier, OAuth token store, channels, cancellation tokens, paths, and timer flags remain concrete types.

**Recovery backdoor:** `begin_file_backed_vault_db()` may construct `VaultServiceImpl` directly as a documented production backdoor when trait injection is impractical.

**Testing patterns:** Executor tests should use `ExecutorBuilder` for setup, not manually construct the full `CommandExecutor` struct after builder migration. No silent skipping of failed tests or disconnected pipelines.

## Knowledge References

Legacy aliases such as `D0`, `S4`, `U10`, and `Plan K` are module shorthand only. They are not new documentation IDs.

Durable architecture knowledge lives in the docs repo:

- TEA and command flow: `../docs/knowledge/oak-keyring/architecture/tea-command-pattern.md` (`OKD-0006`)
- Database/storage: `../docs/knowledge/oak-keyring/storage/database-schema.md` (`OKD-0007`)
- Cryptography: `../docs/knowledge/oak-keyring/security/crypto-architecture.md` (`OKD-0008`)
- Sync: `../docs/knowledge/oak-keyring/sync/sync-architecture.md` (`OKD-0009`)
- Error handling: `../docs/knowledge/oak-keyring/architecture/error-handling.md` (`OKD-0010`)
- TUI overview: `../docs/knowledge/oak-keyring/tui/tui-overview.md` (`OKD-0011`)

When creating new documentation, follow `../docs/INSTRUCTIONS.md`. Preserve old aliases in `legacy_ids`; do not use them as new `id` values.

## Module Map

| Module | Legacy alias | Knowledge | Notes |
| --- | --- | --- | --- |
| `app/` | - | OKD-0006, OKD-0011 | App loop, update/view split, signal handling |
| `commands/` | S0 | OKD-0006 | Command/message/result types |
| `executor/` | S7 | OKD-0006, OKD-0010 | Command dispatch and service bridge |
| `types/` | D1 | OKD-0007 | Core data models |
| `crypto/` | D2 | OKD-0008 | Argon2id, XChaCha20, HKDF, BIP39, keystore |
| `security/` | - | OKD-0008 | Sensitive memory and process safety |
| `cloud/` | D4 | OKD-0009 | Cloud provider/storage abstraction |
| `sync/` | - | OKD-0009 | Sync state machine, pipeline, retry, conflicts |
| `services/vault/` | S1 | OKD-0007, OKD-0010 | Vault records, search, tags, trash, audit, history |
| `services/sync.rs` | S2 | OKD-0009 | Sync orchestration |
| `services/health.rs` | S3 | OKD-0010 | Password health checks |
| `services/clipboard.rs` | S4 | OKD-0010 | Clipboard integration |
| `services/import_export/` | S5 | OKD-0007, OKD-0010 | Import/export formats |
| `services/rotation.rs` | S6 | OKD-0008, OKD-0010 | Key/password rotation |
| `db/` | D0 | OKD-0007 | SQLite schema, models, migrations |
| `config/` | D3 | OKD-0006 | Configuration management |
| `errors/` | - | OKD-0010 | Error codes, levels, contexts, service error mapping |
| `tui/` | U1-U11 | OKD-0006, OKD-0011 | TUI screens, state, traits, components |
| `instance_lock.rs` | - | - | Single-instance lock |
| `paths.rs` | - | - | Application paths |

## File Organization

- `mod.rs` should contain module declarations and re-exports, not business logic.
- Directory modules use `module/{mod, domain_files, tests}.rs`.
- Single-file modules may split tests into `file_test.rs`; declare them from the parent module with `#[cfg(test)]`.
- Split large files when a file is around 600+ lines or when tests dominate the file.

## Test Conventions

- Unit tests should stay close to the module but avoid turning production files into large mixed files.
- Integration tests live under `tests/integration/`.
- Snapshot tests live under `tests/snapshot_tests/` with snapshots in `tests/snapshots/`.
- Sync tests use focused `tests/sync_*_test.rs` files.
- E2E tests live under `tests/e2e/`.

## Version Management

- For user-visible feature merges, user-visible fixes, build snapshots, or release candidates, use the `oak-keyring-version-management` project skill when available.
- `Cargo.toml` `[package].version` is the only application version source.
- Do not use the application version for `schema_version`, `record.version`, `dek_version`, cloud schema versions, or import/export format compatibility.

## Worktrees

- Worktree directory: `./.worktrees`
- When using subagent-driven development for code work, create isolated worktrees under `./.worktrees`.
- Do not create code worktrees in unrelated locations unless the user explicitly asks.

## Non-Negotiable Working Rules

- Fail loud: do not silently skip failed commands, missing tests, blocked checks, or incomplete steps.
- Wire up end-to-end: new behavior must connect through the user-facing workflow, not stop at isolated helpers or dead APIs.
- Verify before completion: run relevant tests or report exactly why they were not run.
- Surface conflicts: when instructions, docs, code, or tests disagree, call out the conflict and choose the most current or authoritative source.
- Keep code changes scoped; update docs as a separate task unless the user asks for cross-repo work.
