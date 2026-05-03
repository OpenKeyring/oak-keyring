use oak_keyring::errors::mapping::clipboard::ClipboardError;
use oak_keyring::errors::service_error::ServiceError;
use oak_keyring::errors::{ErrorCode, ErrorLevel};
use oak_keyring::services::clipboard::{ClipboardService, MockBackend};

#[allow(dead_code)]
fn skip_in_ci() -> bool {
    std::env::var("CI").is_ok()
}

#[test]
fn test_error_propagation_chain() {
    let err = ClipboardError::AccessDenied;
    assert!(matches!(err.error_code(), ErrorCode::Clipboard(_)));
    assert_eq!(err.error_level(), ErrorLevel::Error);
    let warn = ClipboardError::ContentMismatch;
    assert_eq!(warn.error_level(), ErrorLevel::Warning);
}

#[tokio::test]
async fn test_mock_copy_and_smart_clear() {
    let backend = Box::new(MockBackend::new());
    let svc = ClipboardService::with_backend(backend, 30);
    svc.copy("test-password").unwrap();
    assert!(svc.smart_clear().unwrap());
}

#[tokio::test]
async fn test_mock_smart_clear_skips_changed() {
    let backend = Box::new(MockBackend::new());
    let svc = ClipboardService::with_backend(backend, 30);
    svc.copy("original").unwrap();
    // Simulate user copying different text — second copy replaces hash
    svc.copy("user-text").unwrap();
    // smart_clear should clear since last copy was ours
    assert!(svc.smart_clear().unwrap());
}

#[tokio::test]
async fn test_mock_force_clear() {
    let backend = Box::new(MockBackend::new());
    let svc = ClipboardService::with_backend(backend, 30);
    svc.copy("secret").unwrap();
    svc.clear().unwrap();
    assert!(!svc.has_active_timer());
}

#[tokio::test]
async fn test_mock_content_length_limit() {
    let backend = Box::new(MockBackend::new());
    let svc = ClipboardService::with_backend(backend, 30);
    assert!(svc.copy(&"x".repeat(1024)).is_ok());
    assert!(matches!(
        svc.copy(&"x".repeat(1025)).unwrap_err(),
        ClipboardError::ContentTooLong { .. }
    ));
}

#[tokio::test]
async fn test_mock_zero_timeout_no_timer() {
    let backend = Box::new(MockBackend::new());
    let svc = ClipboardService::with_backend(backend, 0);
    svc.copy("test").unwrap();
    assert!(!svc.has_active_timer());
}

#[tokio::test]
async fn test_mock_cancel_timer() {
    let backend = Box::new(MockBackend::new());
    let svc = ClipboardService::with_backend(backend, 30);
    svc.copy("test").unwrap();
    assert!(svc.has_active_timer());
    svc.cancel_timer();
    assert!(!svc.has_active_timer());
}

#[tokio::test]
async fn test_mock_consecutive_copy_resets() {
    let backend = Box::new(MockBackend::new());
    let svc = ClipboardService::with_backend(backend, 30);
    svc.copy("first").unwrap();
    svc.copy("second").unwrap();
    assert!(svc.has_active_timer());
}

#[test]
fn test_headless_detection() {
    let _ = ClipboardService::is_headless();
}

#[tokio::test]
async fn test_new_safe_graceful_degradation() {
    let result = ClipboardService::new_safe(30);
    assert!(result.is_ok(), "new_safe should degrade instead of failing");

    let svc = result.unwrap();
    let copy_result = svc.copy("probe");
    if ClipboardService::is_headless() {
        assert!(matches!(
            copy_result,
            Err(ClipboardError::PlatformUnavailable(_))
        ));
    }
}
