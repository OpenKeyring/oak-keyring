# @openkeyring/ok

Node.js wrapper for the Open-Keyring `ok` command.

This package does not download binaries during installation. It selects one of the bundled platform packages installed through optional dependencies:

- `@openkeyring/ok-darwin-arm64` — macOS Apple Silicon
- `@openkeyring/ok-darwin-x64` — macOS Intel
- `@openkeyring/ok-linux-x64` — Linux x86_64 (glibc 2.35+)
- `@openkeyring/ok-linux-arm64` — Linux ARM64 (glibc 2.35+)

Current preview support covers macOS Apple Silicon/Intel and Linux x86_64/ARM64 (glibc 2.35+). Windows is not supported yet.
