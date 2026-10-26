# Changelog

All notable changes are documented here, following [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [Semantic Versioning](https://semver.org/). A version heading does not imply a published GitHub release.

## [Unreleased]

### Fixed

- Windows HTTP test fixtures now wait for socket readiness and handle partial/transient I/O without background panics. Authenticated GET/POST and redirect/credential cleanup assertions are preserved; deterministic socket regressions and repeat-run verification were added.

## [0.3.0] - 2026-10-24

### Added

- FreeBSD/Linux platform detection, native command adapters, FreeBSD daemon detachment and public fetch downloads.
- Independent native WebView2 tabs, search, titles/favicons, keyboard shortcuts, deferred popup tabs and runtime-supported find.
- Per-profile persistent last-download directories, validated fallback locations and confirmation for every start/retry.
- Native Cancel + Handled interception on every tab; explicit opt-in blob/data Windows-cache-to-SFTP transfer.
- Shared real-byte progress monitor, toolbar active counts, completion notifications and clear-finished history.
- Directory/consent/tab/cancellation regressions and generated worker fixtures for native Linux/FreeBSD CI.

### Fixed

- FreeBSD chmod failure: private permissions now use SFTP attributes, without shell chmod.
- GNU-only publication/editor/copy flags on FreeBSD; shell control execution now explicitly uses /bin/sh for tcsh/fish accounts.
- Native browser layering behind workspace modals/windows; inactive tabs retain loaded pages.
- Query/advanced URLs omitted from permanent job metadata; fetch rejects URLs it cannot protect in process arguments.

### Limitations

- Native FreeBSD/Linux CI and connected acceptance tests have not run in the development environment. Cross-job resume remains unsupported. See QA.md.
- No tag has been pushed and no GitHub release is claimed.

## [0.2.0] - 2026-10-24

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
