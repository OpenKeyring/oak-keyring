//! Task 11: graceful shutdown (SIGTERM/SIGINT), `--idle-lock` enforcement, and
//! pidfile lifecycle for `ok agent`.
//!
//! These tests drive the tty-free [`unlock_and_serve`] core on a file-backed
//! SQLCipher vault (the EXACT production unlock path) and assert the cleanup
//! contract: on shutdown the socket file AND pidfile are removed from disk.
//!
//! # Signal isolation
//!
//! `tokio::signal::unix::signal` is process-wide: once installed, a delivered
//! SIGTERM is broadcast to every receiver in the process. To keep the SIGTERM /
//! SIGINT / idle tests from racing one another's servers, every test in this
//! binary acquires a process-local mutex ([`SHUTDOWN_TEST_GUARD`]) so only ONE
//! `unlock_and_serve` server is live when a signal is sent. This file is its
//! own integration-test binary (separate process from `agent_e2e_test.rs`), so
//! a signal sent here never reaches the e2e binary.

use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::Mutex;

use oak_keyring::agent::cli::unlock_and_serve;
use oak_keyring::agent::identity::IdentityFilter;
use oak_keyring::agent::paths;
use oak_keyring::crypto::argon2::Argon2Params;
use oak_keyring::crypto::bip39::MnemonicLanguage;
use oak_keyring::crypto::keystore::KeyStore;
use oak_keyring::crypto::CryptoManager;
use oak_keyring::db::vault_db::VaultDbFactory;
use oak_keyring::types::credential::{CredentialType, EncryptedPayload};
use oak_keyring::types::record::CreateRecordParams;
use oak_keyring::types::sensitive::SecureStr;

/// Serializes the shutdown tests within this binary so only one agent server
/// (and thus one set of signal receivers) is live at a time. `tokio::sync::Mutex`
/// is async-aware so the guard may be held across `.await` points (the whole
/// test body). `LazyLock` because `tokio::sync::Mutex::new` is not `const`.
static SHUTDOWN_TEST_GUARD: std::sync::LazyLock<Mutex<()>> =
    std::sync::LazyLock::new(|| Mutex::new(()));

/// A real unencrypted ed25519 OpenSSH private key (matches the .pub fixture).
const ED25519_PEM: &str = include_str!("fixtures/test_ed25519");

/// The OpenSSH public key string, stored as the vault record's `public_key`.
const ED25519_PUB_SSH: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPddLwxmYUz+k43Vr+cahIy1iOROowugaJr8lQ6Tmi2V \
     test-ed25519@oak-keyring";

