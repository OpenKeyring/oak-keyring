# Support

oak-keyring is in first-preview status. Support is community-style and best effort through GitHub. There is no formal SLA.

## Where to Ask

- Use GitHub Issues for reproducible bugs, installation failures, crashes, and documentation problems.
- Use GitHub Discussions for usage questions, early feedback, and troubleshooting that may not be a product bug.
- Do not post passwords, vault files, recovery words, OAuth secrets, tokens, or private logs in public issues or discussions.

## What to Include

For bugs or install problems, include:

- OS, version, and architecture (for example: macOS Apple Silicon, Linux x86_64 on Ubuntu 22.04).
- Install method: GitHub Release build, npm bundled binary package, Homebrew, or developer source build.
- oak-keyring version from `ok --version`.
- What you expected to happen.
- What actually happened.
- Exact error text, command output, or a short log excerpt with secrets removed.
- Whether this is a new vault, restored vault, imported data, or existing preview data.

## Preview Limitations

- macOS Apple Silicon/Intel and Linux x86_64/ARM64 (glibc 2.35+) are supported.
- Alpine (musl) and Windows are not supported yet.
- Preview builds are unsigned and not notarized.
- Preview data formats may change before stable releases.
