// =============================================================================
// Process Protections
// =============================================================================
//
// This module provides process-level protections to prevent sensitive data
// (encryption keys, passwords) from being dumped via core dumps or debuggers.
//
// Platform-specific implementation:
// - Unix (macOS, Linux): uses setrlimit(RLIMIT_CORE, 0)
// - Linux: additionally uses prctl(PR_SET_DUMPABLE, 0)
// - macOS: additionally uses Mach exception ports + SIGABRT handler
// - Windows: no protections implemented in this spec
//
// All protections are best-effort - failures log warnings but don't crash.

use std::fmt;

/// Process protection status.
///
/// Indicates which protections were successfully applied at startup.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessProtections {
    /// Core dumps disabled via setrlimit(RLIMIT_CORE, 0) on Unix.
    pub core_dump_disabled: bool,

    /// Process dumpable disabled via prctl(PR_SET_DUMPABLE, 0) on Linux.
    #[cfg(target_os = "linux")]
    pub dumpable_disabled: bool,

    /// Mach exception port installed for hardware exceptions (SIGSEGV, SIGBUS, etc.) on macOS.
    #[cfg(target_os = "macos")]
    pub mach_exception_installed: bool,

    /// SIGABRT signal handler installed on macOS.
    #[cfg(target_os = "macos")]
    pub sigabrt_handler_installed: bool,
}

impl fmt::Display for ProcessProtections {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut enabled = Vec::new();

        if self.core_dump_disabled {
            enabled.push("core dump disabled");
        }

        #[cfg(target_os = "linux")]
        if self.dumpable_disabled {
            enabled.push("dumpable disabled");
        }

        #[cfg(target_os = "macos")]
        {
            if self.mach_exception_installed {
                enabled.push("Mach exception port");
            }
            if self.sigabrt_handler_installed {
                enabled.push("SIGABRT handler");
            }
        }

        if enabled.is_empty() {
            write!(f, "No process protections enabled")
        } else {
            write!(f, "Enabled: {}", enabled.join(", "))
        }
    }
}

/// Applies process-level memory dump protections.
///
/// This function should be called early in the application startup, before
/// any sensitive data (keys, passwords) is loaded or derived.
///
/// All protections are best-effort - if a protection fails, a warning is
/// logged but the application continues running. The returned
/// `ProcessProtections` struct indicates which protections succeeded.
///
/// # Platform-Specific Behavior
///
/// - **Unix (macOS, Linux)**: Calls `setrlimit(RLIMIT_CORE, 0)` to disable
///   core dumps.
/// - **Linux**: Additionally calls `prctl(PR_SET_DUMPABLE, 0)` to prevent
///   the process from being dumped by ptrace.
/// - **macOS**: Additionally installs Mach exception ports and a SIGABRT
///   signal handler to prevent CrashReporter from generating crash logs.
/// - **Windows**: No protections are applied in this implementation.
///
/// # Examples
///
/// ```
/// use oak_keyring::security::apply_process_protections;
///
/// let protections = apply_process_protections();
/// if !protections.core_dump_disabled {
///     // Log that core dump protection failed, but continue anyway
/// }
/// ```
pub fn apply_process_protections() -> ProcessProtections {
    let mut protections = ProcessProtections::default();

    // Apply core dump protection on Unix platforms
    #[cfg(unix)]
    {
        protections.core_dump_disabled = disable_core_dumps();
    }

    // Apply additional Linux-specific protection
    #[cfg(target_os = "linux")]
    {
        protections.dumpable_disabled = disable_dumpable();
    }

    // Apply additional macOS-specific protection
    #[cfg(target_os = "macos")]
    {
        let (mach_installed, sigabrt_installed) = install_crash_handlers();
        protections.mach_exception_installed = mach_installed;
        protections.sigabrt_handler_installed = sigabrt_installed;
    }

    protections
}

/// Disables core dumps via setrlimit(RLIMIT_CORE, 0).
#[cfg(unix)]
fn disable_core_dumps() -> bool {
    unsafe {
        // SAFETY: setrlimit is a standard POSIX system call.
        // We're setting RLIMIT_CORE to 0, which is a safe operation.
        // The function has no side effects other than modifying the
        // resource limit for the current process.
        let rlim = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };

        let result = libc::setrlimit(libc::RLIMIT_CORE, &rlim);

        if result != 0 {
            let err = std::io::Error::last_os_error();
            tracing::warn!(
                error = %err,
                "Failed to disable core dumps via setrlimit(RLIMIT_CORE, 0)"
            );
            false
        } else {
            true
        }
    }
}

/// Disables process dumpable flag via prctl(PR_SET_DUMPABLE, 0).
#[cfg(target_os = "linux")]
fn disable_dumpable() -> bool {
    unsafe {
        // SAFETY: prctl is a Linux system call for process operations.
        // PR_SET_DUMPABLE with argument 0 prevents the process from being
        // dumped by ptrace, which is a safe operation with no adverse effects.
        const PR_SET_DUMPABLE: libc::c_int = 4;

        let result = libc::prctl(PR_SET_DUMPABLE, 0, 0, 0, 0);

        if result != 0 {
            let err = std::io::Error::last_os_error();
            tracing::warn!(
                error = %err,
                "Failed to disable dumpable flag via prctl(PR_SET_DUMPABLE, 0)"
            );
            false
        } else {
            true
        }
    }
}

/// Installs Mach exception ports and SIGABRT handler on macOS.
#[cfg(target_os = "macos")]
fn install_crash_handlers() -> (bool, bool) {
    use crate::security::mach_exception::install_crash_handlers;

    match install_crash_handlers() {
        Ok((mach, sigabrt)) => (mach, sigabrt),
        Err(e) => {
            tracing::warn!(
                error = %e,
                "Failed to install Mach exception handlers"
            );
            (false, false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_protections_default_is_no_protections() {
        let protections = ProcessProtections::default();
        assert!(!protections.core_dump_disabled);

        #[cfg(target_os = "linux")]
        assert!(!protections.dumpable_disabled);

        #[cfg(target_os = "macos")]
        {
            assert!(!protections.mach_exception_installed);
            assert!(!protections.sigabrt_handler_installed);
        }
    }

    #[test]
    fn process_protections_display_empty() {
        let protections = ProcessProtections::default();
        let display = format!("{}", protections);
        assert_eq!(display, "No process protections enabled");
    }

    #[test]
    fn process_protections_display_with_core_dump_disabled() {
        let protections = ProcessProtections {
            core_dump_disabled: true,
            ..Default::default()
        };

        let display = format!("{}", protections);
        assert!(display.contains("core dump disabled"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn process_protections_display_with_all_linux() {
        let protections = ProcessProtections {
            core_dump_disabled: true,
            dumpable_disabled: true,
        };

        let display = format!("{}", protections);
        assert!(display.contains("core dump disabled"));
        assert!(display.contains("dumpable disabled"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn process_protections_display_with_all_macos() {
        let protections = ProcessProtections {
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
    fn apply_process_protections_does_not_crash() {
        // This test verifies that the function doesn't panic or crash.
        // It may fail to apply protections (e.g., in restricted environments),
        // but it should always return a ProcessProtections struct.
        let _protections = apply_process_protections();
    }

    #[test]
    fn process_protections_is_clone_and_copy() {
        let protections = ProcessProtections::default();
        let _copy = protections;
        let _clone = protections; // ProcessProtections is Copy, so this works
    }
}
