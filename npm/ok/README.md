# @openkeyring/ok

Node.js wrapper for the Open-Keyring `ok` command.

This package does not download binaries during installation. It selects one of the bundled platform packages installed through optional dependencies:

- `@openkeyring/ok-darwin-arm64`
- `@openkeyring/ok-darwin-x64`

Current preview support is limited to macOS Apple Silicon and Intel.
