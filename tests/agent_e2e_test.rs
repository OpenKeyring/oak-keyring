//! E2E interop test: a REAL OpenSSH client (`ssh-add`) against `ok agent`.
//!
//! This is the real-client protocol-interop proof for Task 6. It drives the
//! tty-free testable core [`oak_keyring::agent::cli::unlock_and_serve`] on a
//! temp socket — NOT a spawned `ok agent` binary — because the production
//! `run()` path reads the master password from a tty (`rpassword`), which is
//! impractical to feed reliably in a `cargo test` harness. The core performs
//! the EXACT production unlock sequence (`KeyStore::unlock` → db page key →
//! SQLCipher open → `VaultServiceImpl::new_unlocked`), so a real master
//! password round-trips through Argon2id and SQLCipher exactly as `ok agent`
//! would at runtime.
//!
//! # What this proves
//!
//! - The `ok agent` unlock+serve core unlocks a real file-backed vault and
//!   brings up the agent socket.
//! - A real OpenSSH `ssh-add -l` and `ssh-add -L` successfully speak the
//!   ssh-agent wire protocol to our hand-rolled server and observe the vault's
//!   ed25519 key — i.e. our wire format is accepted by the reference client.
//!
//! # Why not spawn the binary
//!
//! Spawning `ok agent` + feeding rpassword over a piped tty is flaky across
//! CI/dev machines. The task brief explicitly permits driving the testable
//! core directly as the minimum bar; we do that and additionally exercise it
//! through the real unlock path (not an in-memory vault), so the password flow
//! is genuinely covered.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use oak_keyring::agent::cli::unlock_and_serve;
use oak_keyring::agent::identity::IdentityFilter;
use oak_keyring::agent::lock::{AgentLock, AgentLockError};
use oak_keyring::crypto::argon2::Argon2Params;
use oak_keyring::crypto::bip39::MnemonicLanguage;
use oak_keyring::crypto::keystore::KeyStore;
use oak_keyring::crypto::CryptoManager;
use oak_keyring::db::vault_db::VaultDbFactory;
use oak_keyring::instance_lock::InstanceLock;
use oak_keyring::types::credential::{CredentialType, EncryptedPayload};
use oak_keyring::types::record::CreateRecordParams;
use oak_keyring::types::sensitive::SecureStr;

/// A real unencrypted ed25519 OpenSSH private key (matches the .pub fixture).
const ED25519_PEM: &str = include_str!("fixtures/test_ed25519");

/// The OpenSSH public key string, stored as the vault record's `public_key`.
const ED25519_PUB_SSH: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPddLwxmYUz+k43Vr+cahIy1iOROowugaJr8lQ6Tmi2V \
     test-ed25519@oak-keyring";

/// Vault record name — becomes the agent identity comment that `ssh-add`
/// surfaces, so we can assert on it.
const RECORD_NAME: &str = "github-key";

