# Sync Engine

基于 Tokio 的异步同步引擎，负责本地与云端之间的数据同步。

## State Machine

```
Idle ──(TriggerSync/PullOnly)──→ Pulling
Pulling ──(PullCompleted)──→ Detecting
Pulling ──(NetworkError)──→ Offline
Pulling ──(OtherError)──→ Error

Detecting ──(no changes)──→ Synced
Detecting ──(has changes)──→ Pushing

Pushing ──(no conflicts)──→ Synced
Pushing ──(has conflicts)──→ Resolving
Pushing ──(NetworkError)──→ Offline
Pushing ──(OtherError)──→ Error

Resolving ──(AllConflictsResolved)──→ Synced
Resolving ──(ConflictResolutionFailed)──→ Resolving (重试)
Resolving ──(NetworkError)──→ Offline

Synced ──(ReportCompleted)──→ Idle
Error ──(BackoffExpired, attempt ≤ max)──→ Pulling
Error ──(MaxRetriesExceeded)──→ Idle
Offline ──(BackoffExpired)──→ Pulling
* ──(Shutdown)──→ ShuttingDown
```

- `can_accept_commands()` 仅 Idle/Error/Offline 返回 true
- NetworkError → 无限重试 (Offline)；OtherError → 有限重试 (Error)
- Synced 是瞬态，立即转回 Idle

## Pipeline (4 阶段)

```
PullMetadata → Detect → Push → Resolve
```

| Stage | 职责 |
|-------|------|
| **PullMetadataStage** | 下载远程 metadata，验证 vault identity token；fast-path: 版本相同时跳过 |
| **DetectStage** | 对比本地/远程版本，分类为 to_upload/to_download/conflicts |
| **PushStage** | 执行上传/下载，存储冲突数据，更新 checkpoint |
| **ResolveStage** | 报告待解决冲突 |

**PipelineResult**: `Completed` / `NoChanges` / `ConflictsDetected` / `Error`

## Conflict Resolution

**检测规则** (基于 local_sync_status + version 对比):
- **Conflict**: Pending AND remote > local → 双方都修改了
- **DownloadOnly**: Synced AND remote > local → 仅远程更新
- **UploadOnly**: Pending AND remote == local → 仅本地更新
- **NoAction**: 其他情况

**ResolutionStrategy**:
- `KeepLocal` — 上传本地 (version+1, 覆盖远程)
- `KeepRemote` — 用远程数据覆盖本地 (反序列化 conflict_data + 验证 AAD/checksum)

**批量解决**: `resolve_all_batch()` 统一应用策略，单项失败不中断

## Checkpoint (原子持久化)

- write-to-temp + rename 模式，防崩溃中断导致数据损坏
- 跟踪: `push_completed_ids`, `pull_completed_ids`, `detect_completed`, `pending_conflicts`
- `from_corrupted_or_none()` — 损坏时返回空 checkpoint，永不 panic

## Distributed Lock

- TTL 锁文件 `.sync.lock` (5 分钟 TTL)
- acquire(): 无锁创建 / 过期覆盖 / 自己续期 / 他人等待
- release(): 幂等 (仅删自己的锁)
- acquire_timeout: 30s, retry_interval: 5s

## Retry

```rust
RetryPolicy { max_retries: 5, base_delay: 5s, max_delay: 300s, multiplier: 2.0, jitter: ±20% }
```

- **无限重试**: NetworkUnreachable
- **有限重试**: NetworkTimeout, ConnectionRefused, LockAcquireFailed, ProviderError
- **不重试**: Auth/Checksum/Serialization/State/Quota 错误

## SyncTask (异步主循环)

Commands: `TriggerSync` / `PullOnly` / `ResolveConflict` / `ResolveAllConflicts` / `Pause` / `Resume` / `Shutdown`

Events: `Completed(SyncReport)` / `Failed` / `ConflictResolved` / `StateChanged` / `ShutdownComplete`

## NonceValidator

Vault identity token 验证 (24 字节随机 → 48 字符 hex):
- `AllowSync` — 双方 token 匹配
- `AdoptRemoteToken` — 本地无 token，采用远程
- `GenerateNewToken` — 首次同步
- `Err(VaultIdentityMismatch)` — token 不匹配
