# Verification record

**Current Update 2 (0.2.0):** see [the complete verification report](docs/update-2-verification.md). The records below preserve the earlier 0.1.x checks and limitations.

Date: 24 October 2026. Primary environment: Windows x64, Rust 1.94.1, Node 24.13.1, MSVC Visual Studio Community 2026, WebView2 155.0.4283.62.

## Completed checks

- `node scripts/frontend.mjs build`: passed. Svelte/TypeScript: **0 errors, 0 warnings**. Vite production assets generated.
- `cargo check --offline --manifest-path src-tauri/Cargo.toml`: passed against the actual resolved dependencies.
- `cargo test --locked --offline --manifest-path src-tauri/Cargo.toml`: **13 passed, 0 failed, 0 ignored** for version 0.1.1.
  - unknown/changed host keys rejected; exact fingerprint pin accepted;
  - remote browser labels denied privileged authority, including local-looking origins;
  - path, filename and URL validation;
  - adversarial shell arguments round-trip through a real POSIX shell without expansion;
  - worker script quotes untrusted values and publishes without overwrite;
  - a local HTTP server supplies real bytes to the actual shell/curl worker, which saves a Unicode/apostrophe/metacharacter filename;
  - existing destination remains unchanged, and cancellation before launch creates no final file.
  - scan depth and path boundaries, cache scoping, invalidation of parents/moved subtrees, stop cleanup, and change detection without repeated notifications;
  - real Bash commands emit prompt, command and working-directory markers while preserving an existing prompt hook; terminal startup quotes the working directory safely.
- `node --test tests/terminal-colors.test.mjs`: **2 passed**, covering wrapped command color ranges, prompt exclusion, working-directory reporting, alternate-screen protection and resource cleanup.
- Tauri Windows icon generation: passed.
- `tauri info`: detects WebView2, MSVC and the x64 Rust toolchain.

The worker test uses Git for Windows' POSIX tools on this machine. It verifies worker logic, **not an SSH connection to a Linux server**. Shell-specific tests skip their bodies if no POSIX shell is available; the shell was available for this run.

## Version 0.1.1 update

`node node_modules/@tauri-apps/cli/tauri.js build --bundles nsis --ci -- --locked --offline`: **passed** with the terminal and directory-cache changes. The production frontend check reported **0 errors and 0 warnings**; the optimized native application and NSIS installer were generated successfully.

- `artifacts/windows-x64/SSH-Virtual-Desktop-0.1.1.exe`: 12,686,848 bytes.
- `artifacts/windows-x64/SSH Virtual Desktop_0.1.1_x64-setup.exe`: 3,146,127 bytes.
- Installer SHA-256: `4b52a86a361381b5be5bd47099ef4a80da10bd5e3ad95edd5263a4f0d7af3c33`.
- `SHA256SUMS.txt` contains hashes for the new files and the retained original 0.1.0 artifacts.

The complete terminal launcher was exercised with real commands in the installed Git Bash, including a working directory containing an apostrophe and shell metacharacters, preservation of a prompt hook, and automatic removal of its temporary startup file. The command-color mapping was unit tested. The scan cache's depth, scope, invalidation and change detection were unit tested. Native connected terminal rendering, interactive editor compatibility, SFTP scan throughput and refresh against externally changed Linux directories remain unverified without a test server. The new installer has been built, but has not been installed/uninstalled in a clean VM.

Manual update checks after connecting: type a command and verify white input, press Enter and verify red submitted text and blue ordinary output; `cd` to another directory and verify the green prompt/header. Try wrapped input and `vim`. In File Manager, scan depth 0 then 2, browse the cached subfolders, modify a file externally, wait for a refresh pass and check the open listing. Use Reload for an immediate server read, perform rename/move/create operations, then stop the cache and verify live SFTP listings resume. Check partial-scan messages for unreadable folders and bounded trees.

## Original 0.1.0 release packaging

`node node_modules/@tauri-apps/cli/tauri.js build --bundles nsis --ci -- --locked --offline`: **passed**. Optimized Windows x64 executable and NSIS installer were generated with the locked dependency set. NSIS tools were downloaded and hash-validated by Tauri.

Prepared deliverables:

- `artifacts/windows-x64/SSH-Virtual-Desktop.exe`: 12,577,280 bytes.
- `artifacts/windows-x64/SSH Virtual Desktop_0.1.0_x64-setup.exe`: 3,117,972 bytes.
- `artifacts/windows-x64/SHA256SUMS.txt`: SHA-256 hashes for both files.

