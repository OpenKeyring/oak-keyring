use std::env;
use std::io::Write;
use std::ops::Range;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use oak_keyring::commands::types::{RecordFilter, RecordSort, SortDirection, SortField};
use oak_keyring::crypto::bip39::{MnemonicLanguage, Passkey};
use oak_keyring::services::vault::VaultServiceImpl;
use oak_keyring::types::credential::{CredentialType, EncryptedPayload};
use oak_keyring::types::record::{CreateRecordParams, UpdateRecordParams};
use oak_keyring::types::sensitive::SecureStr;
use rusqlite::Connection;
use tempfile::TempDir;

#[cfg(feature = "sqlcipher-poc")]
const MODE: &str = "sqlcipher-poc";

#[cfg(not(feature = "sqlcipher-poc"))]
const MODE: &str = "sqlite-bundled";

#[cfg(feature = "sqlcipher-poc")]
const SQLCIPHER_KEY: [u8; 32] = [7u8; 32];

const DEFAULT_COUNTS: [usize; 3] = [100, 1000, 10000];
const SEARCH_QUERY: &str = "bench-login-000";
#[cfg(feature = "sqlcipher-poc")]
const TAG_QUERY: &str = "bench";

enum Metric {
    Millis(Duration),
    NotAvailable(&'static str),
}

impl Metric {
    fn csv_value(&self) -> String {
        match self {
            Self::Millis(duration) => format!("{:.3}", duration_ms(*duration)),
            Self::NotAvailable(_) => "N/A".to_string(),
        }
    }
}

struct BenchRow {
    mode: &'static str,
    count: usize,
    open_ms: Metric,
    unlock_ms: Metric,
    create_ms: Metric,
    list_all_ms: Metric,
    search_ms: Metric,
    update_ms: Metric,
    bulk_import_like_ms: Metric,
    sqlite_to_sqlcipher_migration_ms: Metric,
}

#[cfg(feature = "sqlcipher-poc")]
fn open_connection(vault_dir: &Path) -> Result<Connection> {
    oak_keyring::db::sqlcipher::open_encrypted_vault_dir(vault_dir, &SQLCIPHER_KEY)
        .context("open SQLCipher database")
}

#[cfg(not(feature = "sqlcipher-poc"))]
fn open_connection(vault_dir: &Path) -> Result<Connection> {
    oak_keyring::db::schema::init_db(vault_dir).context("open SQLite database")
}

fn sort() -> RecordSort {
    RecordSort {
        field: SortField::Name,
        direction: SortDirection::Asc,
    }
}

fn login_payload(name: &str, password: &str) -> EncryptedPayload {
    EncryptedPayload::Login {
        name: name.to_string(),
        username: format!("{name}@example.test"),
        password: SecureStr::new(password.to_string()),
        url: Some(format!("https://{name}.example.test")),
        notes: Some("sqlcipher benchmark fixture".to_string()),
    }
}

fn create_params(i: usize) -> CreateRecordParams {
    CreateRecordParams {
        credential_type: CredentialType::Login,
        payload: login_payload(&format!("bench-login-{i:06}"), &format!("secret-{i:06}")),
        tags: vec!["bench".to_string(), format!("bucket-{}", i % 10)],
        is_favorite: i % 11 == 0,
        expires_at: None,
    }
}

fn update_params(id: uuid::Uuid) -> UpdateRecordParams {
    UpdateRecordParams {
        id,
        payload: login_payload("bench-login-updated", "secret-updated"),
        tags: vec!["bench".to_string(), "updated".to_string()],
        is_favorite: true,
        expires_at: None,
        expected_version: 1,
    }
}

fn measure<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let started = Instant::now();
    let result = f();
    (result, started.elapsed())
}

