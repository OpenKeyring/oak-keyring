use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::commands::types::FieldSelector;
use crate::commands::CommandResult;
use crate::config::AppConfig;
use crate::crypto::bip39::{MnemonicLanguage, Passkey};
use crate::executor::clipboard::handle_copy_to_clipboard;
use crate::executor::CommandExecutor;
use crate::services::clipboard::{ClipboardService, MockBackend};
use crate::services::vault::VaultServiceImpl;
use crate::types::{
    AuditOperation, CreateRecordParams, CredentialType, EncryptedPayload, SecureStr,
};

fn make_unlocked_executor() -> CommandExecutor {
    let conn = crate::db::schema::init_db_in_memory().expect("init db");
    let mut vault = VaultServiceImpl::new(conn);
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).expect("mnemonic");
    vault
        .unlock_with_mnemonic(&mnemonic)
        .expect("unlock with mnemonic");

    let (result_tx, _) = mpsc::channel(64);
    CommandExecutor::builder(":memory:".into(), ":memory:".into())
        .vault(Box::new(vault))
        .config(AppConfig::default())
        .result_tx(result_tx)
        .shutdown_token(CancellationToken::new())
        .clipboard(Arc::new(ClipboardService::with_backend(
            Box::new(MockBackend::new()),
            30,
        )))
        .build()
        .expect("executor should build")
}

fn create_login_record(executor: &mut CommandExecutor) -> uuid::Uuid {
    executor
        .vault_mut()
        .expect("vault")
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: "GitHub".to_string(),
                username: "alice".to_string(),
                password: SecureStr::new("correct horse battery staple".to_string()),
                url: Some("https://github.com".to_string()),
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create record")
}

#[tokio::test]
async fn copy_password_records_copy_password_audit_entry() {
    let mut executor = make_unlocked_executor();
    let id = create_login_record(&mut executor);

    let result = handle_copy_to_clipboard(&mut executor, id, FieldSelector::Password).await;
    assert!(matches!(
        result,
        CommandResult::CopiedToClipboard {
            field: FieldSelector::Password,
            ..
        }
    ));

    let (entries, _) = executor
        .vault_mut()
        .expect("vault")
        .query_audit_log(&crate::commands::types::AuditFilter {
            operation: Some(AuditOperation::RecordCopyPassword),
            time_range: None,
            search: None,
            ..Default::default()
        })
        .expect("query audit");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].record_id, Some(id));
    assert_eq!(entries[0].record_name.as_deref(), Some("GitHub"));
}

#[tokio::test]
async fn copy_username_records_copy_field_audit_entry() {
    let mut executor = make_unlocked_executor();
    let id = create_login_record(&mut executor);

    let result = handle_copy_to_clipboard(&mut executor, id, FieldSelector::Username).await;
    assert!(matches!(
        result,
        CommandResult::CopiedToClipboard {
            field: FieldSelector::Username,
            ..
        }
    ));

    let (entries, _) = executor
        .vault_mut()
        .expect("vault")
        .query_audit_log(&crate::commands::types::AuditFilter {
            operation: Some(AuditOperation::RecordCopyField),
            time_range: None,
            search: None,
            ..Default::default()
        })
        .expect("query audit");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].record_id, Some(id));
    assert_eq!(entries[0].record_name.as_deref(), Some("GitHub"));
}
