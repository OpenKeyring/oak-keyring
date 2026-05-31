//! I18N initialization, language switching, and locale normalization.

const SUPPORTED_LOCALES: &[&str] = &["en", "zh-CN"];

/// Normalize a raw system locale string to a supported locale identifier.
///
/// Rules:
/// 1. Strip encoding/modifiers (everything after first `.` or `@`)
/// 2. Replace `_` with `-`
/// 3. Match against SUPPORTED_LOCALES
/// 4. Fallback to "en" if no match
pub fn normalize_locale(raw: &str) -> String {
    // 1. Strip encoding/modifiers
    let stripped = raw
        .split('.')
        .next()
        .unwrap_or(raw)
        .split('@')
        .next()
        .unwrap_or(raw);

    // 2. Replace underscores with hyphens
    let normalized = stripped.replace('_', "-");

    // 3. Match against supported locales — exact match first
    if SUPPORTED_LOCALES.contains(&normalized.as_str()) {
        return normalized;
    }

    // Try matching just the language prefix (e.g., "en-US" → check "en")
    let prefix = normalized.split('-').next().unwrap_or(&normalized);
    if SUPPORTED_LOCALES.contains(&prefix) {
        return prefix.to_string();
    }

    // Try matching by language prefix against supported locales
    // (e.g., "zh-Hans-CN" → prefix "zh" → find "zh-CN")
    for supported in SUPPORTED_LOCALES {
        if supported.starts_with(&format!("{prefix}-")) {
            return supported.to_string();
        }
    }

    // 4. Fallback
    "en".to_string()
}

/// Detect the system locale using sys-locale crate.
pub fn detect_system_locale() -> Option<String> {
    sys_locale::get_locale()
}

/// Initialize i18n based on configured language preference.
///
/// - "auto" → detect system locale, fallback to "en"
/// - "en" or "zh-CN" → use directly
pub fn init(configured_language: &str) {
    let locale = match configured_language {
        "auto" => detect_system_locale()
            .map(|raw| normalize_locale(&raw))
            .unwrap_or_else(|| "en".to_string()),
        locale => locale.to_string(),
    };
    rust_i18n::set_locale(&locale);
}

/// Switch locale at runtime (called when user changes language in config).
pub fn switch_locale(locale: &str) {
    let normalized = normalize_locale(locale);
    rust_i18n::set_locale(&normalized);
}

#[cfg(test)]
static LOCALE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// RAII guard that serializes locale-dependent tests and restores locale on drop.
/// Uses a process-wide mutex so no two locale-sensitive tests run concurrently.
#[cfg(test)]
pub struct LocaleGuard {
    original: String,
    _lock: std::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl LocaleGuard {
    pub fn new(locale: &str) -> Self {
        let lock = LOCALE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = rust_i18n::locale().to_string();
        init(locale);
        Self {
            original,
            _lock: lock,
        }
    }

    pub fn en() -> Self {
        Self::new("en")
    }

    pub fn zh_cn() -> Self {
        Self::new("zh-CN")
    }
}

#[cfg(test)]
impl Drop for LocaleGuard {
    fn drop(&mut self) {
        rust_i18n::set_locale(&self.original);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- normalize_locale tests ---

    #[test]
    fn normalize_locale_chinese_underscore() {
        assert_eq!(normalize_locale("zh_CN"), "zh-CN");
    }

    #[test]
    fn normalize_locale_chinese_hans() {
        assert_eq!(normalize_locale("zh-Hans-CN"), "zh-CN");
    }

    #[test]
    fn normalize_locale_chinese_utf8() {
        assert_eq!(normalize_locale("zh_CN.UTF-8"), "zh-CN");
    }

    #[test]
    fn normalize_locale_english_us() {
        assert_eq!(normalize_locale("en_US"), "en");
    }

    #[test]
    fn normalize_locale_english_gb_utf8() {
        assert_eq!(normalize_locale("en_GB.UTF-8"), "en");
    }

    #[test]
    fn normalize_locale_unknown_fallback() {
        assert_eq!(normalize_locale("ja_JP.UTF-8"), "en");
    }

    #[test]
    fn normalize_locale_empty_fallback() {
        assert_eq!(normalize_locale(""), "en");
    }

    #[test]
    fn normalize_locale_exact_match_en() {
        assert_eq!(normalize_locale("en"), "en");
    }

    #[test]
    fn normalize_locale_exact_match_zh_cn() {
        assert_eq!(normalize_locale("zh-CN"), "zh-CN");
    }

    #[test]
    fn normalize_locale_zh_hans_with_at() {
        // e.g., zh_Hans_CN@cjknarrow
        assert_eq!(normalize_locale("zh_Hans_CN@cjknarrow"), "zh-CN");
    }

    // --- detect_system_locale tests ---

    #[test]
    fn detect_system_locale_returns_some() {
        let result = detect_system_locale();
        assert!(result.is_some());
    }

    #[test]
    fn detect_system_locale_result_normalizes() {
        if let Some(raw) = detect_system_locale() {
            let normalized = normalize_locale(&raw);
            assert!(SUPPORTED_LOCALES.contains(&normalized.as_str()));
        }
    }

    // --- init and switch_locale tests (use guard to restore locale) ---

    #[test]
    fn init_and_switch_locale() {
        let _guard = LocaleGuard::en();

        // init("auto") should resolve to a supported locale
        init("auto");
        let current = &*rust_i18n::locale();
        assert!(
            current == "en" || current == "zh-CN",
            "Expected en or zh-CN, got {current:?}"
        );

        // init("en") should force English
        init("en");
        assert_eq!(&*rust_i18n::locale(), "en");

        // init("zh-CN") should force Chinese
        init("zh-CN");
        assert_eq!(&*rust_i18n::locale(), "zh-CN");

        // switch_locale should change runtime locale
        switch_locale("en");
        assert_eq!(&*rust_i18n::locale(), "en");

        switch_locale("zh-CN");
        assert_eq!(&*rust_i18n::locale(), "zh-CN");

        switch_locale("en");
        assert_eq!(&*rust_i18n::locale(), "en");
    }
}