/// Return the path to `ssh-add`, or `None` if OpenSSH is unavailable. The
/// caller decides how to skip; we must NOT call `std::process::exit` here
/// because libtest runs all tests in one process and that would abort the
/// entire binary. Locates `ssh-add` on PATH without a `which` crate dependency.
fn find_ssh_add() -> Option<PathBuf> {
    let path_env = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_env) {
        let candidate = dir.join("ssh-add");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Build a real file-backed SQLCipher vault in `dir` with one ed25519 SSH
/// record, then drop the service. The vault is left locked on disk so
/// `unlock_and_serve` must unlock it from scratch — exercising the real
/// password round-trip (Argon2id + SQLCipher key).
fn build_file_backed_vault(dir: &std::path::Path, password: &SecureStr) {
    // Initialize the keystore (writes wrapped_secret_key.json).
    let mut sk = [0x11u8; 32];
    KeyStore::initialize(
        dir,
        &mut sk,
        password,
        &Argon2Params::low(),
        MnemonicLanguage::English,
    )
    .expect("initialize keystore");

    // Derive the db page key and create the SQLCipher vault database.
    let db_page_key = KeyStore::unlock(dir, password)
        .expect("unlock keystore for setup")
        .db_page_key()
        .expect("derive db page key");
    let conn =
        VaultDbFactory::create_sqlcipher_vault(dir, &db_page_key).expect("create sqlcipher vault");

    // Build an unlocked service and create the SSH record.
    let keystore = KeyStore::unlock(dir, password).expect("unlock keystore for record");
    let crypto = CryptoManager::from_unlocked_keystore(keystore);
    let mut svc = oak_keyring::services::vault::VaultServiceImpl::new_unlocked(conn, crypto);
    svc.create_record(CreateRecordParams {
        credential_type: CredentialType::Ssh,
        payload: EncryptedPayload::Ssh {
            name: RECORD_NAME.to_string(),
            public_key: ED25519_PUB_SSH.to_string(),
            private_key: Some(SecureStr::new(ED25519_PEM.to_string())),
            passphrase: None,
            notes: None,
        },
        tags: vec![],
        is_favorite: false,
        expires_at: None,
    })
    .expect("create ssh record");
    // `svc` dropped here: the vault remains on disk, locked.
}

/// Wait until `socket` exists (the server binds it during `serve()`). If the
/// server task finishes first (i.e. it errored before binding), surface its
/// error instead of timing out opaquely. Bounds the wait generously for
/// Argon2id + SQLCipher open on a slow CI box.
async fn wait_for_server(
    handle: &mut tokio::task::JoinHandle<Result<(), oak_keyring::agent::cli::AgentCliError>>,
    socket: &std::path::Path,
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if socket.exists() {
            return;
        }
        if handle.is_finished() {
            // The task ended without binding the socket — it must have errored.
            let res = handle.await;
            panic!(
                "agent server task ended before binding socket at {}: {:?}",
                socket.display(),
                res
            );
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "agent server did not bind socket at {} within 30s",
                socket.display()
            );
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Run `ssh-add` (`-l` or `-L`) against `socket`, returning its combined
/// stdout. SSH_AUTH_SOCK + SSH_AUTHSOCKET_* envs are scoped to the child.
fn run_ssh_add(ssh_add: &std::path::Path, flag: &str, socket: &std::path::Path) -> String {
    let output = Command::new(ssh_add)
        .arg(flag)
        .env("SSH_AUTH_SOCK", socket)
        .env("SSH_AUTHSOCKET_NAME", socket)
        .output()
        .unwrap_or_else(|e| panic!("failed to run ssh-add {flag}: {e}"));
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "ssh-add {flag} failed (exit {:?})\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status.code()
    );
    stdout
}

// Multi-threaded runtime: the test blocks a worker running the real `ssh-add`
// child synchronously (`.output()`), while the spawned agent server must keep
// running on another worker to answer the client. A current-thread runtime
// would deadlock (the sync ssh-add call starves the server task).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_ssh_add_lists_vault_ed25519_key() {
    // Real OpenSSH is a hard requirement for this interop test. If it is
    // absent, skipping is an explicit environment limitation (stated here), not
    // a silent pass.
    let Some(ssh_add) = find_ssh_add() else {
        eprintln!(
            "[agent_e2e_test] SKIPPED: `ssh-add` not found on PATH; cannot verify real-client interop"
        );
        return;
    };

    // File-backed vault in a tempdir (auto-cleaned on drop).
    let dir = tempfile::TempDir::new().expect("temp dir");
    let password = SecureStr::new("correct horse battery staple".to_string());
    build_file_backed_vault(dir.path(), &password);

    // Unique temp socket, also auto-cleaned.
    let sock_dir = tempfile::TempDir::new().expect("socket temp dir");
    let socket = sock_dir.path().join("agent.sock");

    // Drive the testable unlock+serve core on a dedicated runtime task. This is
    // the same code path `ok agent` runs after reading the password. `password`
    // is moved into the task (SecureStr is not Clone, by design — secrets are
    // not casually duplicated); it is no longer needed after setup. The socket
    // path is cloned for the server so the original remains for ssh-add below.
    let vault_dir = dir.path().to_path_buf();
    let server_socket = socket.clone();
    let mut handle = tokio::task::spawn(async move {
        unlock_and_serve(
            vault_dir,
            password,
            IdentityFilter::default(),
            server_socket,
            None,
        )
        .await
    });

    // Wait for the server to come up (Argon2id + SQLCipher open), surfacing any
    // startup error rather than timing out blindly.
    wait_for_server(&mut handle, &socket).await;

    // ── ssh-add -l: lists identity fingerprints + comments ──────────────
    // macOS OpenSSH prints the algorithm parenthesized in UPPERCASE, e.g.
    // `256 SHA256:... github-key (ED25519)`; match case-insensitively so the
    // assertion is portable across OpenSSH builds.
    let list = run_ssh_add(&ssh_add, "-l", &socket);
    assert!(
        list.to_lowercase().contains("ed25519"),
        "ssh-add -l must list the ed25519 key; got:\n{list}"
    );
    assert!(
        list.contains(RECORD_NAME),
        "ssh-add -l must surface the record name as comment; got:\n{list}"
    );

    // ── ssh-add -L: lists full public key lines ─────────────────────────
    let full = run_ssh_add(&ssh_add, "-L", &socket);
    assert!(
        full.to_lowercase().contains("ssh-ed25519"),
        "ssh-add -L must print the ed25519 public key line; got:\n{full}"
    );
    assert!(
        full.contains(RECORD_NAME),
        "ssh-add -L comment must be the record name; got:\n{full}"
    );

    // Stop the server.
    handle.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_ssh_add_sees_no_identities_when_filter_excludes_all() {
    let Some(ssh_add) = find_ssh_add() else {
        eprintln!(
            "[agent_e2e_test] SKIPPED: `ssh-add` not found on PATH; cannot verify real-client interop"
        );
        return;
    };

    let dir = tempfile::TempDir::new().expect("temp dir");
    let password = SecureStr::new("agent filter password".to_string());
    build_file_backed_vault(dir.path(), &password);

    let sock_dir = tempfile::TempDir::new().expect("socket temp dir");
    let socket = sock_dir.path().join("agent.sock");

    // A filter that matches no record name -> the agent advertises nothing.
    let filter = IdentityFilter {
        only: vec!["nonexistent-key-name".to_string()],
        allow: None,
    };

    let mut handle = {
        let vault_dir = dir.path().to_path_buf();
        let server_socket = socket.clone();
        tokio::task::spawn(async move {
            unlock_and_serve(vault_dir, password, filter, server_socket, None).await
        })
    };

    wait_for_server(&mut handle, &socket).await;

    // ssh-add -l on an empty agent prints "The agent has no identities." and
    // exits 1 on macOS OpenSSH — accept either an explicit "no identities"
    // message or a nonzero exit. The point is: the real client sees zero keys.
    let output = Command::new(&ssh_add)
        .arg("-l")
        .env("SSH_AUTH_SOCK", &socket)
        .output()
        .expect("run ssh-add -l");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.status.success() || combined.to_lowercase().contains("no identities"),
        "with an excluding filter the real client must see no identities; got:\n{combined}"
    );
    // And -L must produce no key line.
    let list = Command::new(&ssh_add)
        .arg("-L")
        .env("SSH_AUTH_SOCK", &socket)
        .output()
        .expect("run ssh-add -L");
    let full = format!(
        "{}{}",
        String::from_utf8_lossy(&list.stdout),
        String::from_utf8_lossy(&list.stderr)
    );
    assert!(
        !full.contains("ssh-ed25519"),
        "no key must be advertised under the excluding filter; got:\n{full}"
    );

    handle.abort();
}

