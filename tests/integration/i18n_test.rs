//! Integration tests for i18n — verify locale module functions and file validity.
//!
//! Note: `t!()` macro tests are in unit tests (src/tui/i18n/mod.rs) since
//! the macro requires `$crate::_rust_i18n_t` which is only available within
//! the `oak_keyring` crate. Here we test the public API and locale file structure.

use oak_keyring::tui::i18n;

#[test]
fn init_auto_does_not_panic() {
    i18n::init("auto");
}

#[test]
fn init_and_switch_locale() {
    i18n::init("en");
    assert_eq!(&*rust_i18n::locale(), "en");

    i18n::switch_locale("zh-CN");
    assert_eq!(&*rust_i18n::locale(), "zh-CN");

    i18n::switch_locale("en");
    assert_eq!(&*rust_i18n::locale(), "en");
}

#[test]
fn normalize_locale_handles_common_variants() {
    assert_eq!(i18n::normalize_locale("zh_CN.UTF-8"), "zh-CN");
    assert_eq!(i18n::normalize_locale("en_US"), "en");
    assert_eq!(i18n::normalize_locale("zh-Hans-CN"), "zh-CN");
    assert_eq!(i18n::normalize_locale("ja_JP"), "en"); // fallback
    assert_eq!(i18n::normalize_locale(""), "en"); // empty fallback
}

#[test]
fn detect_system_locale_returns_valid_result() {
    if let Some(raw) = i18n::detect_system_locale() {
        let normalized = i18n::normalize_locale(&raw);
        assert!(
            normalized == "en" || normalized == "zh-CN",
            "Normalized to unexpected locale: {normalized}"
        );
    }
}

#[test]
fn locale_files_contain_required_modules() {
    // Verify en.yml contains all required module prefixes by loading it
    let en_content = include_str!("../../locales/en.yml");
    let required_prefixes = [
        "entry:",
        "main:",
        "password_list:",
        "password_detail:",
        "overlay:",
        "generator:",
        "form:",
        "config:",
        "import_export:",
        "audit:",
        "notification:",
        "empty:",
        "error:",
        "loading:",
        "footer:",
        "help:",
        "status_bar:",
        "sync:",
        "trash:",
        "tag:",
        "batch:",
        "health:",
        "history:",
    ];
    for prefix in &required_prefixes {
        assert!(
            en_content.contains(prefix),
            "en.yml missing module prefix: {prefix}"
        );
    }
}

#[test]
fn zh_cn_locale_file_contains_required_modules() {
    let zh_content = include_str!("../../locales/zh-CN.yml");
    let required_prefixes = [
        "entry:",
        "main:",
        "password_list:",
        "password_detail:",
        "overlay:",
        "generator:",
        "form:",
        "config:",
        "import_export:",
        "audit:",
        "notification:",
        "empty:",
        "error:",
        "loading:",
        "footer:",
        "help:",
        "status_bar:",
        "sync:",
        "trash:",
        "tag:",
        "batch:",
        "health:",
        "history:",
    ];
    for prefix in &required_prefixes {
        assert!(
            zh_content.contains(prefix),
            "zh-CN.yml missing module prefix: {prefix}"
        );
    }
}