fn generate_mnemonic(context: &'static str) -> Result<Passkey> {
    Passkey::generate(24, MnemonicLanguage::English)
        .map_err(|err| anyhow::anyhow!("{context}: {err}"))
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn create_records(
    vault: &mut VaultServiceImpl,
    start: usize,
    count: usize,
) -> Result<Vec<uuid::Uuid>> {
    let mut ids = Vec::with_capacity(count);
    for i in start..start + count {
        let id = vault
            .create_record(create_params(i))
            .with_context(|| format!("create benchmark record {i}"))?;
        ids.push(id);
    }
    Ok(ids)
}

fn validate_all_records(
    records: &[oak_keyring::types::record::TuiRecord],
    expected_count: usize,
) -> Result<()> {
    anyhow::ensure!(
        records.len() == expected_count,
        "expected {expected_count} active records, found {}",
        records.len()
    );
    anyhow::ensure!(
        records.iter().all(|record| !record.deleted),
        "active list returned a deleted record"
    );
    Ok(())
}

fn validate_search_records(
    records: &[oak_keyring::types::record::TuiRecord],
    expected_count: usize,
) -> Result<()> {
    anyhow::ensure!(
        records.len() == expected_count,
        "search query {SEARCH_QUERY:?} returned {} records, expected {expected_count}",
        records.len()
    );
    anyhow::ensure!(
        records
            .iter()
            .all(|record| record.name.contains(SEARCH_QUERY)),
        "search returned a record whose name does not contain {SEARCH_QUERY:?}"
    );
    Ok(())
}

fn expected_search_count_for_ranges(ranges: &[Range<usize>]) -> usize {
    ranges
        .iter()
        .cloned()
        .flatten()
        .filter(|i| generated_name_contains_search_query(*i))
        .count()
}

fn generated_name_contains_search_query(i: usize) -> bool {
    format!("bench-login-{i:06}").contains(SEARCH_QUERY)
}

#[cfg(feature = "sqlcipher-poc")]
fn validate_tag_records(
    records: &[oak_keyring::types::record::TuiRecord],
    expected_count: usize,
) -> Result<()> {
    anyhow::ensure!(
        records.len() == expected_count,
        "tag query {TAG_QUERY:?} returned {} records, expected {expected_count}",
        records.len()
    );
    anyhow::ensure!(
        records
            .iter()
            .all(|record| record.tags.iter().any(|tag| tag == TAG_QUERY)),
        "tag query returned a record without tag {TAG_QUERY:?}"
    );
    Ok(())
}

fn make_unlocked_vault(vault_dir: &Path) -> Result<(VaultServiceImpl, Metric, Metric)> {
    let (conn, open_duration) = measure(|| open_connection(vault_dir));
    let mut vault = VaultServiceImpl::new(conn?);
    let mnemonic = generate_mnemonic("generate benchmark mnemonic")?;
    let (unlock_result, unlock_duration) = measure(|| vault.unlock_with_mnemonic(&mnemonic));
    unlock_result.context("unlock benchmark vault with mnemonic")?;

    Ok((
        vault,
        Metric::Millis(open_duration),
        Metric::Millis(unlock_duration),
    ))
}

fn run_workload(vault: &mut VaultServiceImpl, count: usize) -> Result<BenchRow> {
    let (ids, create_duration) = measure(|| create_records(vault, 0, count));
    let ids = ids?;

    let (list_all_result, list_all_duration) = measure(|| {
        vault
            .list_records(&RecordFilter::All, &sort())
            .context("list all benchmark records")
    });
    let list_all_records = list_all_result?;
    validate_all_records(&list_all_records, count).context("validate list all benchmark result")?;

    let (search_result, search_duration) = measure(|| {
        vault
            .list_records(&RecordFilter::Search(SEARCH_QUERY.to_string()), &sort())
            .context("search benchmark records")
    });
    let search_records = search_result?;
    let expected_search_count = expected_search_count_for_ranges(&[0..count]);
    validate_search_records(&search_records, expected_search_count)
        .context("validate search benchmark result")?;

    let update_id = *ids
        .first()
        .context("benchmark count must be greater than zero for update workload")?;
    let (update_result, update_duration) = measure(|| {
        vault
            .update_record(update_params(update_id))
            .context("update benchmark record")
    });
    update_result?;

    // There is no current batch-create service API at this PoC layer, so this
    // bulk-import-like phase intentionally loops through create_record. Each
    // call still uses the product record creation path, including record+tag
    // transactional behavior inside the query layer.
    let (bulk_import_like_result, bulk_import_like_duration) =
        measure(|| create_records(vault, count, count).map(|_| ()));
    bulk_import_like_result?;
    let all_after_bulk = vault
        .list_records(&RecordFilter::All, &sort())
        .context("list records after bulk-import-like phase")?;
    validate_all_records(&all_after_bulk, count * 2)
        .context("validate bulk-import-like active record count")?;

    Ok(BenchRow {
        mode: MODE,
        count,
        open_ms: Metric::NotAvailable("filled by caller"),
        unlock_ms: Metric::NotAvailable("filled by caller"),
        create_ms: Metric::Millis(create_duration),
        list_all_ms: Metric::Millis(list_all_duration),
        search_ms: Metric::Millis(search_duration),
        update_ms: Metric::Millis(update_duration),
        bulk_import_like_ms: Metric::Millis(bulk_import_like_duration),
        sqlite_to_sqlcipher_migration_ms: sqlite_to_sqlcipher_migration_metric(count)?,
    })
}

#[cfg(feature = "sqlcipher-poc")]
fn sqlite_to_sqlcipher_migration_metric(count: usize) -> Result<Metric> {
    let plaintext_dir = TempDir::new().context("create plaintext migration source dir")?;
    let encrypted_dir = TempDir::new().context("create SQLCipher migration target dir")?;
    let plaintext_db_path = plaintext_dir.path().join("vault.db");
    let encrypted_db_path = encrypted_dir.path().join("vault.db");
    let mnemonic = generate_mnemonic("generate plaintext source mnemonic")?;

    {
        let conn = oak_keyring::db::schema::init_db(plaintext_dir.path())
            .context("open plaintext SQLite source database")?;
        let mut vault = VaultServiceImpl::new(conn);
        vault
            .unlock_with_mnemonic(&mnemonic)
            .context("unlock plaintext source vault")?;
        create_records(&mut vault, 0, count).context("populate plaintext migration source")?;
        vault
            .checkpoint_wal()
            .context("checkpoint plaintext source WAL before SQLCipher export")?;
    }

    let (export_result, export_duration) =
        measure(|| export_plaintext_sqlite_to_sqlcipher(&plaintext_db_path, &encrypted_db_path));
    export_result.context("export plaintext SQLite source to SQLCipher target")?;
    validate_sqlcipher_export(encrypted_dir.path(), &mnemonic, count)
        .context("validate SQLCipher export")?;

    Ok(Metric::Millis(export_duration))
}

#[cfg(not(feature = "sqlcipher-poc"))]
fn sqlite_to_sqlcipher_migration_metric(_count: usize) -> Result<Metric> {
    Ok(Metric::NotAvailable(
        "SQLCipher export requires the sqlcipher-poc feature",
    ))
}

#[cfg(feature = "sqlcipher-poc")]
fn export_plaintext_sqlite_to_sqlcipher(source: &Path, target: &Path) -> Result<()> {
    let source_conn = Connection::open(source).context("open plaintext source for export")?;
    let target_path = target
        .to_str()
        .context("SQLCipher export target path must be valid UTF-8")?;
    let raw_key = format!("x'{}'", hex::encode(SQLCIPHER_KEY));

    source_conn
        .execute(
            "ATTACH DATABASE ?1 AS encrypted KEY ?2",
            rusqlite::params![target_path, raw_key],
        )
        .context("attach SQLCipher export target")?;
    let export_result = source_conn
        .execute_batch("SELECT sqlcipher_export('encrypted');")
        .context("run SQLCipher export");
    let detach_result = source_conn
        .execute_batch("DETACH DATABASE encrypted;")
        .context("detach SQLCipher export target");

    export_result.and(detach_result)
}

#[cfg(feature = "sqlcipher-poc")]
fn validate_sqlcipher_export(
    vault_dir: &Path,
    mnemonic: &Passkey,
    expected_count: usize,
) -> Result<()> {
    let conn = open_connection(vault_dir).context("open exported SQLCipher vault")?;
    let mut vault = VaultServiceImpl::new(conn);
    vault
        .unlock_with_mnemonic(mnemonic)
        .context("unlock exported SQLCipher vault")?;

    let all_records = vault
        .list_records(&RecordFilter::All, &sort())
        .context("list exported SQLCipher records")?;
    validate_all_records(&all_records, expected_count)
        .context("validate exported SQLCipher active records")?;

    let search_records = vault
        .list_records(&RecordFilter::Search(SEARCH_QUERY.to_string()), &sort())
        .context("search exported SQLCipher records")?;
    let expected_search_count = expected_search_count_for_ranges(&[0..expected_count]);
    validate_search_records(&search_records, expected_search_count)
        .context("validate exported SQLCipher search records")?;

    let tagged_records = vault
        .list_records(&RecordFilter::Tag(TAG_QUERY.to_string()), &sort())
        .context("list exported SQLCipher tag records")?;
    validate_tag_records(&tagged_records, expected_count)
        .context("validate exported SQLCipher tag records")?;

    Ok(())
}

fn run_case(count: usize) -> Result<BenchRow> {
    anyhow::ensure!(count > 0, "count must be greater than zero");

    let vault_dir = TempDir::new().context("create benchmark vault dir")?;
    let (mut vault, open_ms, unlock_ms) = make_unlocked_vault(vault_dir.path())?;
    let mut row = run_workload(&mut vault, count)?;
    row.open_ms = open_ms;
    row.unlock_ms = unlock_ms;
    Ok(row)
}

fn print_header() {
    println!(
        "mode,count,open_ms,unlock_ms,create_ms,list_all_ms,search_ms,update_ms,bulk_import_like_ms,sqlite_to_sqlcipher_migration_ms"
    );
}

fn print_row(row: &BenchRow) {
    println!(
        "{},{},{},{},{},{},{},{},{},{}",
        row.mode,
        row.count,
        row.open_ms.csv_value(),
        row.unlock_ms.csv_value(),
        row.create_ms.csv_value(),
        row.list_all_ms.csv_value(),
        row.search_ms.csv_value(),
        row.update_ms.csv_value(),
        row.bulk_import_like_ms.csv_value(),
        row.sqlite_to_sqlcipher_migration_ms.csv_value()
    );
}

fn print_na_notes(row: &BenchRow) {
    for (name, metric) in [
        ("open_ms", &row.open_ms),
        ("unlock_ms", &row.unlock_ms),
        ("create_ms", &row.create_ms),
        ("list_all_ms", &row.list_all_ms),
        ("search_ms", &row.search_ms),
        ("update_ms", &row.update_ms),
        ("bulk_import_like_ms", &row.bulk_import_like_ms),
        (
            "sqlite_to_sqlcipher_migration_ms",
            &row.sqlite_to_sqlcipher_migration_ms,
        ),
    ] {
        if let Metric::NotAvailable(reason) = metric {
            eprintln!("{name}=N/A: {reason}");
        }
    }
}

fn parse_counts() -> Result<Vec<usize>> {
    let args = env::args().skip(1);
    let mut counts = Vec::new();

    for raw in args {
        let count = raw
            .parse::<usize>()
            .with_context(|| format!("parse record count from {raw:?}"))?;
        anyhow::ensure!(count > 0, "count must be greater than zero");
        counts.push(count);
    }

    if counts.is_empty() {
        Ok(DEFAULT_COUNTS.to_vec())
    } else {
        Ok(counts)
    }
}

fn main() -> Result<()> {
    let counts = parse_counts()?;
    print_header();
    for count in counts {
        let row = run_case(count)?;
        print_row(&row);
        std::io::stdout().flush().context("flush benchmark CSV")?;
        print_na_notes(&row);
    }
    Ok(())
}
