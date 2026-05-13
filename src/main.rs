use oak_keyring::app::App;
use oak_keyring::config::AppConfig;
use oak_keyring::crypto::self_test;
use oak_keyring::instance_lock::InstanceLock;
use oak_keyring::security;
use oak_keyring::tui::i18n;

fn main() {
    if should_print_version(std::env::args().skip(1)) {
        println!("ok {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    // Apply process-level protections BEFORE any secrets are loaded
    let process_protections = security::apply_process_protections();
    tracing::info!("Process protections: {process_protections}");

    self_test::run_all().unwrap_or_else(|e| {
        eprintln!("Fatal: crypto self-test failed: {e}");
        eprintln!();
        eprintln!("The core encryption self-test failed before any vault operation was attempted.");
        eprintln!("Do not use this build.");
        eprintln!();
        eprintln!("Please reinstall oak-keyring. If the problem persists, report this build and platform.");
        std::process::exit(1);
    });

    // Ensure all required directories exist
    oak_keyring::paths::ensure_dirs().unwrap_or_else(|e| {
        eprintln!("Fatal: failed to create directories: {e}");
        std::process::exit(1);
    });

    // Load config (auto-generate if vault exists but config doesn't)
    let config = AppConfig::load_or_auto_generate().unwrap_or_else(|e| {
        eprintln!("Warning: failed to load config: {e}");
        AppConfig::default_config()
    });

    // Initialize i18n based on config (auto-detect or explicit locale)
    i18n::init(&config.general.language);

    // Acquire instance lock using data_dir
    let instance_lock = InstanceLock::acquire(&oak_keyring::paths::data_dir()).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });

    // Determine vault state
    let has_vault = oak_keyring::paths::has_key_file() || oak_keyring::paths::has_db_file();

    let mut app = App::new(config, has_vault, instance_lock).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    app.run().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
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
