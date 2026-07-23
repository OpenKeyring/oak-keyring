//! `ok agent` command-line entrypoint: unlock the vault and serve the SSH agent.
//!
//! # Testability split
//!
//! [`run`] is the production path: it reads the master password from the tty
//! via `rpassword` (inside [`tokio::task::spawn_blocking`] so the async runtime
//! is never blocked on terminal I/O), then delegates to [`unlock_and_serve`].
//!
//! [`unlock_and_serve`] is the tty-free, testable core: given a vault
//! directory, a [`SecureStr`] password, an [`IdentityFilter`], and a socket
//! path, it performs the EXACT production unlock sequence (mirroring
//! `executor::vault::handle_unlock`) and runs [`AgentServer::serve`]. E2E and
//! integration tests drive this core directly with a known password and a temp
//! socket — no tty, no spawned binary.
//!
//! # Security
//!
//! The password is held only as [`SecureStr`] (zeroizing). It is never logged;
//! error messages never embed it. The `[password]` skip on tracing spans
//! ensures the master password is not captured in trace output.
//!
//! [`AgentServer::serve`]: crate::agent::server::AgentServer::serve

use std::path::{Path, PathBuf};

use clap::Args;
use thiserror::Error;

use crate::agent::identity::IdentityFilter;
use crate::agent::lock::AgentLock;
use crate::agent::paths;
use crate::agent::server::{AgentServer, AgentServerError};
use crate::services::vault::VaultServiceImpl;
use crate::types::SecureStr;

/// `ok agent` subcommand arguments (clap derive).
///
/// All fields are optional so `ok agent` with no flags exposes every SSH key
/// in the vault (the match-all [`IdentityFilter::default`]).
#[derive(Debug, Clone, Args)]
pub struct AgentArgs {
    /// Exact vault record names to expose. Repeatable: `--only a --only b`.
    /// Empty (the default) means "no name restriction".
    #[arg(long, value_name = "NAME")]
    pub only: Vec<String>,

    /// Regex; any record whose name matches is exposed. A name is included when
    /// it is in `--only` OR matches `--allow` (union semantics).
    #[arg(long, value_name = "REGEX")]
    pub allow: Option<String>,

    /// Idle lock: seconds of inactivity after which the agent stops signing.
    ///
    /// When set, the daemon shuts down (gracefully — socket + pidfile removed,
    /// vault session dropped) after this many seconds with no successful
    /// `SIGN_REQUEST`. Each successful sign resets the window. `None` (the
    /// default) disables the idle timer; the agent runs until a shutdown signal
    /// or a fatal accept error.
    #[arg(long, value_name = "SECS")]
    pub idle_lock: Option<u64>,
}

