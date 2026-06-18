# Security Policy

oak-keyring is a local-first password manager currently in pre-1.0 preview status.

It has not received an external security audit yet. Please do not treat preview releases as a fully hardened replacement for an established password manager. The current goal is to make the local vault workflow, terminal UX, and security model easier to review and improve before a stable release.

Please report suspected security vulnerabilities responsibly and avoid public disclosure before maintainers have had time to investigate.

## Supported Versions

Only the latest preview release is supported for security fixes. Older preview builds may be superseded quickly, and preview data formats may change before a stable release line exists.

| Version                | Status                    |
| ---------------------- | ------------------------- |
| Latest preview release | Supported                 |
| Older preview releases | Best effort / unsupported |
| Stable release line    | Not available yet         |

## Supported Platforms

The current preview supports macOS on Apple Silicon and Intel.

| Platform            | Status            |
| ------------------- | ----------------- |
| macOS Apple Silicon | Supported         |
| macOS Intel         | Supported         |
| Linux               | Not supported yet |
| Windows             | Not supported yet |

Preview builds are currently unsigned and not notarized. Before running oak-keyring, verify that downloads come from the official OpenKeyring GitHub release or package channel.

## Reporting a Vulnerability

Use one of these private channels:

* GitHub Security Advisory: `https://github.com/OpenKeyring/oak-keyring/security/advisories/new`
* Email: `alphaqiu@gmail.com`

Do not use public GitHub issues, discussions, chat logs, or social media for vulnerability reports.

For non-security bugs, feature requests, UX feedback, packaging issues, or documentation improvements, please use public GitHub issues.

## What to Include

Please include as much of the following as possible:

* A short description of the issue and likely impact.
* Steps to reproduce, proof-of-concept details, or affected commands.
* The oak-keyring version from `ok --version`.
* Your macOS version and Mac architecture.
* Whether the issue involves a new vault, restored vault, imported data, or synced data.
* Any logs or screenshots with secrets removed.

## Secret Handling Boundaries

Never send real passwords, vault databases, recovery words, OAuth client secrets, tokens, private keys, or full logs containing sensitive values unless a maintainer explicitly arranges a private, minimized exchange.

If a reproduction needs sample data, create a disposable vault with fake records and fake credentials.

## Expected Response

Maintainers aim to acknowledge private reports within 7 days.

During the pre-1.0 preview phase, investigation and fix timing is best effort and does not come with a formal SLA.

## Disclosure

Please allow a reasonable amount of time for investigation and remediation before public disclosure. Coordinated disclosure is appreciated, especially for issues that may affect vault confidentiality, vault integrity, recovery flows, sync behavior, clipboard handling, or credential exposure.
