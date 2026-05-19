//! Dump validation harness for issue #107.
//!
//! Validates that crash/core dump artifacts do not contain plaintext secrets
//! when process-level memory dump protections are active.
//!
//! Usage:
//!   cargo run --example dump_validation_harness -- [--mode protected|unprotected] [--crash abort|segv]

use oak_keyring::security::{apply_process_protections, LockedKey32, LockedSecretBytes};
use std::hint::black_box;

// Unique markers that won't appear accidentally in any binary.
const MARKER_LOCKED_BYTES: &[u8] = b"OAK_DUMP_VAL_7F3A9B2C_DEADBEEF_CAFEBABE_98765432_LOCKED";
const MARKER_LOCKED_KEY: &[u8; 32] = b"OAK_DUMP_KEY_7F3A9B2C_DEADBEEF42";
const MARKER_HEAP_STRING: &str = "OAK_HEAP_VAL_7F3A9B2C_DEADBEEF_CAFEBABE_12345678_HEAP";

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut mode = "protected";
    let mut crash_type = "abort";

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" => {
                mode = args.get(i + 1).map(|s| s.as_str()).unwrap_or("protected");
                i += 2;
            }
            "--crash" => {
                crash_type = args.get(i + 1).map(|s| s.as_str()).unwrap_or("abort");
                i += 2;
            }
            other => {
                eprintln!("Unknown argument: {other}");
                eprintln!("Usage: dump_validation_harness [--mode protected|unprotected] [--crash abort|segv]");
                std::process::exit(1);
            }
        }
    }

    eprintln!("=== oak-keyring dump validation harness ===");
    eprintln!("Mode: {mode}");
    eprintln!("Crash type: {crash_type}");

    // Apply process protections in protected mode
    if mode == "protected" {
        let protections = apply_process_protections();
        eprintln!("Process protections: {protections}");

        #[cfg(target_os = "macos")]
        {
            if protections.mach_exception_installed {
                eprintln!("✓ Mach exception port installed");
            } else {
                eprintln!("✗ Mach exception port FAILED to install");
            }
            if protections.sigabrt_handler_installed {
                eprintln!("✓ SIGABRT handler installed");
            } else {
                eprintln!("✗ SIGABRT handler FAILED to install");
            }
        }
    } else {
        eprintln!("Process protections: SKIPPED (unprotected mode)");
    }

    // Load marker secrets into different memory regions

    // 1. LockedSecretBytes (page-locked memory via mmap + mlock)
    let mut locked_bytes = LockedSecretBytes::with_len(MARKER_LOCKED_BYTES.len())
        .expect("locked bytes allocation should succeed");
    locked_bytes
        .expose_mut()
        .copy_from_slice(MARKER_LOCKED_BYTES);

    // 2. LockedKey32 (page-locked 32-byte key)
    let mut key_src = [0u8; 32];
    key_src.copy_from_slice(MARKER_LOCKED_KEY);
    let locked_key = LockedKey32::new(&mut key_src).expect("locked key allocation should succeed");

    // 3. Heap String (simulates PayloadPlaintextDto scenario — not in locked pages)
    let heap_string = MARKER_HEAP_STRING.to_string();

    // Print markers for the validation script to search for
    eprintln!();
    eprintln!("--- Markers loaded (search for these in dump artifacts) ---");
    eprintln!(
        "MARKER_LOCKED_BYTES ({} bytes): {}",
        MARKER_LOCKED_BYTES.len(),
        std::str::from_utf8(MARKER_LOCKED_BYTES).unwrap()
    );
    eprintln!(
        "MARKER_LOCKED_KEY ({} bytes): {}",
        MARKER_LOCKED_KEY.len(),
        std::str::from_utf8(MARKER_LOCKED_KEY).unwrap()
    );
    eprintln!("MARKER_HEAP_STRING: {heap_string}");
    eprintln!("--- End markers ---");
    eprintln!();

    // Prevent the optimizer from removing any of these values.
    black_box(&locked_bytes);
    black_box(locked_key.expose());
    black_box(&heap_string);

    eprintln!("Triggering controlled crash ({crash_type})...");
    eprintln!("Secrets are still in memory — Drop has NOT run.");

    match crash_type {
        "abort" => std::process::abort(),
        "segv" => {
            #[cfg(unix)]
            {
                // SAFETY: raise(SIGSEGV) sends a signal to the current process.
                // This simulates a segmentation fault crash without UB.
                unsafe {
                    libc::raise(libc::SIGSEGV);
                }
                // If raise returns (signal was caught), fall through to abort.
                std::process::abort();
            }
            #[cfg(not(unix))]
            {
                std::process::abort();
            }
        }
        _ => {
            eprintln!("Unknown crash type: {crash_type}");
            std::process::exit(1);
        }
    }
}
