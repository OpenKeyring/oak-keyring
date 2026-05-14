//! XDG Base Directory specification compliant path resolution for oak-keyring.
//!
//! This module follows the [XDG Base Directory specification][xdg-spec] for
//! determining where to store application files:
//!
//! - **Config files**: `$XDG_CONFIG_HOME/oak-keyring/` (default: `~/.config/oak-keyring/`)
//! - **Data files**: `$XDG_DATA_HOME/oak-keyring/` (default: `~/.local/share/oak-keyring/`)
//!
//! All path functions return `Option<PathBuf>` to handle cases where home
//! directory cannot be determined. This design eliminates the need for
//! custom error types and allows callers to handle missing paths gracefully.
//!
//! [xdg-spec]: https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html

use std::path::{Path, PathBuf};

const APP_NAME: &str = "oak-keyring";

/// Returns the XDG config directory for oak-keyring.
///
/// Follows `$XDG_CONFIG_HOME` environment variable with fallback to
/// `~/.config/oak-keyring/`. Returns `None` if home directory cannot be
/// determined.
///
/// # Examples
///
/// ```rust
/// use oak_keyring::paths::config_dir;
///
/// if let Some(config) = config_dir() {
///     println!("Config directory: {:?}", config);
/// }
/// ```
pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home::home_dir().map(|h| h.join(".config")))
        .map(|p| p.join(APP_NAME))
}

/// Returns a fallback config directory when `$HOME` is unavailable.
///
/// Returns the current working directory joined with `oak-keyring`.
/// Used only as a last resort when `config_dir()` returns `None`.
pub fn config_dir_fallback() -> PathBuf {
    PathBuf::from(".").join(APP_NAME)
}

/// Returns the XDG data directory for oak-keyring.
///
/// Follows `$XDG_DATA_HOME` environment variable with fallback to
/// `~/.local/share/oak-keyring/`. Returns `None` if home directory cannot be
/// determined.
///
/// # Examples
///
/// ```rust
/// use oak_keyring::paths::data_dir;
///
/// if let Some(data) = data_dir() {
///     println!("Data directory: {:?}", data);
/// }
/// ```
pub fn data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home::home_dir().map(|h| h.join(".local/share")))
        .map(|p| p.join(APP_NAME))
}

/// Returns a fallback data directory when `$HOME` is unavailable.
///
/// Returns the current working directory joined with `oak-keyring`.
/// Used only as a last resort when `data_dir()` returns `None`.
pub fn data_dir_fallback() -> PathBuf {
    PathBuf::from(".").join(APP_NAME)
}

/// Returns the path to the SQLite database (`vault.db`).
///
/// Returns `None` if data directory cannot be determined.
pub fn db_path() -> Option<PathBuf> {
    data_dir().map(|p| p.join("vault.db"))
}

/// Returns the path to the wrapped secret key file.
///
/// Returns `None` if data directory cannot be determined.
pub fn key_path() -> Option<PathBuf> {
    data_dir().map(|p| p.join("wrapped_secret_key.json"))
}

/// Returns the path to the configuration file (`config.toml`).
///
/// Returns `None` if config directory cannot be determined.
pub fn config_file_path() -> Option<PathBuf> {
    config_dir().map(|p| p.join("config.toml"))
}

/// Returns the OAuth2 token storage directory.
///
/// Returns `None` if config directory cannot be determined.
pub fn tokens_dir() -> Option<PathBuf> {
    config_dir().map(|p| p.join("tokens"))
}

/// Returns the user's Documents directory (`~/Documents`).
///
/// Returns `None` if home directory cannot be determined.
pub fn document_dir() -> Option<PathBuf> {
    home::home_dir().map(|h| h.join("Documents"))
}

/// Creates config and data directories if they don't exist.
///
/// Returns `None` if either directory cannot be determined, or `Some(())` if
/// directories exist or were successfully created.
pub fn ensure_dirs() -> Option<()> {
    let config = config_dir()?;
    let data = data_dir()?;
    std::fs::create_dir_all(config).ok()?;
    std::fs::create_dir_all(data).ok()?;
    Some(())
}

/// Returns `true` if both `wrapped_secret_key.json` and `vault.db` exist in `data_dir`.
pub fn is_vault_complete_at(data_dir: &Path) -> bool {
    data_dir.join("wrapped_secret_key.json").exists() && data_dir.join("vault.db").exists()
}

/// Returns `true` if both `wrapped_secret_key.json` and `vault.db` exist.
///
/// Returns `false` if data directory cannot be determined or if either file
/// is missing.
pub fn vault_complete() -> bool {
    key_path().map(|p| p.exists()).unwrap_or(false)
        && db_path().map(|p| p.exists()).unwrap_or(false)
}

/// Returns `true` if `wrapped_secret_key.json` exists in `data_dir`.
pub fn has_key_file_at(data_dir: &Path) -> bool {
    data_dir.join("wrapped_secret_key.json").exists()
}

/// Returns `true` if `wrapped_secret_key.json` exists.
///
/// Returns `false` if data directory cannot be determined or if the file
/// is missing.
pub fn has_key_file() -> bool {
    key_path().map(|p| p.exists()).unwrap_or(false)
}

/// Returns `true` if `vault.db` exists in `data_dir`.
pub fn has_db_file_at(data_dir: &Path) -> bool {
    data_dir.join("vault.db").exists()
}

/// Returns `true` if `vault.db` exists.
///
/// Returns `false` if data directory cannot be determined or if the file
/// is missing.
pub fn has_db_file() -> bool {
    db_path().map(|p| p.exists()).unwrap_or(false)
}

