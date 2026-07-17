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
    assert_eq!(i18n::render_locale(), "en");

    i18n::switch_locale("zh-CN");
    assert_eq!(i18n::render_locale(), "zh-CN");

    i18n::switch_locale("en");
    assert_eq!(i18n::render_locale(), "en");
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

#[test]
fn locale_files_do_not_define_duplicate_keys_in_same_map() {
    fn assert_no_duplicate_keys(path: &str) {
        let content = std::fs::read_to_string(path).unwrap();
        let mut stack: Vec<(usize, std::collections::HashSet<String>)> = Vec::new();

        for (line_no, line) in content.lines().enumerate() {
            if line.trim().is_empty() || line.trim_start().starts_with('#') {
                continue;
            }

            let indent = line.chars().take_while(|c| *c == ' ').count();
            let trimmed = line.trim_start();
            let Some((key, _rest)) = trimmed.split_once(':') else {
                continue;
            };
            if key.starts_with('-') {
                continue;
            }

            while stack.last().is_some_and(|(level, _)| *level > indent) {
                stack.pop();
            }
            if stack.last().is_none_or(|(level, _)| *level != indent) {
                stack.push((indent, std::collections::HashSet::new()));
            }

            let current = stack.last_mut().unwrap();
            assert!(
                current.1.insert(key.to_string()),
                "duplicate key `{}` in {} at line {}",
                key,
                path,
                line_no + 1
            );
        }
    }

    assert_no_duplicate_keys("locales/en.yml");
    assert_no_duplicate_keys("locales/zh-CN.yml");
}
