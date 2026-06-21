# Threat Model

oak-keyring is a local-first password manager with a full-screen, keyboard-driven terminal UI.

This document describes the current security assumptions, intended protections, non-goals, and known boundaries of the project. It is not a formal security audit.

oak-keyring is currently pre-1.0 preview software. The design, vault format, sync behavior, packaging, and platform support may change before a stable release.

## Security Goals

oak-keyring aims to protect user vault data in ordinary local-first usage scenarios:

* Keep vault contents encrypted at rest.
* Require the master password or recovery flow before vault access.
* Support local vault management without requiring a hosted account.
* Reduce accidental exposure during normal terminal workflows.
* Keep daily credential workflows such as browsing, searching, editing, tagging, recovery, and copying credentials local and keyboard-driven.
* Make security-sensitive behavior explicit enough for users and contributors to review.

## Assets Protected

The main assets oak-keyring is intended to protect are:

* Passwords
* Usernames
* API keys
* SSH-related secrets
* Secure notes
* Tags and record metadata
* Recovery words
* Local vault database
* Sync-related local configuration
* Import/export data while handled by the user

## Current Architecture Scope

Normal release builds use a SQLCipher-backed local database.

The app uses a master password and recovery words for vault access and recovery.

oak-keyring does not provide a hosted account recovery service. If both the master password and recovery words are lost, maintainers cannot recover the vault.

Optional Google Drive sync is currently a preview feature. It should be understood as optional sync within the implemented product scope, not as a hosted custody model.

## In Scope

This threat model currently covers:

* Local vault storage
* Vault unlock and lock behavior
* Master password based access
* Recovery words
* Terminal UI workflows
* Clipboard copy workflows
* Import and export workflows
* Optional Google Drive sync preview
* Release asset verification expectations
* Local configuration that may affect vault or sync behavior

## Out of Scope

The following are not currently covered as solved security problems:

* A compromised operating system
* Malware running as the same user
* Keyloggers
* Clipboard stealers
* Screen capture malware
* Malicious terminal emulators
* Memory extraction by a privileged attacker
* Kernel-level compromise
* Physical attacks against an unlocked device
* Supply-chain compromise of dependencies, build infrastructure, package managers, or release channels
* Weak master passwords chosen by users
* Loss of both master password and recovery words
* User mistakes when exporting, copying, backing up, or syncing secrets

## Attacker Model

oak-keyring mainly considers these attacker categories:

### Casual Local Attacker

An attacker who can access files on disk but does not know the master password.

oak-keyring should make vault contents unreadable to this attacker when the vault is locked and the master password is unknown.

### Accidental Exposure

A user may accidentally reveal secrets through copy/paste, screenshots, logs, terminal scrollback, shell history, exported files, or shared screen sessions.

oak-keyring should reduce accidental exposure where practical, but users remain responsible for where copied, exported, or displayed secrets go.

### Lost Device

A user may lose a laptop or storage device containing the local vault.

The local vault should remain protected by the master password and recovery design, assuming the master password is strong and the device is not already compromised or unlocked.

### Sync Conflict or Sync Exposure

A user may enable optional Google Drive sync.

oak-keyring should treat sync as a transport and storage convenience, not as a hosted trust or custody model. Users should understand that sync introduces additional operational risks, including account compromise, stale data, conflict handling, and cloud provider availability.

### Malicious Local Software

Malware running on the same machine may read clipboard contents, capture keystrokes, inspect process memory, capture the screen, or tamper with local files.

oak-keyring does not claim to protect against a compromised local environment.

## Trust Assumptions

oak-keyring assumes:

* The operating system is not compromised.
* The terminal emulator is not malicious.
* The user chooses a strong master password.
* The user stores recovery words safely and separately from the device.
* Release binaries or package channels are verified by the user before use.
* The user understands the risks of copying, exporting, backing up, and syncing secrets.
* Dependencies, build tooling, and package distribution channels have not been compromised.

## Non-Goals

oak-keyring does not currently aim to provide:

* Hosted password manager accounts
* Hosted account recovery
* Browser extension autofill
* Enterprise policy management
* Multi-user vault sharing
* Hardware security key enforcement
* Protection against malware on the same machine
* Protection against clipboard-monitoring software
* Protection against keyloggers
* Protection against screen capture
* Protection against a malicious or compromised operating system
* A guarantee that preview builds are suitable for high-risk production use

## Clipboard Handling

Clipboard workflows are inherently risky because other local applications may observe or retain clipboard contents.

oak-keyring should treat clipboard copy as a convenience feature with explicit user intent. Users should avoid copying secrets in untrusted environments, during screen sharing, or on machines where clipboard history tools, remote desktop tools, or malware may be present.

Future versions may improve clipboard handling through timeout-based clearing, clearer UI warnings, or configurable copy behavior.

## Terminal UI Boundaries

oak-keyring runs inside the user's terminal. This has important security implications:

* A malicious terminal emulator may observe displayed content or keystrokes.
* Terminal scrollback may retain visible content.
* Screen sharing or screenshots may expose secrets.
* Shell-level logging or session recording tools may capture sensitive workflows.
* Nerd Font or rendering differences are UI concerns, not security controls.

Users should treat the terminal session as sensitive while the vault is unlocked.

## Recovery Words

Recovery words are equivalent to a sensitive recovery secret.

Users must store recovery words offline or in a separate trusted location. If recovery words are exposed, an attacker may be able to recover access depending on the implementation and surrounding controls.

Maintainers cannot recover a vault if both the master password and recovery words are lost.

## Import and Export

Import and export workflows may create files containing sensitive vault data.

Users are responsible for protecting exported files, deleting temporary files, avoiding cloud folders unless intended, and ensuring backups are stored securely.

oak-keyring should avoid silently creating long-lived plaintext secret material where possible, but users should treat all import/export files as sensitive.

## Sync Preview

Google Drive sync is optional and currently preview-stage.

Sync does not mean hosted custody by OpenKeyring. Users remain responsible for their Google account security, local device security, backups, and recovery material.

Potential sync risks include:

* Cloud account compromise
* Sync conflicts
* Stale or overwritten vault data
* Loss of access to the cloud account
* Misconfigured OAuth credentials in source builds
* Accidental exposure through cloud backup or sharing settings

## Release and Distribution Risks

Preview builds may be unsigned and not notarized.

Users should download oak-keyring from official release or package channels and verify checksums when installing release assets directly.

Source builds may require explicit OAuth2 configuration for sync-related behavior. Users should avoid embedding real secrets in public repositories, logs, screenshots, or shared build environments.

## Known Limitations

Current known limitations include:

* Pre-1.0 preview status
* No external security audit yet
* macOS and Linux (glibc 2.35+) platform support
* Windows and Linux Alpine/musl not supported yet
* Unsigned and non-notarized preview binaries
* Optional sync is preview-stage
* No hosted account recovery
* No protection against compromised local machines
* No protection against malicious terminal emulators
* No formal support SLA

## Security Reports

If you believe you have found a vulnerability, do not open a public issue.

Please follow the process in `SECURITY.md`.

## Review Status

This threat model should be updated whenever oak-keyring changes its vault format, encryption design, recovery design, sync model, clipboard behavior, platform support, packaging, or release process.