Installer SHA-256: `9dad10ed4bf59b358bbf1bd9aa1669adcf9b439e4c67f574c4a7f6cb5516b202`.

The release executable was launched on Windows. Its real native window rendered the polished connection screen with server, port 22, username, authentication methods, password, profile controls and credential-storage option. Initialization RPC succeeded. The window was visually inspected using the computer-use skill. No credentials were entered and no server was contacted. The app remains open for the user. Installation/uninstallation and absent-runtime bootstrap have **not** been exercised in a clean VM. The installer is unsigned.

## Linux acceptance workflow

No reachable Linux test server or credentials were supplied. The following end-to-end steps remain unverified. Their code paths are implemented; compilation and unit tests are not substitutes for these checks.

| # | Acceptance step | Verification |
|---|---|---|
| 1 | Launch the Windows application | Passed: release executable launched and connection screen inspected |
| 2 | Enter Linux SSH credentials | Needs a test server/account |
| 3 | Connect successfully | Needs a test server/account |
| 4 | See virtual desktop after authentication | Needs live SSH authentication |
| 5 | Open terminal | Needs live SSH authentication |
| 6 | Run a real Linux command | Needs a Linux server |
| 7 | Open file manager | Needs live SSH/SFTP |
| 8 | Browse remote directories | Needs live SSH/SFTP |
| 9 | Create a remote folder | Needs live SSH/SFTP |
| 10 | Open embedded browser | Needs desktop session |
| 11 | Navigate a public website | Needs native browser inspection |
| 12 | Click direct download | Needs native WebView2 inspection |
| 13 | See destination picker | Needs native download interception |
| 14 | Select remote directory | Needs live SFTP |
| 15 | Confirm download | Needs live SSH/SFTP |
| 16 | Observe server-side progress | Worker locally tested; Linux integration pending |
| 17 | Finish without relaying body through Windows | Architecture implemented; Linux integration pending |
| 18 | Open destination folder | Needs live SFTP |
| 19 | See downloaded file | Needs live SFTP |
| 20 | Edit and save remote text | Needs live SFTP |
| 21 | Change desktop accent | UI/type check passed; native interaction pending |
| 22 | Change wallpaper | Native dialog and persistence need interaction testing |
| 23 | Restart application | Full connected/appearance restart sequence not tested |
| 24 | Verify appearance persistence | Needs appearance/restart test |
| 25 | Build release installer | Passed: Windows x64 NSIS installer generated and file verified |

## Deliberate MVP limits

- Browser child WebViews use Tauri 2's `unstable` feature. Only the focused browser surface is visible; dialogs and the launcher hide it to respect Windows native airspace.
- Direct public HTTP/HTTPS downloads are the supported delegation case. No browser credentials, cookies, request bodies or authorization headers are exported. Popups and site permission requests are denied.
- aria2 uses one sequential stream and no preallocation so partial-file size is a meaningful byte counter. Download total and percentage remain unknown until completion; there is no fabricated progress.
- Detached jobs require `setsid`/`nohup` and ordinary GNU-compatible tools. Persistence covers client disconnect, not server reboot or policies that terminate user processes. Stale workers are flagged after 60 seconds without a heartbeat. Manual Refresh rereads cached terminal jobs. The newest 500 jobs are displayed.
- Filesystem publication requires hard links and `ln -T`. Empty-folder deletion only; copy uses server-side GNU-compatible `cp`. No file content is fetched to implement copy.
- Text editing is UTF-8, up to 2 MiB. Revision checks have a small concurrent-write race; save preserves mode, not inode identity/ACLs/xattrs.
- Reconnection is user initiated. Existing terminal processes cannot be resumed through a new SSH channel; detached download metadata is rediscovered.
- Release installers are unsigned. Installation/uninstallation and missing-WebView2 bootstrap must be tested in a clean Windows VM before distribution.

## Repeating the live tests

Use a disposable Linux account with SSH/SFTP and a download tool. Verify the displayed first-use fingerprint against the server's host public key, then connect. In a terminal run `printf 'SSH connection works\n'; uname -a`. Create a test folder, create/edit a UTF-8 file, and download a public file of known SHA-256 into the folder. Compare its remote checksum and observe the Windows client's network activity to confirm it never receives the file body. Cancel a larger transfer, retry it, disconnect while another is active, and reconnect to rediscover its status. Test a changed host key separately: authentication must not proceed. Finally verify window movement, minimization, DPI scaling, browser history, dialog layering and persisted appearance after restart.
