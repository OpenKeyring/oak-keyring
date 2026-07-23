# Install oak-keyring

oak-keyring is a privacy-first, local-first preview password manager for the OpenKeyring project. The CLI binary is `ok`.

## Preview Support Boundary

- Supported operating systems: macOS on Apple Silicon and Intel, and Linux x86_64/ARM64 with glibc 2.35 or newer (Ubuntu 22.04+, Debian 12+, Fedora, RHEL/Rocky/Alma 9+, Arch, openSUSE). Alpine (musl) and Windows are not supported yet.
- On Linux, memory locking (`mlock`) may require raising `RLIMIT_MEMLOCK`; see the Linux Memory Locking section below.
- Preview builds are unsigned and not notarized. macOS Gatekeeper may warn before first launch.
- The local vault and sync data formats may change before a stable release. Preview data does not carry a compatibility guarantee.
- Community support is best effort through GitHub Issues and Discussions. There is no formal SLA.

Back up any vault data before upgrading between preview builds.

## GitHub Release Builds

GitHub Release assets are the primary user installation path. Assets are unsigned builds for macOS (Apple Silicon and Intel) and Linux (x86_64 and ARM64, glibc 2.35+).

1. Open the latest release at `https://github.com/OpenKeyring/oak-keyring/releases`.
2. Download the asset that matches your OS and architecture.
3. Unpack it and move `ok` into a directory on your `PATH`, such as `/usr/local/bin` or `~/.local/bin`.
4. Verify it:

```bash
ok --version
```

If macOS blocks the unsigned preview binary, use Finder or System Settings to allow the app after confirming that you downloaded it from the official GitHub release page.

## Homebrew

```bash
brew tap openkeyring/oak-keyring
brew trust --formula openkeyring/oak-keyring/ok
brew install ok
```

Homebrew 6.0.0 and later require non-official taps to be trusted before their
formulae can be loaded. This applies to both macOS and Linux, not just Linux.
If you skip the `brew trust` step, the install fails with:

```text
Error: Refusing to load formula openkeyring/oak-keyring/ok from untrusted tap openkeyring/oak-keyring.
Run `brew trust --formula openkeyring/oak-keyring/ok` or `brew trust openkeyring/oak-keyring` to trust it.
```

Alternatives:

- One-liner that trusts only `ok` (no separate `brew tap`/`brew trust`):
  `brew install openkeyring/oak-keyring/ok`
- Trust the whole tap instead of a single formula:
  `brew trust openkeyring/oak-keyring`

## npm Bundled Binary Package

The npm package installs a bundled `ok` binary for macOS and Linux:

```bash
npm install -g @openkeyring/ok
ok --version
```

Use the GitHub Release path if the npm package is not available for your architecture yet.

## SSH Agent Backend (`ok agent`)

`ok agent` runs a standalone ssh-agent backend backed by the vault's SSH keys.
Start it, export the `SSH_AUTH_SOCK=<path>` it prints, then `ssh-add -l` lists
the vault's SSH keys (see the README for full usage).

```bash
ok agent
# prints: SSH_AUTH_SOCK=<path>
export SSH_AUTH_SOCK=<path>
ssh-add -l
```

`ok agent` is a long-running daemon that locks secrets in RAM with `mlock`,
so on Linux the same `RLIMIT_MEMLOCK` requirement below applies to it.

## Linux Memory Locking

`ok` locks secrets (master key, derived keys) in RAM with `mlock` so they cannot be swapped to disk. On Linux, the default `RLIMIT_MEMLOCK` is often only 64 KiB, which is too small. When `mlock` fails, `ok` fails loudly: vault creation or unlock surfaces an error rather than silently running without memory protection.

Raise the limit before running `ok`. Pick whichever fits your setup:

**Interactive session (temporary):**

```bash
ulimit -l unlimited
ok
```

**Persistent via PAM** — add to `/etc/security/limits.conf`, then log out and back in:

```
*  soft  memlock  unlimited
*  hard  memlock  unlimited
```

**systemd service** — add to the unit file:

```
[Service]
LimitMEMLOCK=infinity
```

**Capability (no ulimit needed)** — grant once, persists across reboots:

```bash
sudo setcap cap_ipc_lock=ep "$(command -v ok)"
```

This step is not needed on macOS, which does not impose the same `mlock` quota on regular users.

## Developer Source Build

Source builds are not the primary preview distribution path because the current build embeds Google OAuth2 configuration for sync. Use this path for development or source inspection.

Prerequisites:

- macOS on Apple Silicon or Intel, or Linux x86_64/ARM64 (glibc 2.35+)
- Rust toolchain from `rustup`
- On macOS: Xcode Command Line Tools. On Linux: a C compiler and build tools (`build-essential`/`gcc`/`make`)
- Google OAuth values for local builds, either in the environment or `.env`

```bash
git clone https://github.com/OpenKeyring/oak-keyring.git
cd oak-keyring
cp .env.example .env
# Edit .env and set OAK_GOOGLE_CLIENT_ID and OAK_GOOGLE_CLIENT_SECRET.
cargo build --release
./target/release/ok --version
```

For development-only local checks where Google Drive sync is not being exercised, placeholder OAuth values are enough to satisfy the build script. For sync testing, use real OAuth2 values.

> [!TIP]
> Recommended: use a Nerd Font in your terminal so icons display correctly.

## Updating

For preview releases, read `CHANGELOG.md` before upgrading. Data format compatibility is not guaranteed until a stable release line exists.
