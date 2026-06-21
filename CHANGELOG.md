# Changelog

All notable user-facing changes will be documented in this file.

This project is in first-preview status. Preview releases may change local vault, sync, import, and export formats before a stable release line exists.

## Unreleased

- Added Linux x86_64/ARM64 (glibc 2.35+) release builds, distribution, and documentation support: release workflow now builds `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` targets; npm ships `@openkeyring/ok-linux-x64` and `@openkeyring/ok-linux-arm64`; Homebrew formula adds an `on_linux` block; INSTALL documents `mlock`/`RLIMIT_MEMLOCK` requirements.
- Added first-preview installation, support, contribution, security, and issue-reporting documentation.
- Updated the preview boundary: macOS Apple Silicon/Intel and Linux x86_64/ARM64 (glibc 2.35+) are supported; Alpine (musl) and Windows are not yet.
- Clarified that preview builds are unsigned and not notarized, with best-effort community support and no formal SLA.

## 0.8.0-preview.1

First preview release with new-look TUI theme and broad polish pass.

### Features

- **New-look TUI theme**: Tokyo Night-based visual overhaul across all screens.
- **SecureNote credential type**: textarea component + SecureNote as a first-class credential; TextArea widget overlay rendering in forms for notes editing.
- **Password generator UX**: improved standalone and embedded generator interactions, refined generator sizing.
- **File logging**: structured log output for diagnostics.
- **List panel improvements**: search restore after navigation, scrollbar, sort fix, leaked password indicator.
- **Detail panel improvements**: metadata table, layout tuning, improved password detail panel polish.
- **Sidebar focus**: improved sidebar focus styling, count badge alignment, selected badge styling preserved on navigation.

### Fixes

- **Sync**:
  - Hardened cloud restore consistency.
  - Fixed private metadata handling.
  - Repaired Google Drive OAuth sync setup.
  - Fixed sync retry handling.
- **Auto-lock**: fixed firing during active use with ActivityTracker.
- **Main screen**:
  - Refined main screen layout and interactions.
  - Polished main screen visual details.
  - Select first list item from sidebar navigation.
- **Form & editing**:
  - Improved form save feedback and sidebar tag input.
  - Refined password form interactions, layout, and overall form polish.
  - Improved export path editing.
  - Refined export and edit interactions.
- **Tag management**: repaired tag management shortcuts.
- **Visual selection**: clarified visual selection summary, repaired visual batch tag workflow.
- **Detail panel**: refined detail actions and timestamp display.
- **List panel**:
  - Widened list panel from 30% to 40% for better readability.
  - Vertically centered empty state in list panel.
  - Vertically centered the empty detail panel placeholder.
  - Improved list pagination and audit scrolling.
  - Tightened list timestamp alignment, added right margin to list item timestamps.
- **Config screen**:
  - SaveConfig exempt from vault lock check.
  - Config screen handles VaultLocked state.
  - List highlight gap fixed.
  - Improved config about panel.
  - Fixed config generator defaults.
- **Change master password screen**: polished interactions.
- **Trash**: corrected trash detail delete action, polished trash list styles.
- **Record notes**: improved TUI record notes interactions.
- **Warning styling**: polished TUI warning styles.
- **Record field limits**: enforced field length limits.
- **Password generator**: refined generator interactions.
- **Unlock screen**: polished unlock interactions.
- **Help**: updated help i18n.

### Terminal & Compatibility

- Enabled Kitty keyboard protocol to prevent ESC ambiguity freeze.
- Resolved CI test failure from terminal-size-dependent mouse hit-test.

### Build & Distribution

- Refreshed okb-gen lockfile to remove vulnerable JWT dependency.
- Updated integration test snapshots for layout and centering changes.

## 0.7.3 - First Preview Baseline

This is the baseline for the first public preview readiness pass.

- Provides the `ok` terminal TUI binary for local-first password management.
- Uses encrypted local storage with the current SQLCipher-backed vault path.
- Includes import/export, password record management, recovery-key support, and Google Drive sync preview functionality.
- Targets macOS Apple Silicon and Intel for preview distribution.
- Does not promise data-format compatibility across preview releases.
