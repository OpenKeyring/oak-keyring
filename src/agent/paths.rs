//! Socket path resolution for the SSH agent backend (`ok agent`).
//!
//! [`socket_path`] is a PURE resolver: it computes where the agent Unix socket
//! should live without touching the filesystem. The socket's parent directory
//! is created with mode `0700` (and the socket itself with `0600`) by
//! [`AgentServer::serve`] at bind time, so this module deliberately does not
//! duplicate that side effect — keeping it trivially testable and consistent
//! with the top-level [`crate::paths`] module (resolution vs. `ensure_dirs`).
//!
//! # Directory precedence
//!
//! 1. `$XDG_RUNTIME_DIR` — the freedesktop-standard per-user runtime dir
//!    (`$XDG_RUNTIME_DIR` is typically `/run/user/<uid>`, `0700`, and cleared
//!    on logout). Preferred for agent sockets.
//! 2. `$TMPDIR` — the per-process temp dir fallback.
//! 3. `/tmp` — the POSIX baseline, used when neither env var is set.
//!
//! The socket always lives at `<base>/oak-keyring/agent.sock`.
//!
//! [`AgentServer::serve`]: crate::agent::server::AgentServer::serve

use std::path::PathBuf;

/// Per-user subdirectory holding the agent socket.
const APP_DIR: &str = "oak-keyring";
/// Socket filename within [`APP_DIR`].
const SOCKET_NAME: &str = "agent.sock";

/// Resolve the SSH agent socket path from the runtime environment.
///
/// Pure (no FS mutation): selects the base directory per the precedence in the
/// [module docs](self) and joins `oak-keyring/agent.sock`. The parent directory
/// and socket permissions are enforced later by the server at bind time.
pub fn socket_path() -> PathBuf {
    socket_path_from(
        std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
        std::env::var_os("TMPDIR").map(PathBuf::from),
    )
}

/// Pure, environment-independent resolution used by both [`socket_path`] and the
/// tests. `$XDG_RUNTIME_DIR` wins, then `$TMPDIR`, then `/tmp`.
fn socket_path_from(xdg_runtime_dir: Option<PathBuf>, tmpdir: Option<PathBuf>) -> PathBuf {
    let base = xdg_runtime_dir
        .or(tmpdir)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join(APP_DIR).join(SOCKET_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_xdg_runtime_dir_when_set() {
        let p = socket_path_from(
            Some(PathBuf::from("/run/user/1000")),
            Some(PathBuf::from("/var/tmp")),
        );
        assert_eq!(p, PathBuf::from("/run/user/1000/oak-keyring/agent.sock"));
    }

    #[test]
    fn falls_back_to_tmpdir_when_xdg_unset() {
        let p = socket_path_from(None, Some(PathBuf::from("/var/folders/abc/T")));
        assert_eq!(
            p,
            PathBuf::from("/var/folders/abc/T/oak-keyring/agent.sock")
        );
    }

    #[test]
    fn falls_back_to_tmp_when_both_unset() {
        let p = socket_path_from(None, None);
        assert_eq!(p, PathBuf::from("/tmp/oak-keyring/agent.sock"));
    }

    #[test]
    fn empty_xdg_is_treated_as_unset_and_falls_through() {
        // An empty value is still `Some("")`; this documents that it is used
        // as-is (join handles the empty component). Callers relying on fallback
        // must leave the env var unset rather than empty.
        let p = socket_path_from(Some(PathBuf::new()), None);
        assert_eq!(p, PathBuf::from("oak-keyring/agent.sock"));
    }

    #[test]
    fn resolves_under_oak_keyring_subdir_with_agent_sock_name() {
        let p = socket_path_from(Some(PathBuf::from("/run/user/1")), None);
        assert!(p.ends_with("oak-keyring/agent.sock"));
    }
}
