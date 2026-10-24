# Update 3 verification — 0.3.0

Date: 24 October 2026. Repository: https://github.com/madarlevi/ssh-virtual-desktop. Working directory: the existing SSH-Virtual-Desktop checkout; this update preserves the application and does not clone/rewrite it. No commit, push, tag or GitHub release was created.

## Implemented

- Removed shell chmod setup and GNU-only flags from FreeBSD execution paths. SFTP creates private directories/files and metadata; the worker uses umask 077 and portable link publication. Editor saves use atomic OpenSSH SFTP rename. Symlinked directories are browsable while scans still avoid symlink traversal.
- Added reusable uname/capability detection, BSD/Linux copy adapters and native detached launchers. FreeBSD fetch handles plain public GET; aria2c/curl/wget remain available. Signed/query URLs are blocked on fetch when their tokens cannot be kept out of process arguments.
- Every native tab installs Cancel + Handled interception before website navigation. Popup requests become environment-linked child tabs; inactive tabs remain loaded. Added titles/favicons/search, native-focus keyboard shortcuts and runtime-supported find. Native Save As menus are disabled; Ctrl+S opens the remote workflow.
- Every supported download/retry requires a destination picker. Confirmed canonical destinations persist by server profile UUID; unavailable/non-directory/unreadable entries fall back to verified locations. Cancel does not modify history. Filename selection uses Content-Disposition, native suggestion, URL path, MIME and a safe fallback.
- Auth forwarding remains unchecked and per request. Optional captured general headers are also opt-in, allowing FreeBSD fetch-only public downloads. Advanced URLs are redacted from persistent job history. Blob/data operations have a separately approved, clearly labeled Windows-cache-to-SFTP fallback with cancellation/cleanup and no silent local saving.
- Shared real-byte monitoring, active counts, completion notifications, history clearing and Open Folder integration. Public retry reopens the picker; credential-bearing requests require fresh capture. Cross-job resume remains unsupported and is reported as such.
- Existing terminal colors, SFTP cache/editor/file operations, themes, profiles, credential vault, MIT ownership, repository metadata and tag release workflow are retained. Added native Linux/FreeBSD worker jobs to CI; all added actions use verified immutable SHAs.

## New files

- `src-tauri/src/remote.rs`, `download_name.rs`, `native_browser.rs`, `local_downloads.rs`.
- `src/lib/browser-tabs.ts`, `download-monitor.ts`.
- `tests/browser-tabs.test.mjs`, `remote-worker.sh`.
- `docs/freebsd.md`, this report.

Modified areas include browser/download/SFTP/SSH/settings Rust modules, browser/picker/manager/connection/desktop Svelte components, typed RPC/styles, version manifests/lockfiles, README/SECURITY/CHANGELOG/QA, CI and the bug-report template. Existing release workflow and Dependabot configuration are preserved.

## Checks actually performed

- Rust tests: **34 passed, 0 failed, 0 ignored**. Includes exact host-key pins, RPC origin isolation, shell quoting round-trips, Unicode/metacharacter filenames, platform detection/adapters, directory validation/history/migration, explicit consent, tab shortcut mapping, delta-only native visibility/modal switching, real HTTP GET/POST replay, authenticated redirect sink isolation, publication collisions, pre-start cancellation, active cancellation after observing real file growth, private request cleanup, GNU-option rejection and actual worker fixture export.
- Frontend tests: **6 passed, 0 failed**. Covers tab selection/close state, browser shortcuts, address/search/scheme validation, terminal colors and release-tag validation.
- Svelte/TypeScript and Vite production build: **0 errors, 0 warnings**.
- Rust Clippy with `--all-targets -- -D warnings`: passed. Final formatting/compilation checks are recorded below after packaging.
- Version validation accepts v0.3.0 and rejects mismatched tags. All manifests and lockfiles agree on 0.3.0.
- GitHub Actions/issue/Dependabot YAML parses; workflow actions have full commit SHAs. FreeBSD VM/download-artifact release SHAs were resolved from their official Git remotes.
- Repository candidate scan found no recognized credential/token patterns, private keys or generated installers. This is a scoped pattern check, not a security audit or a guarantee that arbitrary history contains no secrets.
- Native test harness shell syntax checked with Git for Windows sh. Real worker execution tests used that POSIX environment and its curl. **These are not native FreeBSD or Linux results.**

## Windows packaging

Both final installers and the standalone Windows x64 executable were generated successfully with:

```powershell
node node_modules/@tauri-apps/cli/tauri.js build --bundles nsis,msi --ci -- --locked --offline
```

| Artifact in artifacts/windows-x64 | Bytes | SHA-256 |
| --- | ---: | --- |
| SSH-Virtual-Desktop-0.3.0.exe | 13321216 | 746aef570a3d67eceaa529e30bd40e24e2db8549226e37b08ed4ed5a89dcf080 |
| SSH Virtual Desktop_0.3.0_x64-setup.exe | 3465122 | d218e2f10505ec2659cedc9c1f4bd29b4f4f394ed00d143ffc861b1fed398ce3 |
| SSH Virtual Desktop_0.3.0_x64_en-US.msi | 5479940 | 4dbe00d99e5b1ff5a67184a09a1356e161c8cc85be52f18a197274f4d3e0a1bc |

SHA256SUMS.txt includes these files alongside previous local versions. Outputs are ignored by Git. Installer paths were verified under src-tauri/target/release/bundle/nsis and bundle/msi; PE/installer headers and version metadata were checked. The 0.3.0 executable was launched hidden: the process remained alive, created a window titled SSH Virtual Desktop, and reported ProductVersion 0.3.0. Only that test process was closed. This is a process/window startup smoke test, not visual, connected or installer acceptance.

Final cargo fmt --check and Clippy passed. Final frontend assets matched those embedded by the build after whitespace cleanup. Git diff --check passed. No installer installation/uninstallation, digital signing or GitHub publishing was performed.

The identifier ending in .app triggers a Tauri advisory for macOS. It is retained to preserve existing Windows settings/credential/profile identity. No macOS build is claimed. Installers are unsigned.

## Not verified / remaining work

No reachable FreeBSD/Linux SSH account was supplied. No live SSH/SFTP, native FreeBSD fetch/daemon, Linux detachment, live per-server restart history, website login replay, native tab switching, popup/opener behavior, DownloadStarting ordering/Save As suppression, blob fallback, high-DPI/monitor movement or clean-machine install/uninstall acceptance is claimed. The pinned CI FreeBSD/Linux jobs are configured but have not run because the changes have not been pushed. Use the full [connected checklist](freebsd.md) before declaring Update 3 acceptance complete.

Cross-job validator-safe resume is not implemented. Complete-page/MHTML Save As, site-universal authenticated replay, unavailable request bodies, client certificates and automatic authenticated redirect continuation are unsupported. Fetch-only signed/query downloads require another backend. Direct jobs cannot survive server reboot/forced termination. Client-mediated jobs/history do not survive restart and need the source tab and connection alive. SIGKILL may leave private remote metadata/locks; inspect safely after the worker ends. Source compiles and local regression/build checks cannot replace the unverified connected tests.
