# Install oak-keyring

oak-keyring is a privacy-first, local-first preview password manager for the OpenKeyring project. The CLI binary is `ok`.

## Preview Support Boundary

- Supported operating systems: macOS on Apple Silicon and Intel.
- Linux and Windows are not supported in this first preview.
- Preview builds are unsigned and not notarized. macOS Gatekeeper may warn before first launch.
- The local vault and sync data formats may change before a stable release. Preview data does not carry a compatibility guarantee.
- Community support is best effort through GitHub Issues and Discussions. There is no formal SLA.

Back up any vault data before upgrading between preview builds.

## GitHub Release Builds

For the first preview, GitHub Release assets are the primary user installation path. Assets are expected to be unsigned macOS builds for Apple Silicon and Intel.

1. Open the latest release at `https://github.com/OpenKeyring/oak-keyring/releases`.
2. Download the asset that matches your Mac architecture.
3. Unpack it and move `ok` into a directory on your `PATH`, such as `/usr/local/bin` or `~/.local/bin`.
4. Verify it:

```bash
ok --version
```

If macOS blocks the unsigned preview binary, use Finder or System Settings to allow the app after confirming that you downloaded it from the official GitHub release page.

## npm Bundled Binary Package

If an npm package is published for the preview, it should install a bundled `ok` binary for macOS:

```bash
npm install -g @openkeyring/ok
ok --version
```

Use the GitHub Release path if the npm package is not available for your architecture yet.

## Developer Source Build

Source builds are not the primary preview distribution path because the current build embeds Google OAuth2 configuration for sync. Use this path for development or source inspection.

Prerequisites:

- macOS on Apple Silicon or Intel
- Rust toolchain from `rustup`
- Xcode Command Line Tools
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

## Updating

For preview releases, read `CHANGELOG.md` before upgrading. Data format compatibility is not guaranteed until a stable release line exists.
