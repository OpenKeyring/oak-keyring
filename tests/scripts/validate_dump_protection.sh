#!/bin/bash
# =============================================================================
# validate_dump_protection.sh
# =============================================================================
# Issue #107: Validate crash/core dump plaintext exposure for memory dump
# protection on macOS.
#
# This script:
# 1. Builds the dump validation harness
# 2. Runs it with process protections enabled (controlled crash)
# 3. Checks for crash artifacts (core dumps, crash reports)
# 4. Searches any artifacts for synthetic marker secrets
# 5. Reports: "dump prevented" | "dump exists, markers absent" | "BLOCKING BUG"
#
# Prerequisites:
#   - macOS (this script is macOS-specific)
#   - Rust toolchain installed
#
# Usage:
#   bash tests/scripts/validate_dump_protection.sh
#   bash tests/scripts/validate_dump_protection.sh --crash segv
# =============================================================================

set -uo pipefail

# --- Configuration ---
HARNESS_NAME="dump_validation_harness"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Markers (must match examples/dump_validation_harness.rs)
MARKER_LOCKED_BYTES="OAK_DUMP_VAL_7F3A9B2C_DEADBEEF_CAFEBABE_98765432_LOCKED"
MARKER_LOCKED_KEY="OAK_DUMP_KEY_7F3A9B2C_DEADBEEF42"
MARKER_HEAP="OAK_HEAP_VAL_7F3A9B2C_DEADBEEF_CAFEBABE_12345678_HEAP"

CRASH_TYPE="abort"
if [[ "${1:-}" == "--crash" ]]; then
    CRASH_TYPE="${2:-abort}"
fi

# --- macOS artifact locations ---
CORES_DIR="/cores"
REPORTS_DIR="$HOME/Library/Logs/DiagnosticReports"
RESULT_FILE="$HOME/oak_dump_validation_result_$(date '+%Y%m%d_%H%M%S').txt"

# --- Helper functions ---
log() {
    echo "[$(date '+%H:%M:%S')] $*"
}

list_harness_artifacts() {
    local dir="$1"
    if [[ -d "$dir" ]]; then
        find "$dir" -maxdepth 1 \( -name "*${HARNESS_NAME}*" -o -name "*dump_validation*" \) \
            -type f 2>/dev/null | sort
    fi
}

