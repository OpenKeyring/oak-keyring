use oak_keyring::app::App;
use oak_keyring::config::AppConfig;
use oak_keyring::crypto::keystore::KeyStore;
use oak_keyring::instance_lock::InstanceLock;
use oak_keyring::tui::i18n;

fn main() {
    if should_print_version(std::env::args().skip(1)) {
        println!("ok {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    #[cfg(feature = "test-helpers")]
    let vault_dir = std::env::var("OAK_VAULT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| default_vault_dir());

    #[cfg(not(feature = "test-helpers"))]
    let vault_dir = default_vault_dir();

    let instance_lock = InstanceLock::acquire(&vault_dir).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });

    let config = AppConfig::load(&vault_dir).unwrap_or_else(|_| AppConfig::default_config());

    // Initialize i18n based on config (auto-detect or explicit locale)
    i18n::init(&config.general.language);

    let has_vault = KeyStore::vault_exists(&vault_dir);
    let mut app =
        App::new(config, vault_dir, has_vault, instance_lock).expect("failed to create app");
    app.run().expect("app run failed");
}

fn default_vault_dir() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("open-keyring")
}

fn should_print_version<I>(args: I) -> bool
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    args.into_iter()
        .any(|arg| matches!(arg.as_ref(), "--version" | "-V"))
}

#[cfg(test)]
mod tests {
    use super::should_print_version;

    #[test]
    fn version_flag_is_detected() {
        assert!(should_print_version(["--version"]));
        assert!(should_print_version(["-V"]));
    }

    #[test]
    fn non_version_args_do_not_print_version() {
        assert!(!should_print_version(["--help"]));
        assert!(!should_print_version(["vault"]));
    }
}
