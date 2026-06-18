# oak-keyring

English | [简体中文](README-ZH.md)

[![Release](https://img.shields.io/github/v/release/OpenKeyring/oak-keyring?include_prereleases&label=release)](https://github.com/OpenKeyring/oak-keyring/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/OpenKeyring/oak-keyring/ci.yml?branch=develop&label=ci)](https://github.com/OpenKeyring/oak-keyring/actions/workflows/ci.yml?query=branch%3Adevelop)
[![npm](https://img.shields.io/npm/v/%40openkeyring%2Fok?label=npm)](https://www.npmjs.com/package/@openkeyring/ok)
[![License](https://img.shields.io/github/license/OpenKeyring/oak-keyring)](LICENSE)

oak-keyring is a privacy-first, local-first password manager with a
keyboard-driven terminal UI.

Many password tools provide scriptable CLIs, but daily vault management also
needs browsing, selection, confirmation, recovery, and status feedback.
oak-keyring uses a full-screen TUI so those workflows stay interactive,
keyboard-driven, and local.

The command-line binary is `ok`.

![TUI vault browser: navigate, edit, and copy credentials with keyboard shortcuts](examples/demo.gif)

## Features

- **Vault management** — browse, create, edit, and delete credentials and
  secure notes
- **Password generator** — standalone or embedded in forms, configurable
  length and character sets
- **Keyboard-driven TUI** — full-screen interface with sidebar navigation,
  search, and batch operations
- **Tags and trash** — organize records with tags; soft-delete with trash
  and restore
- **Import and export** — transfer data in and out of the vault
- **Vault recovery** — recover access with BIP-39 recovery words
- **Sync** — optional cloud sync via Google Drive (preview)
- **Auto-lock** — lock the vault after inactivity
- **Password health** — leaked password indicators and health checks
- **macOS** — Apple Silicon and Intel builds (preview)

## Install

### GitHub Release (recommended)

1. Download the tarball matching your Mac architecture.
2. Verify `checksums.txt`.
3. Unpack and run `ok --version`.

Preview builds are unsigned and not notarized. macOS may require manual
approval.

### Homebrew

```bash
brew tap openkeyring/oak-keyring
brew install ok
```

### npm

```bash
npm install -g @openkeyring/ok
ok --version
```

### Source

```bash
git clone https://github.com/OpenKeyring/oak-keyring.git
cd oak-keyring
cp .env.example .env
# Edit .env and set OAK_GOOGLE_CLIENT_ID and OAK_GOOGLE_CLIENT_SECRET.
cargo build --release
./target/release/ok --version
```

Source builds embed Google OAuth2 configuration for sync. Use source builds
for development or local inspection, and configure OAuth2 values explicitly.

> [!TIP]
> Recommended: use a Nerd Font in your terminal so icons display correctly.

## Quick Start

Start the app:

```bash
ok
```

On first run, create a vault, choose a strong master password, and save the
recovery words somewhere safe. If both the master password and recovery words
are lost, maintainers cannot recover your vault.

## Basic Usage

oak-keyring opens into a full-screen terminal interface. The main workflow is:

1. **Create or unlock a vault** — start `ok`, then create a local vault on
   first run or unlock an existing vault with your master password.
2. **Browse and search records** — use the sidebar and record list to move
   through credentials, secure notes, tags, and trash. Use `Ctrl+K` to enter
   search mode, then `Enter` to keep the filtered result or `Esc` to cancel.
3. **View and copy secrets** — select a record to inspect its details. In the
   detail panel, use `c` to copy the password field, `u` to copy the username
   field, and `p` to reveal or hide password fields when available.
4. **Create and edit records** — use `n` to create a new record and `e` to edit
   the selected record outside trash.
5. **Generate passwords** — open the password generator from the main screen
   with `Ctrl+G`, or use the generator when it appears inside record forms.
6. **Configure sync** — open Config with `Ctrl+P`. Google Drive sync is
   optional and still part of the preview boundary; after it is configured,
   `Ctrl+R` triggers sync from the main screen.
7. **Import and export** — use the TUI import/export flows when moving data
   into or out of the vault. Treat exported files as sensitive data.

For the current website documentation, see
[openkeyring.com/en/docs/](https://openkeyring.com/en/docs/).

## Community and Support

Welcome to the OpenKeyring community. If you need help, have questions, or
want to discuss the project, use the official channels below.

- **GitHub Issues** — use
  [GitHub Issues](https://github.com/OpenKeyring/oak-keyring/issues) for bug
  reports, installation problems, and feature requests.
- **Discord Server** — [Join OpenKeyring Discord](https://discord.gg/3xnuu2bQz)
  for text chat, quick questions, and community discussion.

Support is community-style and best effort. There is no formal SLA.

## Preview Status

oak-keyring is pre-1.0 preview software (v0.8.0-preview.1).

- Current builds target macOS (Apple Silicon and Intel); Linux and Windows
  are not yet available.
- macOS binaries are unsigned and not notarized.
- Vault data, configuration, and packaging may change before a stable release.
- There is no formal support SLA.
- You are responsible for your master password, recovery words, and backups.

## Security and Privacy

oak-keyring is local-first: the vault belongs to the user and is stored locally
by default. Normal release builds use a SQLCipher-backed local database. The
app uses a master password and recovery words for vault access and recovery.

The preview does not provide a hosted account recovery service. Keep recovery
words and backups separate from the device running oak-keyring. Any sync
features should be treated within the currently implemented product scope, not
as a hosted custody model.

If you download release assets directly, verify checksums before running the
binary. Report security issues through [SECURITY.md](SECURITY.md) and [PRIVACY.md](PRIVACY.md).

## Links

- [Website Docs](https://openkeyring.com/en/docs/) — install, usage,
  shortcuts, security, and preview status
- [SECURITY.md](SECURITY.md) — vulnerability reporting and security boundaries
- [THREAT_MODEL.md](THREAT_MODEL.md) — security assumptions, non-goals, and threat boundaries
- [PRIVACY.md](PRIVACY.md) — local data handling, optional sync, telemetry, and privacy boundaries
- [CONTRIBUTING.md](CONTRIBUTING.md) — how to contribute
- [CHANGELOG.md](CHANGELOG.md) — release history
- [LICENSE](LICENSE) — MIT license
