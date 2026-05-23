# Security Policy

oak-keyring is a local-first password manager in first-preview status. Please report suspected security vulnerabilities responsibly and avoid public disclosure before maintainers have had time to investigate.

## Supported Versions and Previews

Only the latest preview release is supported for security fixes. Older preview builds may be superseded quickly, and preview data formats may change before a stable release line exists.

The current preview supports macOS on Apple Silicon and Intel. Linux and Windows are not supported yet. Preview builds are unsigned and not notarized, so verify that downloads come from the official OpenKeyring GitHub release or package channel before running them.

## Reporting a Vulnerability

Use one of these private channels:

- GitHub Security Advisory: `https://github.com/OpenKeyring/oak-keyring/security/advisories/new`
- Email: alphaqiu@gmail.com

Do not use public GitHub issues, discussions, chat logs, or social media for vulnerability reports.

## What to Include

- A short description of the issue and likely impact.
- Steps to reproduce, proof-of-concept details, or affected commands.
- oak-keyring version from `ok --version`.
- macOS version and Mac architecture.
- Whether the issue involves a new vault, restored vault, imported data, or synced data.
- Any logs or screenshots with secrets removed.

## Secret Handling Boundaries

Never send real passwords, vault databases, recovery words, OAuth client secrets, tokens, private keys, or full logs containing sensitive values unless a maintainer explicitly arranges a private, minimized exchange.

If a reproduction needs sample data, create a disposable vault with fake records and fake credentials.

## Expected Response

Maintainers aim to acknowledge private reports within 7 days. During the first-preview phase, investigation and fix timing is best effort and does not come with a formal SLA.