/// Runtime coexistence proof for the agent single-instance lock (Task 10).
///
/// While a real agent daemon is up (driven through the production
/// [`unlock_and_serve`] core on a file-backed SQLCipher vault), a concurrent
/// [`AgentLock::acquire`] on the SAME vault_dir MUST fail with
/// [`AgentLockError::AlreadyRunning`] (a second `ok agent` is rejected), while a
/// concurrent [`InstanceLock::acquire`] (the TUI's lock) on the same dir MUST
/// succeed — the two locks are independent advisory locks on distinct inodes
/// (`.agent.lock` vs `.instance.lock`), so `ok agent` and `ok` (TUI) coexist.
///
/// This complements the inline unit test `agent_lock_and_instance_lock_coexist_on_same_data_dir`
/// by exercising the lock through the live daemon path: it proves the lock is
/// actually acquired during `unlock_and_serve` and held for the daemon's
/// lifetime (not just that two bare `acquire` calls are independent).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_lock_blocks_second_agent_but_coexists_with_tui_lock() {
    // Real vault in a tempdir so `.agent.lock` / `.instance.lock` land in a
    // dir that is auto-cleaned and isolated from every other test.
    let dir = tempfile::TempDir::new().expect("vault temp dir");
    let password = SecureStr::new("correct horse battery staple".to_string());
    build_file_backed_vault(dir.path(), &password);

    let sock_dir = tempfile::TempDir::new().expect("socket temp dir");
    let socket = sock_dir.path().join("agent.sock");
    let vault_dir = dir.path().to_path_buf();
    let server_socket = socket.clone();
    let mut handle = tokio::task::spawn(async move {
        unlock_and_serve(
            vault_dir,
            password,
            IdentityFilter::default(),
            server_socket,
            None,
        )
        .await
    });

    // Wait for the daemon to bind the socket — at this point `unlock_and_serve`
    // has already acquired `AgentLock` and is holding it for the daemon's life.
    wait_for_server(&mut handle, &socket).await;

    // (1) A second `ok agent` against the same vault_dir must be rejected.
    let second = AgentLock::acquire(dir.path());
    match second {
        Err(AgentLockError::AlreadyRunning) => {}
        other => panic!(
            "second AgentLock::acquire must fail with AlreadyRunning while the daemon holds it; got {other:?}"
        ),
    }

    // (2) The TUI's instance lock MUST be acquirable concurrently — independent
    // inode, no mutual exclusion. This is the core coexistence guarantee.
    let _tui_lock = InstanceLock::acquire(dir.path())
        .expect("TUI InstanceLock must coexist with running agent");
    assert!(dir.path().join(".agent.lock").exists());
    assert!(dir.path().join(".instance.lock").exists());

    // Release the daemon; its AgentLock drops, freeing the advisory lock.
    handle.abort();
    // Give the aborted task a moment to release the FD before the tempdir drops.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // (3) After the daemon released the lock, AgentLock is acquirable again.
    let reacquired = AgentLock::acquire(dir.path());
    assert!(
        reacquired.is_ok(),
        "AgentLock must be reacquirable after the daemon released it; got {:?}",
        reacquired.err()
    );
}
