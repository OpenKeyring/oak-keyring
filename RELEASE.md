# First Preview Release Checklist

This checklist is for preparing the first OKI-0012 preview release of `ok`.

## Prerequisites

- Confirm `Cargo.toml` contains the release version.
- Confirm the working tree contains only intended release changes.
- Install the Rust targets:
  - `aarch64-apple-darwin`
  - `x86_64-apple-darwin`
  - `x86_64-unknown-linux-gnu`
  - `aarch64-unknown-linux-gnu`
- Run the release gate checks before packaging:
  - `cargo fmt --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test`
  - Any release-specific smoke test for the packaged `ok` binary.
- Set `OAK_GOOGLE_CLIENT_ID` and `OAK_GOOGLE_CLIENT_SECRET` as required by the build environment. Dummy local values are acceptable for local verification only.

## Build Artifacts

The release workflow (`.github/workflows/release.yml`) builds all four targets on tag push:

- Apple Silicon: `aarch64-apple-darwin` (macos-14 runner)
- Intel: `x86_64-apple-darwin` (macos-14 runner)
- Linux x86_64: `x86_64-unknown-linux-gnu` (ubuntu-22.04 runner, glibc 2.35 baseline)
- Linux ARM64: `aarch64-unknown-linux-gnu` (ubuntu-22.04-arm runner, glibc 2.35 baseline)

For a local macOS preview build only:

```bash
scripts/package-preview.sh
```

Expected archives (the release workflow produces all four; local preview produces only the two macOS archives):

- `dist/ok-v<version>-aarch64-apple-darwin.tar.gz`
- `dist/ok-v<version>-x86_64-apple-darwin.tar.gz`
- `dist/ok-v<version>-x86_64-unknown-linux-gnu.tar.gz`
- `dist/ok-v<version>-aarch64-unknown-linux-gnu.tar.gz`
- `dist/checksums.txt`

Each archive contains the `ok` binary and project docs that are present in the repo root, currently `README.md` and `LICENSE`.

## Signing Caveat

The first preview artifacts are unsigned and not notarized.

Self-signing or ad-hoc signing can be done locally for testing, but it is not equivalent to Developer ID signing and Apple notarization. Users may see Gatekeeper warnings for unsigned preview builds.

## GitHub Release

- Create the release tag after the release gate checks pass.
- Upload the target archives and `dist/checksums.txt`.
- Use artifact names exactly matching the packaging output:
  - `ok-v<version>-aarch64-apple-darwin.tar.gz`
  - `ok-v<version>-x86_64-apple-darwin.tar.gz`
  - `ok-v<version>-x86_64-unknown-linux-gnu.tar.gz`
  - `ok-v<version>-aarch64-unknown-linux-gnu.tar.gz`
  - `checksums.txt`
- Include the checksum file contents in the release notes or link directly to the uploaded file.
- Call out that this is a preview build, that macOS artifacts are unsigned and not notarized, and that Linux artifacts require glibc 2.35+ and may need `RLIMIT_MEMLOCK` raised (see INSTALL.md).

## npm Packages

The npm packages are staged from already-built release artifacts. They do not download binaries during `postinstall`.

Package names:

- `@openkeyring/ok`
- `@openkeyring/ok-darwin-arm64`
- `@openkeyring/ok-darwin-x64`
- `@openkeyring/ok-linux-x64`
- `@openkeyring/ok-linux-arm64`

Dry-run before publishing:

```bash
npm/stage-binaries.sh
cd npm/ok-darwin-arm64 && npm pack --dry-run
cd ../ok-darwin-x64 && npm pack --dry-run
cd ../ok-linux-x64 && npm pack --dry-run
cd ../ok-linux-arm64 && npm pack --dry-run
cd ../ok && npm pack --dry-run
```

Publish order (platform packages first, main wrapper last):

1. `@openkeyring/ok-darwin-arm64`
2. `@openkeyring/ok-darwin-x64`
3. `@openkeyring/ok-linux-x64`
4. `@openkeyring/ok-linux-arm64`
5. `@openkeyring/ok`

Publishing platform packages first prevents the main wrapper package from depending on package versions that are not available yet.

## Homebrew

Update the Homebrew tap formula after the GitHub Release is published:

```bash
cd homebrew-oak-keyring
# Update the formula with the new version, URL, and SHA256
# Update both on_macos and on_linux blocks (4 url/sha256 entries total)
# Push to the tap repository
```

Tap: `openkeyring/oak-keyring`

Install command:

```bash
brew tap openkeyring/oak-keyring
brew install ok
```

Do not publish a Homebrew source-build formula. The current build embeds
Google OAuth2 configuration for sync, so a normal user source build through
Homebrew would not carry the release OAuth2 configuration.

## Website Check

After artifacts and install channels are finalized, check the website install page:

- Artifact names and versions match the GitHub release.
- npm package names and install commands match the published packages.
- Homebrew install instructions match the published tap formula.
- The preview signing/notarization caveat is visible where macOS users will see it.
