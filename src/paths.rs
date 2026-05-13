//! Fixed XDG-compliant path resolution for oak-keyring.
//!
//! All paths are non-configurable. Config lives under `~/.config/oak-keyring/`,
//! data lives under `~/.local/share/oak-keyring/`.
//!
//! Test overrides: `OAK_VAULT_DIR` and `OAK_CONFIG_DIR` env vars
//! (only active under `test-helpers` feature).
//!
//! Windows is out of scope — `$HOME` is used directly on Unix.

use std::path::PathBuf;

const APP_NAME: &str = "oak-keyring";

fn home_dir() -> PathBuf {
    #[cfg(feature = "test-helpers")]
    if let Ok(dir) = std::env::var("OAK_HOME_DIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from(std::env::var("HOME").expect("HOME environment variable not set"))
}

/// Returns the data directory (`~/.local/share/oak-keyring/`).
pub fn data_dir() -> PathBuf {
    #[cfg(feature = "test-helpers")]
    if let Ok(dir) = std::env::var("OAK_VAULT_DIR") {
        return PathBuf::from(dir);
    }
    home_dir().join(".local/share").join(APP_NAME)
}

/// Returns the config directory (`~/.config/oak-keyring/`).
pub fn config_dir() -> PathBuf {
    #[cfg(feature = "test-helpers")]
    if let Ok(dir) = std::env::var("OAK_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    home_dir().join(".config").join(APP_NAME)
}

/// Returns the path to `vault.db`.
pub fn db_path() -> PathBuf {
    data_dir().join("vault.db")
}

/// Returns the path to `wrapped_secret_key.json`.
pub fn key_path() -> PathBuf {
    data_dir().join("wrapped_secret_key.json")
}

/// Returns the path to `config.toml`.
pub fn config_file_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// Returns the OAuth2 token storage directory.
pub fn tokens_dir() -> PathBuf {
    config_dir().join("tokens")
}

/// Returns the user's Documents directory (`~/Documents`).
pub fn document_dir() -> PathBuf {
    home_dir().join("Documents")
}

/// Creates config and data directories if they don't exist.
pub fn ensure_dirs() -> Result<(), std::io::Error> {
    std::fs::create_dir_all(config_dir())?;
    std::fs::create_dir_all(data_dir())?;
    Ok(())
}

/// Returns `true` if both `wrapped_secret_key.json` and `vault.db` exist.
pub fn vault_complete() -> bool {
    key_path().exists() && db_path().exists()
}

/// Returns `true` if `wrapped_secret_key.json` exists.
pub fn has_key_file() -> bool {
    key_path().exists()
}

/// Returns `true` if `vault.db` exists.
pub fn has_db_file() -> bool {
    db_path().exists()
}

/// Replaces the home directory prefix with `~` for compact display.
pub fn display_path_with_tilde(path: &std::path::Path) -> String {
    let home = home_dir();
    let path_str = path.to_string_lossy();
    let home_str = home.to_string_lossy();
    if let Some(rest) = path_str.strip_prefix(&home_str as &str) {
        return format!("~{}", rest);
    }
    path_str.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_structure() {
        let dir = data_dir();
        let s = dir.to_string_lossy();
        assert!(s.contains(".local/share"));
        assert!(s.ends_with("oak-keyring"));
    }

    #[test]
    fn config_dir_structure() {
        let dir = config_dir();
        let s = dir.to_string_lossy();
        assert!(s.contains(".config"));
        assert!(s.ends_with("oak-keyring"));
    }

    #[test]
    fn derived_paths_are_consistent() {
        assert_eq!(db_path(), data_dir().join("vault.db"));
        assert_eq!(key_path(), data_dir().join("wrapped_secret_key.json"));
        assert_eq!(config_file_path(), config_dir().join("config.toml"));
        assert_eq!(tokens_dir(), config_dir().join("tokens"));
    }

    #[test]
    fn document_dir_is_home_documents() {
        let d = document_dir();
        let s = d.to_string_lossy();
        assert!(s.ends_with("Documents"));
    }
}
