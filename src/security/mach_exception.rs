//! Mach exception handler for macOS crash dump prevention.
//!
//! This module provides a two-layer defense against macOS CrashReporter:
//!
//! **Layer 1: Mach Exception Ports** - Intercepts hardware exceptions (SIGSEGV,
//! SIGBUS, SIGILL, SIGFPE) before they reach CrashReporter. A background thread
//! listens for exception messages via `mach_msg`, calls `zeroize_all()`, and exits
//! silently via `_exit(0)`.
//!
//! **Layer 2: SIGABRT Signal Handler** - SIGABRT bypasses Mach exception ports,
//! so we register a traditional `sigaction` handler that zeroizes and exits.
//!
//! Both layers use `_exit(0)` instead of `std::process::exit()` to avoid running
//! destructors and to prevent CrashReporter from generating `.ips` crash reports.
//!
//! # Implementation Notes
//!
//! - The Mach exception handler thread runs once at startup and never stops.
//! - `zeroize_all()` uses `try_lock()` to avoid blocking in signal context.
//! - All error conditions log warnings but don't crash (best-effort protection).
//! - The handler is installed before any secrets are loaded.

use mach2::{
    exception_types::{
        exception_mask_t, EXCEPTION_STATE, EXC_MASK_ARITHMETIC, EXC_MASK_BAD_ACCESS,
        EXC_MASK_BAD_INSTRUCTION, MACH_EXCEPTION_CODES,
    },
    kern_return::KERN_SUCCESS,
    mach_port::{mach_port_allocate, mach_port_deallocate, mach_port_insert_right},
    message::{mach_msg_header_t, MACH_MSG_TYPE_MAKE_SEND, MACH_RCV_LARGE, MACH_RCV_MSG},
    port::{mach_port_t, MACH_PORT_NULL, MACH_PORT_RIGHT_RECEIVE},
    task::task_set_exception_ports,
    thread_status::THREAD_STATE_NONE,
    traps::mach_task_self,
};
use std::ptr;

/// Result type for Mach exception operations.
type MachResult<T> = Result<T, String>;

/// Size of the buffer for receiving exception messages.
///
/// The buffer must be large enough for `__Request__exception_raise_t` plus
/// a trailer. 1024 bytes is sufficient for the exception message structure.
const EXCEPTION_MSG_BUFFER_SIZE: usize = 1024;

/// Installs the Mach exception port and SIGABRT signal handler.
///
/// This function should be called early in application startup, before any
/// sensitive data is loaded. It:
///
/// 1. Allocates a Mach port with receive and send rights
/// 2. Registers the port for hardware exceptions (SIGSEGV, SIGBUS, SIGILL, SIGFPE)
/// 3. Spawns a background thread to listen for exception messages
/// 4. Installs a SIGABRT signal handler (bypasses Mach exception ports)
///
/// # Returns
///
/// - `Ok(true)` if both layers were installed successfully
/// - `Ok(false)` if one or both layers failed to install (best-effort)
/// - `Err(String)` if a critical error occurred
///
/// # Errors
///
/// All errors are logged as warnings. The function returns `Ok(false)` for
/// non-critical failures to allow the application to continue running.
pub fn install_crash_handlers() -> MachResult<(bool, bool)> {
    // Layer 1: Install Mach exception port
    let mach_installed = install_mach_exception_port().unwrap_or(false);

    // Layer 2: Install SIGABRT signal handler
    let sigabrt_installed = install_sigabrt_handler().unwrap_or(false);

    Ok((mach_installed, sigabrt_installed))
}

/// Installs the Mach exception port for hardware exception interception.
///
/// # Returns
///
/// - `Ok(true)` if the exception port was installed successfully
/// - `Ok(false)` if installation failed (best-effort)
/// - `Err(String)` if a critical error occurred during port allocation
fn install_mach_exception_port() -> MachResult<bool> {
    unsafe {
        // Step 1: Allocate a Mach port with receive right
        let mut port: mach_port_t = MACH_PORT_NULL;
        let kr = mach_port_allocate(mach_task_self(), MACH_PORT_RIGHT_RECEIVE, &mut port);

        if kr != KERN_SUCCESS {
            let err = format!("mach_port_allocate failed with kr={}", kr);
            tracing::warn!("{}", err);
            return Err(err);
        }

        // Step 2: Insert send right so we can receive messages
        let kr = mach_port_insert_right(mach_task_self(), port, port, MACH_MSG_TYPE_MAKE_SEND);

        if kr != KERN_SUCCESS {
            let err = format!("mach_port_insert_right failed with kr={}", kr);
            tracing::warn!("{}", err);
            mach_port_deallocate(mach_task_self(), port);
            return Err(err);
        }

        // Step 3: Register the exception port
        //
        // EXC_MASK_BAD_ACCESS: SIGSEGV (invalid memory access)
        // EXC_MASK_BAD_INSTRUCTION: SIGILL (illegal instruction)
        // EXC_MASK_ARITHMETIC: SIGFPE (division by zero, etc.)
        //
        // We do NOT include EXC_MASK_CRASH or EXC_MASK_CORPSE_NOTIFY because
        // those are used by CrashReporter itself.
        let exception_mask: exception_mask_t =
            EXC_MASK_BAD_ACCESS | EXC_MASK_BAD_INSTRUCTION | EXC_MASK_ARITHMETIC;

        let kr = task_set_exception_ports(
            mach_task_self(),
            exception_mask,
            port,
            (EXCEPTION_STATE | MACH_EXCEPTION_CODES) as i32,
            THREAD_STATE_NONE,
        );

        if kr != KERN_SUCCESS {
            let err = format!("task_set_exception_ports failed with kr={}", kr);
            tracing::warn!("{}", err);
            mach_port_deallocate(mach_task_self(), port);
            return Ok(false);
        }

        // Step 4: Spawn background thread to listen for exceptions
        std::thread::Builder::new()
            .name("mach_exception_handler".to_string())
            .spawn(move || {
                exception_handler_thread(port);
            })
            .map_err(|e| format!("failed to spawn exception handler thread: {}", e))?;

        tracing::debug!("Mach exception port installed successfully");
        Ok(true)
    }
}

