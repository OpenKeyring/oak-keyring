# oak-keyring

English | [简体中文](README-ZH.md)

oak-keyring is a privacy-first, local-first password manager with a
keyboard-driven terminal UI.

It is built for people who are comfortable in the terminal but still want an
interactive full-screen vault experience: browse records, create and edit
credentials, generate passwords, copy secrets, import and export data, and
recover a vault without leaving the TUI.

The command-line binary is `ok`.

## Preview Status

oak-keyring is pre-1.0 preview software. The first public preview is intended
for trying the product, validating the installation paths, and collecting early
feedback before a later stable release policy exists.

Important preview boundaries:

- Do not rely on this preview as your only password vault.
- The first preview includes macOS builds for Apple Silicon and Intel Macs.
- Linux and Windows builds are not included in this preview.
- The macOS preview binaries are unsigned and not notarized.
- Vault data, configuration, and release packaging may change before a later
  stable release.
- There is no formal support SLA.
- You are responsible for your master password, recovery words, and backups.

## Why a Terminal UI?

Many password tools provide scriptable CLIs, but daily vault management also
needs browsing, selection, confirmation, recovery, and status feedback.
oak-keyring uses a TUI so those workflows stay interactive, keyboard-driven,
and local.

## Current Platform Support

The first preview supports macOS only:

| Platform | Target |
| --- | --- |
| Apple Silicon Mac | `aarch64-apple-darwin` |
| Intel Mac | `x86_64-apple-darwin` |

The product direction remains broader than this first preview. Linux and
Windows builds are not part of the initial release set.

## Install Options

Recommended GitHub Release unsigned/not-notarized build:

1. Download the tarball matching your Mac architecture.
2. Verify `checksums.txt`.
3. Unpack the archive and run `ok --version`.

The GitHub Release builds for the first preview are unsigned and not notarized.
macOS may require user approval before running them. Ad-hoc or self-signed
local signing is useful for testing, but it is not equivalent to Apple
Developer ID signing and notarization.

npm convenience install:

```bash
npm install -g @openkeyring/ok
ok --version
```

The npm package is expected to bundle the matching macOS platform binary. It is
a convenience install path, not the primary security trust root.

Developer source build:

```bash
git clone https://github.com/OpenKeyring/oak-keyring.git
cd oak-keyring
cp .env.example .env
# Edit .env and set OAK_GOOGLE_CLIENT_ID and OAK_GOOGLE_CLIENT_SECRET.
cargo build --release
./target/release/ok --version
```

Source builds are not the primary preview distribution path because the current
build embeds Google OAuth2 configuration for sync. Use source builds for
development or local inspection, and configure OAuth2 values explicitly.

## First Run Basics

Start the app:

```bash
ok
```

On first run, create a vault, choose a strong master password, and save the
recovery words somewhere safe. If both the master password and recovery words
are lost, maintainers cannot recover your vault.

## Security and Privacy Expectations

oak-keyring is local-first: the vault belongs to the user and is stored locally
by default. Normal release builds use a SQLCipher-backed local database. The
app uses a master password and recovery words for vault access and recovery.

The preview does not provide a hosted account recovery service. Keep recovery
words and backups separate from the device running oak-keyring. Any sync
features should be treated within the currently implemented product scope, not
as a hosted custody model.

If you download release assets directly, verify checksums before running the
binary. Report security issues through [SECURITY.md](SECURITY.md).

## Documentation Links

- [SECURITY.md](SECURITY.md): vulnerability reporting and security boundaries.
- [LICENSE](LICENSE): MIT license.
- Project documentation: `../docs/` in the Open-Keyring workspace.
- Website source: `../website/` in the Open-Keyring workspace.

## Project Status

oak-keyring is an active preview project. The current release-readiness work is
focused on making the first macOS preview understandable, installable, and
honest about its limits before broader platform support and stronger release
guarantees are introduced.
