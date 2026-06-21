# Contributing

Thanks for helping improve oak-keyring. The project is in a first-preview phase, so the most useful contributions are focused bug reports, installation feedback, small fixes, and documentation improvements.

## Good First Contributions

- Reproducible bug reports with environment details.
- Documentation fixes for install, support, or troubleshooting gaps.
- Small code fixes with clear tests.
- Preview packaging feedback for macOS Apple Silicon/Intel and Linux x86_64/ARM64.

Avoid large roadmap proposals, broad architecture rewrites, or speculative platform work in a first contribution. Open a discussion first for anything that changes user-visible behavior or security-sensitive code.

## Local Checks

From the repository root:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

Local builds need `OAK_GOOGLE_CLIENT_ID` and `OAK_GOOGLE_CLIENT_SECRET` in the environment or `.env`. Use `.env.example` as the template. Placeholder values are acceptable for local checks that do not exercise Google Drive sync.

## Pull Requests

- Keep PRs small and focused.
- Explain the user-visible change and the checks you ran.
- Add or update tests when behavior changes.
- Do not include secrets, vault data, recovery words, or private logs.
- Keep docs changes scoped to the behavior they describe.

## Preview Scope

The current preview supports macOS on Apple Silicon/Intel and Linux x86_64/ARM64 (glibc 2.35+). Alpine (musl) and Windows are not supported yet; preview builds are unsigned and not notarized, data formats may change, and support is best effort without a formal SLA.