/// Background thread that listens for Mach exception messages.
///
/// This thread blocks on `mach_msg` waiting for exception messages. When an
/// exception is received, it:
///
/// 1. Calls `zeroize_all()` to wipe all registered secrets
/// 2. Calls `_exit(0)` to exit immediately without triggering CrashReporter
///
/// The thread runs forever until the process exits or an exception occurs.
#[allow(clippy::never_loop)]
fn exception_handler_thread(port: mach_port_t) {
    let mut buffer = [0u8; EXCEPTION_MSG_BUFFER_SIZE];

    loop {
        let header = buffer.as_mut_ptr() as *mut mach_msg_header_t;

        unsafe {
            // Initialize the message header for receiving
            (*header).msgh_size = EXCEPTION_MSG_BUFFER_SIZE as u32;
            (*header).msgh_local_port = port;
        }

        // Block until an exception message arrives
        let result = unsafe {
            mach2::message::mach_msg(
                header,
                MACH_RCV_MSG | MACH_RCV_LARGE,
                0,                                // send_size
                EXCEPTION_MSG_BUFFER_SIZE as u32, // rcv_size
                port,                             // receive_name
                0,                                // timeout (no timeout)
                MACH_PORT_NULL,                   // notify_port
            )
        };

        if result != KERN_SUCCESS {
            break;
        }

        // Exception received - zeroize all secrets and exit silently.
        // No logging here: this runs in crash handler context where
        // async-signal-safe functions only are allowed.
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        crate::security::crash_handler::zeroize_all();
        unsafe {
            libc::_exit(0);
        }

        // In normal operation, mach_msg blocks indefinitely waiting for exceptions.
        // When an exception arrives, we zeroize and exit above, so the loop doesn't
        // continue in practice. However, the loop structure is correct for the pattern.
    }
}

/// Installs a SIGABRT signal handler to catch abort() calls.
///
/// SIGABRT bypasses Mach exception ports, so we need a separate signal handler.
/// The handler calls `zeroize_all()` and exits via `_exit(0)` to prevent
/// CrashReporter from generating a crash report.
///
/// # Returns
///
/// - `Ok(true)` if the handler was installed successfully
/// - `Ok(false)` if installation failed (best-effort)
/// - `Err(String)` if sigaction is unavailable
fn install_sigabrt_handler() -> MachResult<bool> {
    unsafe {
        // SAFETY: sigaction is a POSIX system call. We're installing a handler
        // for SIGABRT that calls zeroize_all() and _exit(0). The handler is
        // async-signal-safe because it only calls zeroize_all() (which uses
        // try_lock()) and _exit().
        extern "C" fn sigabrt_handler(
            _sig: libc::c_int,
            _info: *mut libc::siginfo_t,
            _ctx: *mut libc::c_void,
        ) {
            core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
            crate::security::crash_handler::zeroize_all();
            unsafe {
                libc::_exit(0);
            }
        }

        // Configure the sigaction structure
        //
        // On macOS, libc::sigaction has:
        // - sa_sigaction: usize (function pointer cast to usize)
        // - sa_mask: sigset_t
        // - sa_flags: c_int
        //
        // Note: macOS does NOT have sa_restorer (unlike Linux).
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = sigabrt_handler as *const () as usize;
        sa.sa_flags = libc::SA_SIGINFO;

        // Register the handler
        let result = libc::sigaction(libc::SIGABRT, &sa, ptr::null_mut());

        if result != 0 {
            let err = std::io::Error::last_os_error();
            tracing::warn!(
                error = %err,
                "Failed to install SIGABRT handler via sigaction"
            );
            return Ok(false);
        }

        tracing::debug!("SIGABRT handler installed successfully");
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_crash_handlers_does_not_crash() {
        // This test verifies that the function doesn't panic or crash.
        // It may fail to install handlers (e.g., in restricted environments),
        // but it should always return a Result.
        let _result = install_crash_handlers();
    }

    #[test]
    fn install_crash_handlers_returns_tuple() {
        // Verify that the function returns a tuple of (bool, bool).
        let result = install_crash_handlers();
        assert!(result.is_ok());
        let (mach, sigabrt) = result.unwrap();
        // Both should be bool
        let _: bool = mach;
        let _: bool = sigabrt;
    }
}
