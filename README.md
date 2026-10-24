# SSH Virtual Desktop

A Windows desktop for remote **FreeBSD and Linux** servers. Browse locally in WebView2, run real SSH terminals, manage files over SFTP, and download directly on the server. FreeBSD is the primary remote compatibility target.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![GitHub release](https://img.shields.io/github/v/release/madarlevi/ssh-virtual-desktop)](https://github.com/madarlevi/ssh-virtual-desktop/releases)
[![CI](https://github.com/madarlevi/ssh-virtual-desktop/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/madarlevi/ssh-virtual-desktop/actions/workflows/ci.yml)

Badges reflect GitHub's current state; this update has not been pushed and no GitHub build or release is claimed. Version **0.3.0** is prepared locally. See [QA.md](QA.md) for checks actually performed.

## Features

- KDE-inspired local desktop with taskbar, launcher, draggable/resizable application windows and persistent appearance.
- Shared SSH connection, first-use host fingerprint confirmation, changed-key rejection, password/private-key authentication and Windows Credential Manager.
- Real SSH PTY terminals with xterm.js, Unicode, resize and Bash prompt colors.
- SFTP file manager, server-side copy/move, text editor with conflict detection, depth-limited folder caching and automatic updates.
- Isolated native WebView2 browser with independent tabs, titles, favicons, shared login sessions and keyboard shortcuts.
- Always-confirmed remote downloads, per-server destination memory, real byte progress, cancellation, retry and explicit opt-in authentication.
- Windows NSIS/MSI packaging and tag-triggered GitHub releases.

## Architecture

```mermaid
flowchart LR
  UI[Svelte desktop on Windows] --> Native[Tauri / Rust services]
  UI --> WV[Isolated local WebView2 browser]
  Native -->|SSH channels / SFTP metadata| Remote[FreeBSD / Linux server]
  WV -->|Request metadata; explicit consent for session data| Native
  Internet[HTTP/HTTPS source] -->|File contents fetched by server| Remote
```

**The client renders everything.** The remote server runs only shell commands, PTYs, SFTP and download tools. No graphical desktop, remote browser, GPU, VNC, RDP, X11/Wayland forwarding, screen streaming, WSL, Docker or permanent custom daemon is required.

The frontend lives in `src/components/`, with typed RPC in `src/lib/api.ts`. Rust separates SSH (`ssh.rs`), SFTP (`files.rs`), cache (`directory_cache.rs`), terminals (`terminal_shell.rs`), native browser observation (`browser_requests.rs`), bounded request storage/validation (`download_request.rs`), download workers (`downloads.rs`) and settings/vault (`store.rs`). Untrusted websites have no privileged RPC access.

## Screenshots

Connected-session screenshots will be added after interactive Windows/FreeBSD/Linux verification. No mock screenshots or fabricated terminal/file data are included. The original release connection screen was inspected; current verification limits are recorded in [QA.md](QA.md).

## Windows requirements

- Windows 10/11 x64 and a working network route to the remote SSH service.
- Microsoft Edge **Evergreen WebView2 Runtime**. The installers run Microsoft's runtime bootstrapper if missing; internet access may be needed.
- No Rust, Node.js, compiler or local Linux installation is needed to run an installer build.

## Remote server requirements

OpenSSH server with SFTP, `/bin/sh`, and native utilities. FreeBSD (including `/root`, `/usr/home` and symlinked homes) and Debian, Ubuntu, Fedora, Arch Linux are supported by OS-specific adapters. Home directories come from SFTP; no Linux home layout is assumed.

- **FreeBSD:** base `fetch`, `/usr/sbin/daemon`, `link`, `mv`, `mkdir`, `date`, `sleep`, `rm`, `cat`, `rmdir`, `wc` and `sh` are sufficient for public direct GET downloads. GNU coreutils and Bash are not required.
- **Linux:** aria2c, curl or GNU wget; `setsid`, `nohup` and the same base utilities. Server-side copy uses native GNU cp; FreeBSD uses BSD cp.
- **Optional on either:** aria2c for public GET; **curl for headers, authentication and POST**. Query/signed URLs need curl, aria2c or wget: native fetch has no protected URL-input-file interface, so those URLs are blocked with fetch to avoid exposing tokens in process arguments.
- Nothing is installed on your server automatically. No graphical desktop, GPU, browser, permanent daemon or public download RPC service is required.

## Install

1. Open [GitHub Releases](https://github.com/madarlevi/ssh-virtual-desktop/releases). Until a maintainer pushes a version tag and its workflow finishes, there may be no downloadable release.
2. Download the Windows x64 `*-setup.exe` (NSIS) or `.msi` installer and optionally verify `SHA256SUMS.txt` with `Get-FileHash`.
3. Run the installer. NSIS installs per user; the MSI may require elevation. Use its Start Menu shortcut to launch the app. Windows Settings provides normal uninstallation.
4. Allow WebView2 setup if the runtime is missing.

These builds are **unsigned**. Windows SmartScreen may display an unknown-publisher warning or block execution under organizational policy. Verify the repository, release and checksum; do not bypass a policy you do not control. No signing certificate is bundled or implied.

## Connect to SSH

Enter hostname/IP, port (22 by default), remote username and password or OpenSSH private-key path/passphrase. The first connection stops before authentication and displays a SHA-256 host-key fingerprint. Compare it through a trusted channel, for example the server's `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub`, before trusting it. A changed key blocks connection; there is no bypass button.

Profiles, pinned fingerprints and appearance are in `%APPDATA%\dev.sshvirtualdesktop.app\settings.json`. Selected passwords/passphrases are kept in **Windows Credential Manager**, never that settings file. Private keys remain at their original local path. Do not commit settings, profiles, keys or credentials. Reconnect is user initiated; old PTYs cannot be resumed after SSH drops. Detached download jobs are rediscovered separately.

## Browser and server-side downloads

Open Browser from the taskbar and navigate with its address bar. This is an actual Windows WebView2 child view; websites render locally. Every tab registers native DownloadStarting interception before loading a website. Supported HTTP/HTTPS downloads, including attachments without extensions, open the SFTP destination picker after normal local saving is cancelled. Alternatively use **Downloads → New download** for a direct URL.

**The picker appears every time**, including Retry. It opens in that server profile's last confirmed folder, validates access, and falls back to the remote home or a verified accessible folder. Only confirmation updates history; cancelling does not. Paths are persisted separately by profile UUID across app restarts. Tabs on one server share the setting. Choose the remote directory and filename. In **Advanced Download Options**, review the HTTP method, header count, cookie forwarding status, authentication indicator and selected backend. Use **Check request & backend**, then **Download to Server**.

Optional captured general headers are off by default, so a public GET works with FreeBSD fetch alone. Enable them when a link needs browser request headers; curl is then required. Authentication forwarding is **off by default**. The checkbox **Allow forwarding required authentication data to the remote server for this download** approves only this request. The remote server temporarily receives the approved session information and may gain access to that session: use a trusted server. Captured secret values stay native; the UI shows counts and names. Only cookies actually observed on the corresponding native request are eligible. The browser cookie database is never exported.

With no consent, sensitive headers/cookies are omitted and POST is blocked. Explicit manual headers and UTF-8 POST bodies are supported. Forwarded authentication and POST require HTTPS. If WebView2 cannot expose a complete replayable body, the request is blocked rather than changed to GET. Replaying a POST can repeat a server-side action already performed by the browser.

Public GET selects aria2c, curl, wget, then FreeBSD fetch, with bounded retries. Advanced requests use curl and private SFTP request files, never credential-bearing command arguments. Authenticated/POST redirects stop; open the target in the browser and approve a new request. Cross-origin credentials are never forwarded automatically. Expired, IP-bound, client-certificate and anti-bot sessions can fail. Blob/data downloads cannot be fetched by the remote server. When WebView2 provides a download operation, the picker offers a separately unchecked **Windows download → SFTP upload** fallback. It uses this app's temporary Windows cache, transfers all file bytes through the client, and requires the browser tab/client/SSH connection to stay open. It never runs silently. Other unavailable request bodies remain blocked.

File contents flow **website → FreeBSD/Linux server → destination**. Progress is real remote partial-file size; speed derives from byte samples and total/percentage are shown only when available. Browser interception may happen after some network buffering. Jobs survive client disconnect when server policy allows, not server reboot or processes being killed. Credential files are removed on normal worker exit/cancel; crashes/SIGKILL may require manual cleanup. Separate-job resume is not implemented; Retry starts a fresh transfer. Authenticated/header/body requests need recapture and approval.

See [FreeBSD compatibility and testing](docs/freebsd.md), [download architecture and API limitations](docs/downloads.md) and [security policy](SECURITY.md).

### Browser tabs and shortcuts

Use + or **Ctrl+T** for a new tab; **Ctrl+W** closes it. Switching tabs hides the inactive native view without reloading it. Up to 24 tabs per browser window (64 total) share a persistent local browser profile.

| Shortcut | Action |
| --- | --- |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab |
| Ctrl+1…8 / Ctrl+9 | Select tab / last tab |
| Ctrl+L | Address bar (also supports search) |
| Ctrl+R / Alt+Left / Alt+Right | Reload / back / forward |
| Ctrl+J | Downloads and history |
| Ctrl+F | Find on page (requires a compatible Evergreen runtime) |
| Ctrl+S | Remote URL download workflow |

Downloads shows the backend, real transferred bytes, known totals, sample-based speed, status and errors. Active counts appear in the browser toolbar. Completed jobs offer Open Folder; notifications work even with Downloads closed. Clear finished history removes terminal job metadata, never downloaded files. The active browser is hidden behind workspace windows, launcher and modals to avoid native airspace overlap.

## File manager and editor

Browse with breadcrumbs, the path field or back/forward/parent buttons. Create folders; select items to rename, copy, move or delete. Delete is confirmed and supports files or empty folders. Copy remains on the server using an OS adapter (`cp -R -P -p -n` on FreeBSD, `cp -a -n` on Linux); it does not relay file contents through Windows.

**Scan & cache** takes a numeric depth: 0 caches only the current folder; 1 includes immediate subfolders; maximum 16. Shared metadata stays in RAM for the SSH connection. Scans repeat 30 seconds after each pass and update open views when entries change. File operations invalidate affected listings. **Reload** fetches the current folder directly from SFTP. **Stop cache** clears it. A pass is bounded to 2,000 folders, 100,000 entries and two minutes; errors/limits show a partial-cache status. Directory symlinks are not followed during scans.

Double-click a supported UTF-8 text file to edit it locally and save over SFTP. The editor limits files to 2 MiB, rejects obvious binary content, warns about unsaved changes and checks for remote modification conflicts. Saving uses OpenSSH SFTP's atomic posix-rename extension and fails safely if unsupported. Replacement preserves Unix mode bits, but not inode identity, hard links, ACLs or extended attributes; a small compare/save race remains possible.

## Terminal

Each window opens a real SSH PTY and the remote user's default shell. Bash 4.4+ displays the folder in green, submitted commands in red, ordinary output in blue and current input in white. The header tracks the actual remote working directory. A private temporary startup file loads `~/.bashrc`, installs session-only prompt hooks and removes itself; persistent shell configuration is not edited. Bash sessions are interactive/non-login. Other shells retain their native prompt/colors. ANSI colors and full-screen terminal applications keep their behavior.

Copy with **Ctrl+Shift+C**, paste with **Ctrl+Shift+V**. File Manager's **Open Terminal Here** opens a shell in that remote directory.

## Appearance

Open Appearance in the launcher to change accent, light/dark theme, desktop/taskbar/window colors and local wallpaper. Changes apply immediately and persist locally. Wallpaper bytes stay on Windows (8 MiB maximum), never the remote server. Reset to Defaults restores the built-in appearance.

## Development

Install Node.js **24**, stable Rust with `x86_64-pc-windows-msvc`, Visual Studio 2022 or newer **Desktop development with C++** plus Windows SDK, and WebView2. Open PowerShell:

```powershell
git clone https://github.com/madarlevi/ssh-virtual-desktop.git
cd ssh-virtual-desktop
npm ci
npm run tauri dev
```

```powershell
npm run versions:check
npm run workflows:check
npm run check
npm run test:ui
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
npm run build
```

Tests use synthetic sessions and controlled HTTP fixtures. They do not need production secrets. Shell worker tests require Git for Windows' POSIX tools on Windows and skip their bodies if no shell is installed; read the verification record before claiming coverage.

## Windows build and packaging

```powershell
npm run tauri build
# Explicit installer selection:
npm run tauri build -- --bundles nsis,msi
```

The standalone application is `src-tauri/target/release/ssh-virtual-desktop.exe`. It is **not** an installer. NSIS installers are under `src-tauri/target/release/bundle/nsis/*-setup.exe`; MSI installers are under `src-tauri/target/release/bundle/msi/*.msi`. Tauri downloads/validates NSIS/WiX tools when needed. Both include the configured app icon, publisher/version metadata, Start Menu integration, uninstallation and WebView2 handling. Build outputs, generated schemas, installers, node_modules and caches are ignored by Git; lockfiles are tracked.

Regenerate third-party notices after dependency changes:

```powershell
cargo metadata --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc --manifest-path src-tauri/Cargo.toml > .build-cache/cargo-metadata.json
node scripts/licenses.mjs
```

Create `.build-cache` first if absent. Review changed notices and MPL source copies; do not remove attribution. Dependencies keep their own licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## Create a release

1. Complete the checks and live smoke tests; update CHANGELOG and review the dependency notices.
2. Synchronize all manifests and lockfiles with `npm run version:set -- 0.3.0`, then `npm run versions:check`.
3. Commit and push reviewed changes to `main`. Create a tag matching that exact version:

```powershell
git tag v0.3.0
git push origin v0.3.0
```

For example, for an application whose manifests actually say `0.1.0`, the exact commands are:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

**Do not tag this 0.3.0 checkout as v0.1.0.** The release workflow fails if the tag differs from package.json, Cargo.toml, tauri.conf.json or either lockfile. Tags such as `v0.3.0-beta.1` produce prereleases.

Pushing `v*` triggers `.github/workflows/release.yml`: checks, a Windows build, verified NSIS/MSI installers, SHA-256 checksums, then GitHub-generated release notes and actual installer uploads using `GITHUB_TOKEN`. No personal access token is needed. Publishing occurs only after the `release` environment's configured approval. A failed build never publishes placeholders. This update itself does not push a tag or publish a release.

Repository settings: enable Actions, create an environment named **release** with required reviewers and deployment-tag rules for `v*`, protect `main` with the CI check, enable private vulnerability reporting, and allow the publish job's scoped `contents: write` permission. No SSH account/production secrets are required. Organization policies may need to permit the pinned actions and GitHub Releases. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Security and limitations

Read [SECURITY.md](SECURITY.md) before forwarding authentication. Host keys are pinned; untrusted browser views have no privileged capabilities. Never weaken IPC origin/label checks or add wildcard remote capabilities. Secrets belong in the OS vault or approved temporary private server files, never logs or repository files.

Native child WebViews use Tauri's stable 2.x release with its `unstable` multi-WebView feature. Windows native airspace means only the focused browser surface is shown; dialogs/launcher hide it while pages remain alive. HTTP/HTTPS and about:blank popups become child tabs with the same native environment/opener where supported; website permission requests are denied. Native context menus are disabled because some Save As entries bypass download interception. Ctrl+S opens the remote URL workflow. Browser-generated downloads and complete-page Save As are not universally replayable. High-DPI, window layering, SSH cleanup and clean-machine installation still require real-device checks. No audit, code signing, clean-VM verification or site-universal authenticated replay is claimed. [QA.md](QA.md) separates build/unit checks from unverified connected workflows.

## Roadmap

- Complete connected Windows/FreeBSD/Linux acceptance checks and add verified screenshots.
- Add validator-safe resume and explicit origin-reviewed redirect continuation.
- Improve native download correlation, error diagnostics and site compatibility without exposing credentials.
- Sign release installers when a maintainer-provided certificate is available.

## Contributing and license

Issues and pull requests are welcome. Follow [CONTRIBUTING.md](CONTRIBUTING.md), use the provided templates, and report vulnerabilities privately under [SECURITY.md](SECURITY.md).

Copyright © 2026 **Madár Levente (madarlevi)**. Application source and original icon are provided under the [MIT License](LICENSE). Third-party dependencies and bundled notices retain their own licenses; MPL-covered dependency sources are included separately.
