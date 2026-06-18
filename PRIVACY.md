# Privacy Policy

Last updated: June 19, 2026

Oak Keyring (`oak-keyring`) is a local-first, open-source terminal password manager.

This document explains what information Oak Keyring processes, what stays on your device, how optional Google Drive sync works, what the public website may collect, and what the OpenKeyring project does not collect.

Oak Keyring is currently pre-1.0 preview software. This policy may be updated as the project evolves.

## Summary

By default, Oak Keyring is designed to run locally.

OpenKeyring does not operate a hosted password vault service, does not provide hosted account recovery, and does not have access to your master password, recovery words, vault key, plaintext vault contents, or local vault database.

In normal local usage:

* Your vault data stays on your device.
* Your master password is not sent to OpenKeyring.
* Your recovery words are not sent to OpenKeyring.
* Your vault key is not sent to OpenKeyring.
* Your saved credentials are not sent to OpenKeyring.
* Your passwords, API keys, SSH secrets, secure notes, tags, favorites, and metadata are not sent to OpenKeyring.
* OpenKeyring does not run a hosted backend for storing, reading, syncing, or recovering user vaults.
* The Oak Keyring app does not intentionally include telemetry, advertising tracking, analytics reporting, or automatic usage reporting.

## Local-first Vault Data

Oak Keyring stores vault data locally on your device by default.

Oak Keyring may store or process the following data locally:

* Vault records
* Usernames
* Passwords
* API keys
* SSH-related secrets
* Secure notes
* Tags
* Favorites
* Record metadata
* Vault configuration
* Local app configuration
* Recovery-related data
* Import/export files created by the user
* Sync-related local configuration if optional sync is enabled
* OAuth tokens if optional Google Drive sync is enabled

You are responsible for protecting your device, local user account, backups, exported files, terminal session, and any storage location where you place vault-related files.

## Master Password and Recovery Words

Your master password and recovery words are sensitive secrets.

OpenKeyring cannot recover your vault if you lose both your master password and your recovery words.

Do not share your master password, recovery words, vault database, OAuth tokens, private keys, API tokens, SSH keys, or real credentials in:

* GitHub issues
* GitHub discussions
* Pull requests
* Screenshots
* Logs
* Terminal recordings
* Emails
* Chat messages
* Public support requests
* Social media posts

If a bug report requires reproduction data, create a disposable test vault with fake records and fake credentials.

## No Hosted Account

Oak Keyring does not currently provide:

* Hosted user accounts
* Hosted vault storage
* Hosted account recovery
* Hosted password recovery
* Hosted secret sharing
* Enterprise administration
* Server-side access to user vaults

This means OpenKeyring does not have a server-side copy of your vault and cannot restore your data for you.

## Optional Google Drive Sync Preview

Google Drive sync is optional.

If you authorize Google Drive sync, Oak Keyring uses Google OAuth2 to access Google Drive with this scope:

```text
https://www.googleapis.com/auth/drive.file
```

This scope is used to create, read, update, and delete Oak Keyring sync files in Google Drive that the app creates or that you explicitly make available to the app.

Oak Keyring uses that access only to synchronize:

* Encrypted vault records
* Sync metadata
* Conflict data
* Sync lock files needed for backup and multi-device movement

Google Drive receives encrypted sync data. Google Drive does not receive your vault key, master password, recovery words, or plaintext saved passwords from Oak Keyring.

Google Drive sync should be understood as optional user-controlled sync, not as OpenKeyring-hosted custody.

When using Google Drive sync, you are responsible for:

* The security of your Google account
* Google account recovery settings
* Google Drive sharing permissions
* Cloud backup settings
* OAuth consent and permissions
* Sync conflicts
* Deleted or overwritten files
* Loss of access to your Google account
* Any data handling performed by Google under its own terms and privacy policy

OpenKeyring does not operate Google Drive and does not control Google's infrastructure, account security, retention, access logs, privacy practices, or service availability.

## Google API Limited Use

Oak Keyring's use and transfer of information received from Google APIs will adhere to the Google API Services User Data Policy, including the Limited Use requirements.

Google user data is used only to provide or improve the user-facing Google Drive sync feature, maintain security, comply with applicable law, or act with your consent.

OpenKeyring does not use Google user data for:

* Advertising
* User profiling
* Model training
* Sale to third parties
* Transfer to advertising platforms
* Transfer to data brokers
* Transfer to information resellers

Human access to Google user data is limited to cases where:

* You ask for support and provide the relevant information.
* Access is necessary for security or abuse investigation.
* Access is required by law.
* You explicitly consent.

## OAuth Tokens

OAuth access and refresh tokens are stored on your device in Oak Keyring's local configuration token directory.

On Unix-like systems, the token file is restricted to owner-only permissions when the operating system allows it.

You can revoke Google Drive authorization from your Google Account, delete locally stored Oak Keyring tokens, and delete Oak Keyring sync files from Google Drive.

Removing authorization or deleting sync files may stop cloud sync until you authorize Google Drive again or recreate the sync data.

If you build Oak Keyring from source or configure sync-related features manually, do not commit OAuth credentials, tokens, client secrets, configuration files, logs, screenshots, or terminal recordings containing sensitive values to public repositories.

If you suspect sync credentials have been exposed, revoke them through the relevant provider and rotate any affected credentials.

## Clipboard Handling

Oak Keyring may copy usernames, passwords, or other secrets to the system clipboard when you explicitly request it.

