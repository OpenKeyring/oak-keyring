# First Preview Release Checklist

This checklist is for preparing the first OKI-0012 preview release of `ok`.

## Prerequisites

- Confirm `Cargo.toml` contains the release version.
- Confirm the working tree contains only intended release changes.
- Install the Rust targets:
  - `aarch64-apple-darwin`
  - `x86_64-apple-darwin`
- Run the release gate checks before packaging:
  - `cargo fmt --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test`
  - Any release-specific smoke test for the packaged `ok` binary.
- Set `OAK_GOOGLE_CLIENT_ID` and `OAK_GOOGLE_CLIENT_SECRET` as required by the build environment. Dummy local values are acceptable for local verification only.

## macOS Artifacts

Build and package both macOS targets:

- Apple Silicon: `aarch64-apple-darwin`
- Intel: `x86_64-apple-darwin`

Use:

```bash
scripts/package-preview.sh
```

Expected archives:

- `dist/ok-v<version>-aarch64-apple-darwin.tar.gz`
- `dist/ok-v<version>-x86_64-apple-darwin.tar.gz`
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
  - `checksums.txt`
- Include the checksum file contents in the release notes or link directly to the uploaded file.
- Call out that this is a preview build and that macOS artifacts are unsigned and not notarized.

## npm Packages

The npm packages are staged from already-built release artifacts. They do not download binaries during `postinstall`.

Package names:

- `@openkeyring/ok`
- `@openkeyring/ok-darwin-arm64`
- `@openkeyring/ok-darwin-x64`

Dry-run before publishing:

```bash
npm/stage-binaries.sh
cd npm/ok-darwin-arm64 && npm pack --dry-run
cd ../ok-darwin-x64 && npm pack --dry-run
cd ../ok && npm pack --dry-run
```

Publish order:

1. `@openkeyring/ok-darwin-arm64`
2. `@openkeyring/ok-darwin-x64`
3. `@openkeyring/ok`

Publishing platform packages first prevents the main wrapper package from depending on package versions that are not available yet.

## Homebrew Status

Do not publish a Homebrew source-build formula for the first preview. The
current build embeds Google OAuth2 configuration for sync, so a normal user
source build through Homebrew would not carry the release OAuth2 configuration.

## Website Check

After artifacts and install channels are finalized, check the website install page:

- Artifact names and versions match the GitHub release.
- npm package names and install commands match the published packages.
- Website install instructions do not advertise Homebrew for the first preview.
- The preview signing/notarization caveat is visible where macOS users will see it.