/// Replaces the home directory prefix with `~` for compact display.
///
/// # Examples
///
/// ```rust
/// use oak_keyring::paths::display_path_with_tilde;
/// use std::path::Path;
///
/// # #[cfg(unix)]
/// # {
/// let home = std::env::var("HOME").unwrap();
/// let path_str = format!("{}/.config/oak-keyring/config.toml", home);
/// let path = Path::new(&path_str);
/// let displayed = display_path_with_tilde(path);
/// assert_eq!(displayed, "~/.config/oak-keyring/config.toml");
/// # }
/// ```
pub fn display_path_with_tilde(path: &Path) -> String {
    if let Some(home) = home::home_dir() {
        let path_str = path.to_string_lossy();
        let home_str = home.to_string_lossy();
        if let Some(rest) = path_str.strip_prefix(&home_str as &str) {
            return format!("~{}", rest);
        }
    }
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_structure() {
        // Ensure clean environment for this test
        std::env::remove_var("XDG_DATA_HOME");

        let dir = data_dir();
        assert!(
            dir.is_some(),
            "data_dir should return Some when HOME is set"
        );

        let dir = dir.unwrap();
        let s = dir.to_string_lossy();
        assert!(
            s.contains(".local/share") || s.contains("local/share"),
            "data path should contain .local/share"
        );
        assert!(
            s.ends_with("oak-keyring"),
            "data path should end with oak-keyring"
        );
    }

    #[test]
    fn config_dir_structure() {
        // Ensure clean environment for this test
        std::env::remove_var("XDG_CONFIG_HOME");

        let dir = config_dir();
        assert!(
            dir.is_some(),
            "config_dir should return Some when HOME is set"
        );

        let dir = dir.unwrap();
        let s = dir.to_string_lossy();
        assert!(
            s.contains(".config") || s.contains("config"),
            "config path should contain .config"
        );
        assert!(
            s.ends_with("oak-keyring"),
            "config path should end with oak-keyring"
        );
    }

    #[test]
    fn derived_paths_are_consistent() {
        assert_eq!(db_path(), data_dir().map(|p| p.join("vault.db")));
        assert_eq!(
            key_path(),
            data_dir().map(|p| p.join("wrapped_secret_key.json"))
        );
        assert_eq!(
            config_file_path(),
            config_dir().map(|p| p.join("config.toml"))
        );
        assert_eq!(tokens_dir(), config_dir().map(|p| p.join("tokens")));
    }

    #[test]
    fn document_dir_is_home_documents() {
        let d = document_dir();
        assert!(
            d.is_some(),
            "document_dir should return Some when HOME is set"
        );

        let d = d.unwrap();
        let s = d.to_string_lossy();
        assert!(
            s.ends_with("Documents"),
            "document path should end with Documents"
        );
    }

    #[test]
    fn display_path_with_tilde_replaces_home() {
        let home = home::home_dir().expect("HOME not set");
        let test_path = home.join(".config").join("oak-keyring").join("test.toml");
        let displayed = display_path_with_tilde(&test_path);

        assert!(
            displayed.starts_with('~'),
            "displayed path should start with ~"
        );
        assert!(
            displayed.contains(".config/oak-keyring/test.toml"),
            "displayed path should contain full relative path"
        );
    }

    #[test]
    fn display_path_with_tilde_handles_non_home_paths() {
        let test_path = Path::new("/tmp/test-file.txt");
        let displayed = display_path_with_tilde(test_path);

        assert_eq!(
            displayed, "/tmp/test-file.txt",
            "non-home paths should be returned as-is"
        );
    }

    #[test]
    fn is_vault_complete_at_returns_false_for_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            !is_vault_complete_at(tmp.path()),
            "vault_complete should be false with no files"
        );
        assert!(
            !has_key_file_at(tmp.path()),
            "has_key_file should be false with no files"
        );
        assert!(
            !has_db_file_at(tmp.path()),
            "has_db_file should be false with no files"
        );
    }

    #[test]
    fn is_vault_complete_at_returns_true_for_both_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("wrapped_secret_key.json"), "{}").unwrap();
        std::fs::write(tmp.path().join("vault.db"), "").unwrap();

        assert!(
            is_vault_complete_at(tmp.path()),
            "vault_complete should be true with both files"
        );
        assert!(has_key_file_at(tmp.path()), "has_key_file should be true");
        assert!(has_db_file_at(tmp.path()), "has_db_file should be true");
    }

    #[test]
    fn is_vault_complete_at_partial_states() {
        let tmp = tempfile::tempdir().unwrap();

        // Only key file.
        std::fs::write(tmp.path().join("wrapped_secret_key.json"), "{}").unwrap();
        assert!(
            !is_vault_complete_at(tmp.path()),
            "vault_complete should be false with only key file"
        );
        assert!(has_key_file_at(tmp.path()), "has_key_file should be true");
        assert!(
            !has_db_file_at(tmp.path()),
            "has_db_file should be false with only key file"
        );

        // Clean up key, create only db.
        std::fs::remove_file(tmp.path().join("wrapped_secret_key.json")).unwrap();
        std::fs::write(tmp.path().join("vault.db"), "").unwrap();
        assert!(
            !is_vault_complete_at(tmp.path()),
            "vault_complete should be false with only db file"
        );
        assert!(
            !has_key_file_at(tmp.path()),
            "has_key_file should be false with only db file"
        );
        assert!(has_db_file_at(tmp.path()), "has_db_file should be true");
    }

    #[test]
    fn config_dir_fallback_returns_cwd_app_dir() {
        let fallback = config_dir_fallback();
        assert!(
            fallback.ends_with("oak-keyring"),
            "fallback should end with oak-keyring"
        );
    }

    #[test]
    fn data_dir_fallback_returns_cwd_app_dir() {
        let fallback = data_dir_fallback();
        assert!(
            fallback.ends_with("oak-keyring"),
            "fallback should end with oak-keyring"
        );
    }
}
