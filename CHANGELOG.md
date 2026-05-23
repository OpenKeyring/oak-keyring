# Changelog

All notable user-facing changes will be documented in this file.

This project is in first-preview status. Preview releases may change local vault, sync, import, and export formats before a stable release line exists.

## Unreleased

- Added first-preview installation, support, contribution, security, and issue-reporting documentation.
- Clarified the preview boundary: macOS Apple Silicon and Intel only; Linux and Windows are not supported yet.
- Clarified that preview builds are unsigned and not notarized, with best-effort community support and no formal SLA.

## 0.7.3 - First Preview Baseline

This is the baseline for the first public preview readiness pass.

- Provides the `ok` terminal TUI binary for local-first password management.
- Uses encrypted local storage with the current SQLCipher-backed vault path.
- Includes import/export, password record management, recovery-key support, and Google Drive sync preview functionality.
- Targets macOS Apple Silicon and Intel for preview distribution.
- Does not promise data-format compatibility across preview releases.
