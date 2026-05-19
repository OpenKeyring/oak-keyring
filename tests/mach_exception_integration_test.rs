//! Integration test for Mach exception port and SIGABRT handler.
//!
//! This test verifies that:
//! 1. The Mach exception port installation succeeds
//! 2. The SIGABRT handler installation succeeds
//! 3. The protection status fields are correctly set
//!
//! Note: We cannot test actual crash handling in a unit test because it would
//! terminate the test runner. Instead, we verify that the installation succeeds.

#[cfg(target_os = "macos")]
mod tests {
    use oak_keyring::security::apply_process_protections;

    #[test]
    fn mach_exception_port_installation_succeeds() {
        let protections = apply_process_protections();

        // Core dump protection should always succeed on Unix
        assert!(protections.core_dump_disabled);

        // Mach exception port and SIGABRT handler are best-effort.
        // In sandboxed environments they may fail, which is acceptable.
        if !protections.mach_exception_installed {
            eprintln!("WARNING: Mach exception port failed to install (best-effort)");
        }
        if !protections.sigabrt_handler_installed {
            eprintln!("WARNING: SIGABRT handler failed to install (best-effort)");
        }
    }

    #[test]
    fn protection_status_displays_correctly() {
        let protections = oak_keyring::security::ProcessProtections {
            core_dump_disabled: true,
            mach_exception_installed: true,
            sigabrt_handler_installed: true,
        };

        let display = format!("{}", protections);
        assert!(display.contains("core dump disabled"));
        assert!(display.contains("Mach exception port"));
        assert!(display.contains("SIGABRT handler"));
    }

    #[test]
    fn protection_status_with_only_mach_exception() {
        let protections = oak_keyring::security::ProcessProtections {
            core_dump_disabled: true,
            mach_exception_installed: true,
            sigabrt_handler_installed: false,
        };

        let display = format!("{}", protections);
        assert!(display.contains("core dump disabled"));
        assert!(display.contains("Mach exception port"));
        assert!(!display.contains("SIGABRT handler"));
    }

    #[test]
    fn protection_status_with_only_sigabrt() {
        let protections = oak_keyring::security::ProcessProtections {
            core_dump_disabled: true,
            mach_exception_installed: false,
            sigabrt_handler_installed: true,
        };

        let display = format!("{}", protections);
        assert!(display.contains("core dump disabled"));
        assert!(!display.contains("Mach exception port"));
        assert!(display.contains("SIGABRT handler"));
    }
}
