//! Logging initialization with daily-rotating file appender.

use std::path::Path;
use tracing_subscriber::EnvFilter;

/// Initialize tracing subscriber writing to a daily-rotating log file.
///
/// Returns a `WorkerGuard` that must be held for the app's lifetime.
/// Returns `None` if the file appender cannot be created (logging disabled).
pub fn init(data_dir: &Path) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let log_dir = data_dir.join("logs");
    std::fs::create_dir_all(&log_dir).ok()?;

    let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("oak-keyring")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&log_dir)
        .ok()?;

    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    Some(guard)
}