Clipboard contents may be visible to other local applications, clipboard history tools, remote desktop tools, screen sharing tools, malware, or operating system services.

OpenKeyring does not control how your operating system or other applications handle clipboard contents after a secret has been copied.

Avoid copying secrets on shared, compromised, remotely controlled, or untrusted machines.

## Terminal and Screen Exposure

Oak Keyring runs inside your terminal.

Secrets may be exposed through:

* Terminal display
* Terminal scrollback
* Screenshots
* Screen sharing
* Session recording
* Remote desktop tools
* Shell or terminal logging
* Malicious terminal emulators
* Local malware

Treat your terminal session as sensitive while your vault is unlocked.

If you need to share screenshots or recordings for bug reports, use a disposable test vault with fake data.

## Logs and Debug Output

Do not share logs unless you have reviewed and removed secrets.

Bug reports, screenshots, terminal recordings, crash output, debug output, and reproduction steps should not include real passwords, recovery words, private keys, tokens, vault databases, OAuth secrets, or sensitive personal data.

Oak Keyring should avoid intentionally logging secrets, but users remain responsible for reviewing any diagnostic material before sharing it.

## Import and Export

Import and export workflows may create files containing sensitive vault data.

You are responsible for where exported files are stored, how they are backed up, who can access them, and whether they are later deleted securely.

Avoid placing exported vault data in:

* Shared folders
* Public repositories
* Cloud-synced directories unless intended
* Issue attachments
* Chat messages
* Email attachments
* Screenshots
* Public support requests

Treat all import/export files as sensitive.

## Backups

You are responsible for backing up your vault and recovery material.

OpenKeyring does not provide hosted backup, hosted recovery, or server-side vault restoration.

If you back up vault files, recovery words, exported data, or sync data to another location, that location becomes part of your security boundary.

Backups stored in cloud folders, external drives, NAS devices, Time Machine, or third-party backup services may be subject to the privacy and security practices of those systems.

## Public Website Data

The public OpenKeyring website does not require an account.

Some public website pages may use Cloudflare Web Analytics in production to understand aggregate page traffic and basic site health.

Website analytics are not connected to a hosted Oak Keyring vault account, because Oak Keyring does not provide one.

Oak Keyring's browser-local password generator does not submit generated passwords to OpenKeyring.

The public website may be served through third-party infrastructure. Those providers may process standard web request metadata such as IP address, user agent, request path, timestamps, and basic network information according to their own policies.

## Package Managers and Third-party Distribution

If you install Oak Keyring through a package manager, release mirror, build system, or third-party distribution channel, that channel may collect its own logs or metadata.

OpenKeyring does not control third-party package managers, mirrors, analytics, download logs, network infrastructure, operating system services, or build systems.

Download Oak Keyring only from trusted sources and verify release information when possible.

## GitHub and Community Interaction

If you interact with the Oak Keyring project on GitHub or other public communities, any information you post publicly may be visible to others.

Do not post secrets in:

* GitHub issues
* GitHub discussions
* Pull requests
* Comments
* Screenshots
* Logs
* Terminal recordings
* Public chat rooms
* Social media posts

Security vulnerabilities should be reported privately according to `SECURITY.md`.

For non-security bugs, feature requests, documentation issues, UI feedback, or packaging issues, use public GitHub issues only when the report does not include sensitive data.

## Sharing and Retention

OpenKeyring does not operate a hosted vault service and does not sell personal information.

Data stored in your local vault remains under your control.

Data synchronized to Google Drive remains subject to your Google Account settings and Google's terms and privacy policy.

Locally stored vault data, OAuth tokens, backups, exported files, and sync files remain until you delete them, revoke access, or remove the relevant files from your device or Google Drive.

Public GitHub issues, discussions, pull requests, comments, email messages, chat messages, and social media posts may remain in those systems unless removed under the rules of the relevant service.

## Telemetry and Analytics

The Oak Keyring app does not intentionally include telemetry, advertising tracking, analytics reporting, or automatic usage reporting.

The public website may use aggregate analytics for page traffic and site health, as described in the Public Website Data section.

If app telemetry is ever introduced in the future, it should be documented clearly and should not silently collect:

* Vault contents
* Master passwords
* Recovery words
* Vault keys
* Plaintext secrets
* OAuth tokens
* Private keys
* Sensitive notes
* User credentials

## Children’s Privacy

Oak Keyring is a developer-oriented local password manager.

It is not directed to children under 13, and OpenKeyring does not knowingly collect personal information from children under 13.

If you believe a child has provided personal information to the project, contact OpenKeyring so it can be reviewed and removed where appropriate.

## Security Reports

If you believe you have found a vulnerability, do not open a public issue.

Please follow the private reporting process in `SECURITY.md`.

Do not send real vault files, passwords, recovery words, OAuth tokens, private keys, or production credentials unless a maintainer explicitly arranges a private, minimized exchange.

## Changes to This Policy

This privacy policy may change as Oak Keyring evolves from preview software toward a stable release.

Material privacy-impacting changes should be documented in the repository, release notes, changelog, or public website where appropriate.

## Related Documents

For security reporting, see `SECURITY.md`.

For security assumptions, non-goals, and threat boundaries, see `THREAT_MODEL.md`.

For license terms and warranty disclaimers, see `LICENSE`.

## Contact

For security vulnerabilities, follow the private reporting process in `SECURITY.md`.

For general privacy-related questions about the project documentation or behavior, contact OpenKeyring by email or open a GitHub issue if the question does not include sensitive information.
