mod audit_sync_test;
mod clipboard_test;
mod db_lifecycle_test;
mod db_migration_test;
mod error_propagation_test;
mod executor_command_test;
mod executor_dispatch_test;
mod health_test;
mod i18n_test;
mod main_pipeline_test;
mod okb_sample_file_test;
mod okb_serialization_roundtrip_test;
mod rotation_test;
mod smoke_test;
#[cfg(feature = "sqlcipher")]
mod sqlcipher_poc_test;
#[cfg(feature = "sqlcipher")]
mod sqlcipher_production_test;
mod ui_entry_test;
mod vault_db_crypto_test;
mod vault_service_test;
