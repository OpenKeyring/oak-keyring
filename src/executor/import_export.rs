use std::path::PathBuf;

use crate::commands::CommandResult;
use crate::commands::types::{CsvColumnMapping, ExportScope, ImportSource};
use crate::errors::{ErrorCode, ErrorContext};
use crate::types::SecureStr;

use super::CommandExecutor;

pub fn handle_validate_import_file(
    _executor: &mut CommandExecutor,
    _source: ImportSource,
    _path: PathBuf,
    _password: Option<SecureStr>,
) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Validate import file not yet implemented."),
    }
}

pub fn handle_execute_import(
    _executor: &mut CommandExecutor,
    _source: ImportSource,
    _path: PathBuf,
    _password: Option<SecureStr>,
    _column_mapping: Option<CsvColumnMapping>,
) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Execute import not yet implemented."),
    }
}

pub fn handle_execute_export(
    _executor: &mut CommandExecutor,
    _scope: ExportScope,
    _output_path: PathBuf,
    _export_password: SecureStr,
    _master_password: SecureStr,
) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Execute export not yet implemented."),
    }
}