# --- Main ---
{
    log "=== oak-keyring Dump Protection Validation ==="
    log "macOS: $(sw_vers -productName) $(sw_vers -productVersion) ($(sw_vers -buildVersion))"
    log "Kernel: $(uname -r)"
    log "Arch: $(uname -m)"
    log "Crash type: $CRASH_TYPE"
    log ""

    # --- Step 1: Build harness ---
    log "--- Step 1: Building harness ---"
    cd "$PROJECT_ROOT"
    if ! cargo build --example "$HARNESS_NAME" 2>&1; then
        log "ERROR: Failed to build harness"
        exit 1
    fi
    EXAMPLE_BIN="${PROJECT_ROOT}/target/debug/examples/${HARNESS_NAME}"
    log "Binary: $EXAMPLE_BIN"
    log ""

    # --- Step 2: Record pre-existing artifacts ---
    log "--- Step 2: Recording pre-existing crash artifacts ---"
    PRE_CORES_FILE="$(mktemp)"
    PRE_REPORTS_FILE="$(mktemp)"
    ls -1 "$CORES_DIR" 2>/dev/null > "$PRE_CORES_FILE" || true
    list_harness_artifacts "$REPORTS_DIR" > "$PRE_REPORTS_FILE" || true
    PRE_CORE_COUNT="$(wc -l < "$PRE_CORES_FILE" | tr -d ' ')"
    PRE_REPORT_COUNT="$(wc -l < "$PRE_REPORTS_FILE" | tr -d ' ')"
    log "Pre-existing cores in $CORES_DIR: $PRE_CORE_COUNT"
    log "Pre-existing harness reports: $PRE_REPORT_COUNT"
    log ""

    # --- Step 3: Run protected harness ---
    log "--- Step 3: Running PROTECTED harness ---"
    log "Command: $EXAMPLE_BIN --mode protected --crash $CRASH_TYPE"
    log ""

    EXIT_CODE=0
    "$EXAMPLE_BIN" --mode protected --crash "$CRASH_TYPE" 2>&1 || EXIT_CODE=$?
    log "Harness exited with code: $EXIT_CODE (expected: signal, non-zero)"
    log ""

    # Wait for macOS CrashReporter to process
    log "Waiting 5s for CrashReporter to finish..."
    sleep 5

    # --- Step 4: Check for new artifacts ---
    log "--- Step 4: Checking for new crash artifacts ---"

    POST_CORES_FILE="$(mktemp)"
    POST_REPORTS_FILE="$(mktemp)"
    ls -1 "$CORES_DIR" 2>/dev/null > "$POST_CORES_FILE" || true
    list_harness_artifacts "$REPORTS_DIR" > "$POST_REPORTS_FILE" || true

    NEW_ARTIFACTS_FILE="$(mktemp)"
    comm -13 "$PRE_CORES_FILE" "$POST_CORES_FILE" | while read -r f; do
        [[ -n "$f" ]] && echo "$CORES_DIR/$f"
    done >> "$NEW_ARTIFACTS_FILE"
    comm -13 "$PRE_REPORTS_FILE" "$POST_REPORTS_FILE" | while read -r f; do
        [[ -n "$f" ]] && echo "$f"
    done >> "$NEW_ARTIFACTS_FILE"

    NEW_COUNT="$(wc -l < "$NEW_ARTIFACTS_FILE" | tr -d ' ')"
    log "New artifacts found: $NEW_COUNT"
    if [[ "$NEW_COUNT" -gt 0 ]]; then
        while read -r f; do
            [[ -n "$f" ]] && log "  $f ($(du -h "$f" 2>/dev/null | cut -f1))"
        done < "$NEW_ARTIFACTS_FILE"
    fi
    log ""

    # --- Step 5: Search for markers ---
    log "--- Step 5: Searching for marker secrets in artifacts ---"
    FOUND_MARKERS=0

    if [[ "$NEW_COUNT" -gt 0 ]]; then
        while read -r artifact; do
            if [[ -n "$artifact" ]] && [[ -f "$artifact" ]]; then
                log "Searching: $artifact"
                for marker in "$MARKER_LOCKED_BYTES" "$MARKER_LOCKED_KEY" "$MARKER_HEAP"; do
                    if strings "$artifact" 2>/dev/null | grep -qF "$marker"; then
                        log "  !!! FOUND marker in $(basename "$artifact"): $marker"
                        FOUND_MARKERS=$((FOUND_MARKERS + 1))
                    fi
                done
                if [[ $FOUND_MARKERS -eq 0 ]]; then
                    log "  No markers found in this artifact"
                fi
            fi
        done < "$NEW_ARTIFACTS_FILE"
    fi
    log ""

    # --- Step 6: Report results ---
    log "--- Step 6: Result ---"

    if [[ "$NEW_COUNT" -eq 0 ]]; then
        log "PASS: No crash artifacts produced."
        log "Result: DUMP PREVENTED by process protections."
        log ""
        log "This confirms setrlimit(RLIMIT_CORE, 0) and/or ptrace(PT_DENY_ATTACH)"
        log "successfully prevented crash/core dump generation."
        RESULT="DUMP_PREVENTED"
    elif [[ $FOUND_MARKERS -eq 0 ]]; then
        log "PASS: Crash artifacts produced but NO marker secrets found."
        log "Result: DUMP EXISTS BUT MARKERS ABSENT."
        log ""
        log "Crash artifacts were generated but synthetic markers are not present,"
        log "suggesting secrets were excluded from the dump."
        RESULT="DUMP_EXISTS_MARKERS_ABSENT"
    else
        log "FAIL: BLOCKING SECURITY BUG"
        log "Result: Marker secrets found in crash dump artifacts!"
        log ""
        log "This means process protections did NOT prevent secrets from appearing"
        log "in crash/core dumps. This must be fixed before any security release."
        RESULT="BLOCKING_SECURITY_BUG"
    fi

    log ""
    log "=== Validation complete ==="
    log "Result: $RESULT"
    log "Crash type: $CRASH_TYPE"
    log "macOS: $(sw_vers -productVersion)"
    log "Date: $(date -u '+%Y-%m-%dT%H:%M:%SZ')"

    # Cleanup temp files
    rm -f "$PRE_CORES_FILE" "$PRE_REPORTS_FILE" "$POST_CORES_FILE" "$POST_REPORTS_FILE" "$NEW_ARTIFACTS_FILE"

    if [[ "$RESULT" == "BLOCKING_SECURITY_BUG" ]]; then
        exit 1
    fi
} 2>&1 | tee "$RESULT_FILE"

echo ""
echo "Result written to: $RESULT_FILE"
