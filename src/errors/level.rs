#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorLevel {
    /// Full-screen ErrorDialog, blocking, user must act.
    /// Triggers: database corruption, file I/O unrecoverable, vault root data corruption.
    Fatal,
    /// Inline error within panel, partial blocking, retry available.
    /// Triggers: single record operation failure, decryption failure, import partial failure.
    Operation,
    /// StatusBar 5s temporary notification, non-blocking.
    /// Triggers: clipboard unavailable, sync timeout, HIBP rate limited, tag already exists.
    Minor,
}
