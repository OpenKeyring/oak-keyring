#[derive(Debug, thiserror::Error)]
pub enum ErrorCode {
    #[error("CRYPTO: {0}")]
    Crypto(String),
    #[error("DB: {0}")]
    Db(String),
    #[error("SYNC: {0}")]
    Sync(String),
    #[error("UI: {0}")]
    Ui(String),
    #[error("CONFIG: {0}")]
    Config(String),
    #[error("CLIPBOARD: {0}")]
    Clipboard(String),
    #[error("VAULT: {0}")]
    Vault(String),
    #[error("HEALTH: {0}")]
    Health(String),
    #[error("IMPORT_EXPORT: {0}")]
    ImportExport(String),
    #[error("EXECUTOR: {0}")]
    Executor(String),
    #[error("ROTATION: {0}")]
    Rotation(String),
}