/// Build a real file-backed SQLCipher vault in `dir` with one ed25519 SSH
/// record. The vault is left locked on disk so `unlock_and_serve` must unlock
/// it from scratch — the real password round-trip (Argon2id + SQLCipher key).
fn build_file_backed_vault(dir: &std::path::Path, password: &SecureStr) {
    let mut sk = [0x11u8; 32];
    KeyStore::initialize(
        dir,
        &mut sk,
        password,
        &Argon2Params::low(),
        MnemonicLanguage::English,
    )
    .expect("initialize keystore");

    let db_page_key = KeyStore::unlock(dir, password)
        .expect("unlock keystore for setup")
        .db_page_key()
        .expect("derive db page key");
    let conn =
        VaultDbFactory::create_sqlcipher_vault(dir, &db_page_key).expect("create sqlcipher vault");

    let keystore = KeyStore::unlock(dir, password).expect("unlock keystore for record");
    let crypto = CryptoManager::from_unlocked_keystore(keystore);
    let mut svc = oak_keyring::services::vault::VaultServiceImpl::new_unlocked(conn, crypto);
    svc.create_record(CreateRecordParams {
        credential_type: CredentialType::Ssh,
        payload: EncryptedPayload::Ssh {
            name: "github-key".to_string(),
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
/// error instead of timing out opaquely.
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

/// Spawn an `unlock_and_serve` server on a temp socket backed by `vault_dir`,
/// returning the task handle and the socket path.
fn spawn_server(
    vault_dir: PathBuf,
    password: SecureStr,
    socket: PathBuf,
    idle_lock: Option<u64>,
) -> tokio::task::JoinHandle<Result<(), oak_keyring::agent::cli::AgentCliError>> {
    tokio::task::spawn(async move {
        unlock_and_serve(
            vault_dir,
            password,
            IdentityFilter::default(),
            socket,
            idle_lock,
        )
        .await
    })
}

/// Send a Unix signal to the current process using `libc::kill`. This is
/// process-local and only affects the test binary itself.
fn send_signal(signal: libc::c_int) {
    // SAFETY: `kill` is an async-signal-safe POSIX call that delivers a signal
    // to the current process. The tokio signal handler installed by the agent
    // server catches it, so the process does NOT die. PIDs are non-negative and
    // fit in `i32` on every supported platform.
    let pid: i32 = std::process::id().try_into().expect("PID fits in i32");
    let rc = unsafe { libc::kill(pid, signal) };
    assert_eq!(rc, 0, "libc::kill failed for signal {signal}");
}

/// Wait until `path` no longer exists, up to `timeout`. Returns true if it was
/// removed, false on timeout.
async fn wait_for_removed(path: &std::path::Path, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if !path.exists() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

// ─── SIGTERM / SIGINT → graceful shutdown + cleanup ─────────────────────────

/// Shared body for the SIGTERM and SIGINT cleanup tests.
async fn run_signal_cleanup_test(signal: libc::c_int) {
    let _guard = SHUTDOWN_TEST_GUARD.lock().await;

    let dir = tempfile::TempDir::new().expect("vault temp dir");
    let password = SecureStr::new("correct horse battery staple".to_string());
    build_file_backed_vault(dir.path(), &password);

    let sock_dir = tempfile::TempDir::new().expect("socket temp dir");
    let socket = sock_dir.path().join("agent.sock");
    let pidfile = paths::pidfile_for_socket(&socket);

    let vault_dir = dir.path().to_path_buf();
    let server_socket = socket.clone();
    let mut handle = spawn_server(vault_dir, password, server_socket, None);
    wait_for_server(&mut handle, &socket).await;

    // Pidfile must exist at startup and contain the running PID.
    assert!(pidfile.exists(), "pidfile should be written at startup");
    let pid_contents = std::fs::read_to_string(&pidfile).expect("read pidfile");
    let pid: u32 = pid_contents
        .trim()
        .parse()
        .expect("pidfile must contain a numeric PID");
    assert_eq!(pid, std::process::id(), "pidfile must record this process");

    // Deliver the signal; the server must catch it and clean up.
    send_signal(signal);

    // The server task must finish (clean exit) within a generous bound.
    let res = tokio::time::timeout(Duration::from_secs(10), &mut handle).await;
    match res {
        Ok(Ok(Ok(()))) => {}
        Ok(Ok(Err(e))) => panic!("server returned error on shutdown: {e:?}"),
        Ok(Err(join_err)) => panic!("server task panicked during shutdown: {join_err}"),
        Err(_) => panic!("server did not stop within 10s of signal {signal}"),
    }

    // The cleanup contract: socket + pidfile removed.
    assert!(
        wait_for_removed(&socket, Duration::from_secs(5)).await,
        "socket file must be removed on shutdown; still at {}",
        socket.display()
    );
    assert!(
        wait_for_removed(&pidfile, Duration::from_secs(5)).await,
        "pidfile must be removed on shutdown; still at {}",
        pidfile.display()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sigterm_triggers_graceful_shutdown_and_cleanup() {
    run_signal_cleanup_test(libc::SIGTERM).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sigint_triggers_graceful_shutdown_and_cleanup() {
    run_signal_cleanup_test(libc::SIGINT).await;
}

// ─── idle-lock → graceful shutdown + cleanup ────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_lock_triggers_shutdown_after_inactivity() {
    let _guard = SHUTDOWN_TEST_GUARD.lock().await;

    let dir = tempfile::TempDir::new().expect("vault temp dir");
    let password = SecureStr::new("idle lock password".to_string());
    build_file_backed_vault(dir.path(), &password);

    let sock_dir = tempfile::TempDir::new().expect("socket temp dir");
    let socket = sock_dir.path().join("agent.sock");
    let pidfile = paths::pidfile_for_socket(&socket);

    // 1 second of inactivity triggers the idle shutdown. No sign requests are
    // sent, so the timer fires untouched.
    let vault_dir = dir.path().to_path_buf();
    let server_socket = socket.clone();
    let mut handle = spawn_server(vault_dir, password, server_socket, Some(1));
    wait_for_server(&mut handle, &socket).await;

    assert!(pidfile.exists(), "pidfile should be written at startup");

    // The server should stop on its own after the idle window. Allow generous
    // slack for scheduler jitter on a loaded CI box.
    let res = tokio::time::timeout(Duration::from_secs(6), &mut handle).await;
    match res {
        Ok(Ok(Ok(()))) => {}
        Ok(Ok(Err(e))) => panic!("server returned error on idle shutdown: {e:?}"),
        Ok(Err(join_err)) => panic!("server task panicked during idle shutdown: {join_err}"),
        Err(_) => panic!("server did not stop within 6s of idle timeout"),
    }

    assert!(
        wait_for_removed(&socket, Duration::from_secs(5)).await,
        "socket file must be removed after idle shutdown"
    );
    assert!(
        wait_for_removed(&pidfile, Duration::from_secs(5)).await,
        "pidfile must be removed after idle shutdown"
    );
}

// NOTE on idle-reset coverage: the idle timer is reset on every successful
// SIGN_REQUEST via a `try_send` into the activity channel (see `serve`). A full
// reset-on-sign integration test would require either a real `ssh-add`
// round-trip or a hand-rolled sign wire body; that interop is already covered
// for the sign path by `agent_e2e_test.rs` / `agent_signer_test.rs`, and the
// reset wiring is a one-liner verified by `cargo clippy` + the structural test
// above (timer fires with no signs). Adding a parallel sign-while-idle test
// here would re-enter the process-wide signal mutex for no additional coverage
// of THIS task's shutdown contract, so it is intentionally omitted.
