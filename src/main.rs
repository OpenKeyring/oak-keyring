use clap::Parser;
use oak_keyring::agent::cli::{self, AgentArgs};
use oak_keyring::app::{App, VaultInitState};
use oak_keyring::config::AppConfig;
use oak_keyring::crypto::self_test;
use oak_keyring::instance_lock::InstanceLock;
use oak_keyring::security;
use oak_keyring::tui::i18n;

/// Top-level command parser.
///
/// A flat `#[derive(Parser)]` with an optional subcommand keeps the existing
/// default behavior intact: `ok` with NO subcommand runs the TUI exactly as
/// before (`mode == None` → [`run_tui`]). `ok --version` / `-V` are handled by
/// clap natively (printing `ok <CARGO_PKG_VERSION>` to stdout, empty stderr),
/// matching the prior hand-rolled `should_print_version` behavior. `ok agent`
/// dispatches to the SSH agent backend.
#[derive(Parser)]
#[command(name = "ok", version, about = "oak-keyring password manager")]
struct Cli {
    /// Optional subcommand. When omitted (`None`), the TUI runs unchanged.
    #[command(subcommand)]
    mode: Option<Command>,
}

/// Available subcommands. The TUI is the implicit default (no subcommand), so
/// there is no explicit `Tui` variant — that preserves byte-for-byte the
/// historical `ok` (no args) entrypoint.
#[derive(clap::Subcommand)]
enum Command {
    /// Run the SSH agent backend backed by the vault's SSH keys.
    Agent(AgentArgs),
}

fn main() {
    let cli = Cli::parse();
    match cli.mode {
        None => run_tui(),
        Some(Command::Agent(args)) => run_agent(args),
    }
}

/// Run the `ok agent` SSH agent backend in a dedicated tokio runtime.
fn run_agent(args: AgentArgs) {
    // Apply process-level protections BEFORE any secrets are loaded (parity
    // with `run_tui`): the agent daemon handles the master password and
    // private keys, so it must apply mlock et al. before any unlock/crypto.
    let process_protections = security::apply_process_protections();
    tracing::info!("Process protections: {process_protections}");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|e| {
            eprintln!("Fatal: failed to start tokio runtime: {e}");
            std::process::exit(1);
        });
    if let Err(e) = runtime.block_on(cli::run(args)) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

/// The historical `ok` entrypoint, unchanged: directory setup, logging, process
/// protections, crypto self-test, config + i18n, instance lock, vault-state
/// routing, and the TUI event loop.
fn run_tui() {
    // Ensure all required directories exist
    if oak_keyring::paths::ensure_dirs().is_none() {
        eprintln!("Fatal: failed to create directories - HOME must be set");
        std::process::exit(1);
    }

    // Initialize file logging (data_dir/oak-keyring.YYYY-MM-DD.log, daily rotation)
    let data_dir =
        oak_keyring::paths::data_dir().unwrap_or_else(oak_keyring::paths::data_dir_fallback);
    let _log_guard = oak_keyring::logging::init(&data_dir);

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

    // Get config directory
    let config_dir =
        oak_keyring::paths::config_dir().unwrap_or_else(oak_keyring::paths::config_dir_fallback);

    // Load config (auto-generate if vault exists but config doesn't)
    let config = AppConfig::load_or_auto_generate(&config_dir, &data_dir).unwrap_or_else(|e| {
        use oak_keyring::config::ConfigError;
        match &e {
            ConfigError::Io(_) => {
                eprintln!("Warning: config file not found or unreadable, using defaults: {e}");
                AppConfig::default_config()
            }
            ConfigError::Parse(_) => {
                eprintln!("Fatal: config file has invalid format: {e}");
                eprintln!("Please fix or remove the config file and try again.");
                std::process::exit(1);
            }
            ConfigError::Validation(_) => {
                eprintln!("Fatal: config validation failed: {e}");
                eprintln!("Please correct the config values and try again.");
                std::process::exit(1);
            }
        }
    });

    // Initialize i18n based on config (auto-detect or explicit locale)
    i18n::init(&config.general.language);

    // Acquire instance lock using data_dir
    let instance_lock = InstanceLock::acquire(&data_dir).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });

    // Determine vault state (4-state routing per spec)
    let has_key = oak_keyring::paths::has_key_file_at(&data_dir);
    let has_db = oak_keyring::paths::has_db_file_at(&data_dir);
    let vault_state = VaultInitState {
        has_vault: has_key && has_db,
        vault_has_key_only: has_key && !has_db,
        vault_has_db_only: !has_key && has_db,
    };

    let mut app = App::new(config, vault_state, instance_lock, data_dir, config_dir)
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1);
        });
    app.run().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
}
