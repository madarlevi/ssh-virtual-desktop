# Changelog

All notable changes are documented here, following [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [Semantic Versioning](https://semver.org/). A version heading does not imply a published GitHub release.

## [Unreleased]

### Added

- Public repository documentation, MIT license template, contribution/security policies and issue/PR templates.
- CI and protected-environment Windows release workflows with pinned actions, NSIS/MSI artifacts, checksums, tag validation and generated release notes.
- Native WebView2 request observation, bounded request vault, per-download consent and advanced request summaries/manual GET/POST options.
- Private request configuration transfer, authentication-aware curl selection, credential cleanup and authenticated redirect blocking.

### Changed

- Application version prepared as 0.2.0; no release has been pushed or published by this update.
- Public GET retries are bounded; POST is not automatically retried. Requests with removed credentials require a new approval.

## [0.1.1] - 2026-10-24

### Added

- Bash terminal colors, actual remote working-directory display, and refreshed terminal styling.
- Depth-limited SFTP folder metadata scans, shared in-memory cache, automatic updates and explicit Reload.

## [0.1.0] - 2026-10-24

### Added

- Initial Windows Tauri desktop, strict SSH host verification, OS credential storage, real PTY terminals and SFTP file management/editor.
- Isolated native WebView2 browser, detached server-side public downloads and appearance persistence.

[Unreleased]: https://github.com/madarlevi/ssh-virtual-desktop/commits/main
