use oak_keyring::app::App;
use oak_keyring::config::AppConfig;
use oak_keyring::crypto::keystore::KeyStore;
use oak_keyring::tui::i18n;

fn main() {
    // In debug builds, allow OAK_VAULT_DIR override for testing (ignored in release)
    let vault_dir = if cfg!(debug_assertions) {
        std::env::var("OAK_VAULT_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::data_local_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("open-keyring")
            })
    } else {
        dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("open-keyring")
    };
    let config = AppConfig::load(&vault_dir).unwrap_or_else(|_| AppConfig::default_config());

    // Initialize i18n based on config (auto-detect or explicit locale)
    i18n::init(&config.general.language);

    let has_vault = KeyStore::vault_exists(&vault_dir);
    let mut app = App::new(config, vault_dir, has_vault).expect("failed to create app");
    app.run().expect("app run failed");
}
