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
- **Linux** — x86_64 and ARM64 builds, glibc 2.35+ (preview)

## Install

### GitHub Release (recommended)

1. Download the tarball matching your OS and architecture.
2. Verify `checksums.txt`.
3. Unpack and run `ok --version`.

Preview builds are unsigned and not notarized. macOS may require manual
approval.

### Homebrew

```bash
brew tap openkeyring/oak-keyring
brew trust --formula openkeyring/oak-keyring/ok
brew install ok
```

Homebrew 6.0+ requires trusting non-official taps (macOS and Linux). See
[INSTALL.md](INSTALL.md) for the exact error and alternatives.

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

## SSH Agent Backend (`ok agent`)

`ok agent` runs an ssh-agent backend backed by the SSH keys stored in your
vault. Use it for `ssh` logins, `git` over SSH, and any tool that reads
`SSH_AUTH_SOCK` — **the private key never leaves the oak-keyring process, and
your master password never reaches AI tools or scripts.**

### How it works

1. The daemon unlocks your vault once (you type the master password), then
   listens on a Unix socket.
2. `ssh` / `git` / AI send sign requests over `SSH_AUTH_SOCK`; oak-keyring
   signs in-process and returns only the signature.
3. Private keys are decrypted per sign request and zeroized immediately —
   never cached, never written outside the vault.

### Prerequisites

- At least one **SSH key record** in your vault. Create one in the TUI:
  new record (`n`) → type **SSH** → paste the public key and the OpenSSH
  private key (with its passphrase if it has one).
- Supported types: **ed25519**, **RSA (SHA-2)**, **ECDSA (nistp256/384/521)**.

### Start the agent

```bash
ok agent
```

It prompts for your master password, then prints the socket path, for example:

```
SSH_AUTH_SOCK=/run/user/1000/oak-keyring/agent.sock
```

### Use it with ssh and git

In the shell where you run `ssh` / `git`, export that path:

```bash
export SSH_AUTH_SOCK=/run/user/1000/oak-keyring/agent.sock
ssh-add -l       # lists the vault's SSH keys
ssh user@host    # authenticates with the vault key — no ~/.ssh key file needed
git push         # same mechanism for git over SSH
```

Tip: start `ok agent` once per session (or from your shell rc with a fixed
socket path) and export `SSH_AUTH_SOCK`, so SSH tools find it automatically.

### Options

| Flag | Purpose |
| --- | --- |
| `--only NAME` | Expose only records whose name matches exactly (repeatable). |
| `--allow REGEX` | Also expose records whose name matches the regex (union with `--only`). |
| `--idle-lock SECS` | Shut down after this many seconds with no successful sign (default: never). |

Run `ok agent --help` for the full list.

### Security model

- **Private keys never leave the daemon** — only signatures cross the socket.
- **No key caching** — decrypted key material is zeroized right after each signature.
- **Master password isolation** — read once via a terminal prompt; it never
  enters command-line args, shell history, or any AI tool's context.
- **Per-sign audit** — each successful signature is recorded in the vault
  audit log as `SSH sign`.
- **Coexists with the TUI** — a separate single-instance lock, so `ok` and
  `ok agent` can run at the same time against the same vault.

> Local owner-trust model: any process running as your user can already read
> your files, so the socket is `0600`. The benefit over a plain `ssh-agent` is
> that your SSH private keys stay encrypted at rest inside the vault and are
> never written to `~/.ssh` as plaintext files.

### Stop the agent

`ok agent` runs in the foreground. Stop it with `Ctrl+C` or `kill <pid>`
(`SIGTERM` / `SIGINT`). On shutdown it locks the vault (zeroizes keys) and
removes the socket and pidfile.

### Troubleshooting

- **`ssh-add -l`: "Could not open a connection"** — `SSH_AUTH_SOCK` isn't
  exported in this shell, or points at a stale path. Re-export the path the
  agent printed.
- **`ssh-add -l` lists nothing** — no SSH key record in the vault, or all
  filtered out by `--only` / `--allow`.
- **"another agent is already running"** — an `ok agent` is already up; stop
  it first (or remove a stale `.agent.lock` in the data dir).
- **Stale socket after a crash** — `ok agent` clears a leftover socket on the
  next start; you can also delete it manually.
- **Linux memory-lock errors** — raise `RLIMIT_MEMLOCK` (see [INSTALL.md](INSTALL.md)).

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

- Current builds target macOS (Apple Silicon and Intel) and Linux (x86_64/ARM64, glibc 2.35+); Windows is not yet available. On Linux, `mlock` may need `RLIMIT_MEMLOCK` raised (see INSTALL.md).
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
