# Sync Engine

An async sync engine built on Tokio, responsible for data synchronization between local and cloud storage.

## File Organization

- **`mod.rs`**: module declarations + re-exports ONLY. Zero business logic.
- **Tests**: `*_test.rs` files alongside the source file or inside module directories as `tests.rs`

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
Resolving ──(ConflictResolutionFailed)──→ Resolving (retry)
Resolving ──(NetworkError)──→ Offline

Synced ──(ReportCompleted)──→ Idle
Error ──(BackoffExpired, attempt ≤ max)──→ Pulling
Error ──(MaxRetriesExceeded)──→ Idle
Offline ──(BackoffExpired)──→ Pulling
* ──(Shutdown)──→ ShuttingDown
```

- `can_accept_commands()` returns true only for Idle/Error/Offline
- NetworkError → infinite retry (Offline); OtherError → bounded retry (Error)
- Synced is transient and immediately transitions back to Idle

## Pipeline (4 Stages)

```
PullMetadata → Detect → Push → Resolve
```

| Stage | Responsibility |
|-------|---------------|
| **PullMetadataStage** | Download remote metadata, verify vault identity token; fast-path: skip when versions match |
| **DetectStage** | Compare local/remote versions, classify into to_upload/to_download/conflicts |
| **PushStage** | Execute uploads/downloads, store conflict data, update checkpoint |
| **ResolveStage** | Report pending conflicts |

**PipelineResult**: `Completed` / `NoChanges` / `ConflictsDetected` / `Error`

## Conflict Resolution

Conflict detection and resolution are handled by `conflict.rs` (ConflictManager), which provides stateless conflict management operations.

**Core types**:
- `ConflictAction` — detection result: Conflict/DownloadOnly/UploadOnly/NoAction
- `ResolutionAction` — resolution strategy: KeepLocal (upload local, version+1) / KeepRemote (overwrite local with remote)
- `ConflictItem` — a single conflict entry (used for batch resolution)
- `ResolvedConflict` / `KeepRemoteData` — resolved data

**Detection rules** (based on local_sync_status + version comparison):
- **Conflict**: Pending AND remote > local → both sides modified
- **DownloadOnly**: Synced AND remote > local → only remote updated
- **UploadOnly**: Pending AND remote == local → only local updated
- **NoAction**: other cases

**ResolutionStrategy**:
- `KeepLocal` — upload local (version+1, overwrite remote)
- `KeepRemote` — overwrite local with remote data (deserialize conflict_data + verify AAD/checksum)

**Batch resolution**: `resolve_all_batch()` applies a uniform strategy; a single item failure does not interrupt the batch

## Checkpoint (Atomic Persistence)

- write-to-temp + rename pattern, preventing data corruption from crash interruptions
- Tracks: `push_completed_ids`, `pull_completed_ids`, `detect_completed`, `pending_conflicts`
- `from_corrupted_or_none()` — returns an empty checkpoint when corrupted, never panics

## Distributed Lock

- TTL lock file `.sync.lock` (5-minute TTL)
- acquire(): create when unlocked / overwrite when expired / self-renew / wait when held by others
- release(): idempotent (only deletes its own lock)
- acquire_timeout: 30s, retry_interval: 5s

## Retry

```rust
RetryPolicy { max_retries: 5, base_delay: 5s, max_delay: 300s, multiplier: 2.0, jitter: ±20% }
```

- **Infinite retry**: NetworkUnreachable
- **Bounded retry**: NetworkTimeout, ConnectionRefused, LockAcquireFailed, ProviderError
- **No retry**: Auth/Checksum/Serialization/State/Quota errors

## SyncTask (Async Main Loop)

Commands: `TriggerSync` / `PullOnly` / `ResolveConflict` / `ResolveAllConflicts` / `Pause` / `Resume` / `Shutdown`

Events: `Completed(SyncReport)` / `Failed` / `ConflictResolved` / `StateChanged` / `ShutdownComplete`

## NonceValidator

Vault identity token verification (24 random bytes → 48-char hex):
- `AllowSync` — both tokens match
- `AdoptRemoteToken` — no local token, adopt the remote one
- `GenerateNewToken` — first sync
- `Err(VaultIdentityMismatch)` — tokens do not match