/// Errors surfaced by the `ok agent` CLI.
#[derive(Debug, Error)]
pub enum AgentCliError {
    /// Reading the master password from the tty failed (no tty, I/O error, or
    /// the spawn_blocking task panicked).
    #[error("failed to read master password: {0}")]
    ReadPassword(String),
    /// The vault could not be unlocked (wrong password, missing keystore, or
    /// the SQLCipher database could not be opened). The message never includes
    /// the password.
    #[error("failed to unlock vault: {0}")]
    Unlock(String),
    /// `--allow` was not a valid regex.
    #[error("invalid --allow regex: {0}")]
    InvalidRegex(#[from] regex::Error),
    /// The agent server failed to start or the accept loop failed terminally.
    #[error(transparent)]
    Server(#[from] AgentServerError),
    /// Another `ok agent` instance is already running against this data dir,
    /// or the agent single-instance lock file could not be acquired/created.
    /// Fail loud: do NOT fall through to serving without the lock.
    #[error(transparent)]
    Lock(#[from] crate::agent::lock::AgentLockError),
}

/// Run `ok agent`: acquire the agent single-instance lock, prompt for the
/// master password, then unlock + serve.
///
/// This is the production entrypoint wired from `main.rs`. The agent lock is
/// acquired BEFORE the password prompt (Task 10 Minor-1): a second `ok agent`
/// against the same data dir fails loud with
/// [`AgentLockError::AlreadyRunning`] BEFORE the user is asked to type a
/// password. It performs the tty-bound work (password read) and delegates the
/// testable core to [`unlock_and_serve_with_lock`].
pub async fn run(args: AgentArgs) -> Result<(), AgentCliError> {
    // Locate the vault via the same path resolution the TUI uses, with the same
    // last-resort fallback.
    let vault_dir = crate::paths::data_dir().unwrap_or_else(crate::paths::data_dir_fallback);

    // Acquire the agent single-instance lock BEFORE the password prompt so a
    // competing `ok agent` fails fast without making the user type a password
    // it can never use. Held for the daemon's whole lifetime; dropped on return
    // releases the advisory lock.
    let agent_lock = AgentLock::acquire(&vault_dir)?;

    // Read the master password off the async runtime. rpassword blocks on
    // terminal I/O, so it must run in spawn_blocking; the blocking section is
    // the single place the plaintext password exists outside SecureStr.
    let password = tokio::task::spawn_blocking(|| rpassword::prompt_password("Master password: "))
        .await
        .map_err(|e| AgentCliError::ReadPassword(format!("prompt task failed: {e}")))?
        .map_err(|e| AgentCliError::ReadPassword(e.to_string()))?;
    let password = SecureStr::new(password);

    let socket_path = paths::socket_path();
    let filter = build_filter(&args)?;

    unlock_and_serve_with_lock(
        vault_dir,
        password,
        filter,
        socket_path,
        args.idle_lock,
        agent_lock,
    )
    .await
}

/// Build the [`IdentityFilter`] from parsed args, compiling `--allow` once.
fn build_filter(args: &AgentArgs) -> Result<IdentityFilter, AgentCliError> {
    let allow = match &args.allow {
        Some(pattern) => Some(regex::Regex::new(pattern)?),
        None => None,
    };
    Ok(IdentityFilter {
        only: args.only.clone(),
        allow,
    })
}

/// The tty-free, testable core: acquire the agent single-instance lock, unlock
/// the vault at `vault_dir` with `password`, and serve the SSH agent on
/// `socket_path`.
///
/// Mirrors the production unlock sequence in
/// `executor::vault::handle_unlock`: `KeyStore::unlock` → derive the database
/// page key → open the SQLCipher vault → `VaultServiceImpl::new_unlocked`. With
/// the `sqlcipher` feature off (non-production), it falls back to opening a
/// plain SQLite vault and unlocking the crypto manager in place.
///
/// Before any unlock or socket bind, it acquires the agent single-instance
/// lock ([`AgentLock`]) on `<vault_dir>/.agent.lock`. This is distinct from the
/// TUI's `.instance.lock`, so `ok agent` and `ok` (TUI) coexist against the
/// same vault, while a second `ok agent` fails loud with
/// [`AgentLockError::AlreadyRunning`]. The lock guard is held for the daemon's
/// entire lifetime and released on return. This thin wrapper exists so tests
/// (which never go through the tty) still enforce single-instance exactly like
/// production; the production `run()` path instead acquires the lock BEFORE the
/// password prompt and delegates to [`unlock_and_serve_with_lock`].
///
/// On success it prints `SSH_AUTH_SOCK=<path>` to stdout (flushed) BEFORE
/// entering the accept loop, so a caller/script can read the socket path
/// deterministically. The accept loop then runs until a fatal accept error, a
/// shutdown signal (SIGTERM/SIGINT), or the idle-lock timer elapses — see
/// [`AgentServer::serve`].
///
/// `idle_lock` enforces the idle sign-lock: when `Some(secs)`, the daemon
/// shuts down after `secs` with no successful SIGN_REQUEST; `None` (default)
/// disables the timer.
///
/// [`AgentLockError::AlreadyRunning`]: crate::agent::lock::AgentLockError::AlreadyRunning
/// [`AgentServer::serve`]: crate::agent::server::AgentServer::serve
pub async fn unlock_and_serve(
    vault_dir: PathBuf,
    password: SecureStr,
    filter: IdentityFilter,
    socket_path: PathBuf,
    idle_lock: Option<u64>,
) -> Result<(), AgentCliError> {
    // Acquire the agent single-instance lock BEFORE unlocking the vault or
    // binding the socket. Held for the daemon's lifetime: dropping on return
    // releases the advisory lock. `vault_dir` is the data_dir in production
    // (`run`), so `.agent.lock` lands beside the TUI's `.instance.lock`.
    let agent_lock = AgentLock::acquire(&vault_dir)?;
    unlock_and_serve_with_lock(
        vault_dir,
        password,
        filter,
        socket_path,
        idle_lock,
        agent_lock,
    )
    .await
}

/// Internal core shared by [`run`] (production, lock acquired before the
/// password prompt) and [`unlock_and_serve`] (test path, lock acquired inside).
/// The caller MUST already hold `_agent_lock` for the daemon's lifetime;
/// accepting it as a parameter keeps the guard alive for the body.
pub async fn unlock_and_serve_with_lock(
    vault_dir: PathBuf,
    password: SecureStr,
    filter: IdentityFilter,
    socket_path: PathBuf,
    idle_lock: Option<u64>,
    _agent_lock: AgentLock,
) -> Result<(), AgentCliError> {
    let vault = unlock_vault(&vault_dir, &password)?;
    let server = AgentServer::start(vault, filter, socket_path.clone())?;

    // Announce the socket before serving so callers can consume it
    // deterministically. Flush so a piped reader sees it immediately.
    println!("SSH_AUTH_SOCK={}", socket_path.display());
    let _ = std::io::Write::flush(&mut std::io::stdout());

    server.serve(idle_lock).await?;
    Ok(())
}

/// Unlock the vault at `vault_dir` with `password`, returning an unlocked
/// [`VaultServiceImpl`] ready for [`AgentServer::start`].
///
/// Mirrors `executor::vault::handle_unlock` step-for-step. The password is
/// borrowed only for the keystore unwrap and never persisted or logged.
fn unlock_vault(vault_dir: &Path, password: &SecureStr) -> Result<VaultServiceImpl, AgentCliError> {
    #[cfg(feature = "sqlcipher")]
    {
        use crate::crypto::{keystore::KeyStore, CryptoManager};
        use crate::db::vault_db::VaultDbFactory;

        // Key-first unlock: load the keystore with the master password.
        let keystore = KeyStore::unlock(vault_dir, password).map_err(AgentCliError::Unlock)?;

        // Derive the SQLCipher database page key from the unlocked keystore.
        let db_page_key = keystore.db_page_key().map_err(AgentCliError::Unlock)?;

        // Open the existing encrypted vault database.
        let conn = VaultDbFactory::open_sqlcipher_vault(vault_dir, &db_page_key)
            .map_err(|e| AgentCliError::Unlock(e.to_string()))?;

        let crypto = CryptoManager::from_unlocked_keystore(keystore);
        Ok(VaultServiceImpl::new_unlocked(conn, crypto))
    }

    #[cfg(not(feature = "sqlcipher"))]
    {
        use crate::db::schema::init_db;

        // Non-production plain-SQLite path: open then unlock the crypto manager.
        let conn = init_db(vault_dir).map_err(|e| AgentCliError::Unlock(e.to_string()))?;
        let mut svc = VaultServiceImpl::new(conn);
        svc.unlock(vault_dir, password)
            .map_err(|e| AgentCliError::Unlock(e.to_string()))?;
        Ok(svc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_filter_default_is_match_all() {
        let args = AgentArgs {
            only: vec![],
            allow: None,
            idle_lock: None,
        };
        let f = build_filter(&args).expect("default filter");
        assert!(f.only.is_empty());
        assert!(f.allow.is_none());
        assert!(f.matches("anything"));
    }

    #[test]
    fn build_filter_compiles_allow_regex_and_carries_only() {
        let args = AgentArgs {
            only: vec!["exact".to_string()],
            allow: Some("^key-".to_string()),
            idle_lock: None,
        };
        let f = build_filter(&args).expect("filter");
        assert_eq!(f.only, vec!["exact".to_string()]);
        let re = f.allow.expect("regex present");
        assert!(re.is_match("key-1"));
        assert!(!re.is_match("other"));
    }

    #[test]
    fn build_filter_rejects_invalid_regex() {
        let args = AgentArgs {
            only: vec![],
            allow: Some("(".to_string()), // unbalanced group
            idle_lock: None,
        };
        assert!(matches!(
            build_filter(&args),
            Err(AgentCliError::InvalidRegex(_))
        ));
    }

    #[test]
    fn agent_args_idle_lock_is_optional_and_preserved() {
        let args = AgentArgs {
            only: vec![],
            allow: None,
            idle_lock: Some(120),
        };
        assert_eq!(args.idle_lock, Some(120));
    }
}
