use oak_keyring::app::App;
use oak_keyring::config::AppConfig;

fn main() {
    let vault_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("open-keyring");
    let config = AppConfig::load(&vault_dir).unwrap_or_else(|_| AppConfig::default_config());
    let mut app = App::new(config).expect("failed to create app");
    app.run().expect("app run failed");
}
