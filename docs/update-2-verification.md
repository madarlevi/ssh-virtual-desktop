# Update 2 verification — 0.2.0

Date: 24 October 2026. Platform: Windows x64. This updates the existing project; the SSH desktop, terminal colors, SFTP cache/editor, browser isolation and appearance code remain in place.

## Repository state

The workspace originally had no `.git` directory. The supplied GitHub repository was public and returned no branch refs/license. A local `main` repository was initialized and `origin` set to `https://github.com/madarlevi/ssh-virtual-desktop.git`. No files were committed, no branch/tag was pushed and no release was published. Copyright and publisher use the maintainer-supplied **Madár Levente (madarlevi)** / **Madár Levente**.

## Created files

- `LICENSE`, `.gitattributes`, `.editorconfig`, `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`.
- `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `.github/dependabot.yml`, issue form/config files and PR template.
- `src-tauri/src/download_request.rs`, `browser_requests.rs`, `download_tests.rs`.
- `src/components/AdvancedDownloadOptions.svelte`, `src/downloads.css`.
- `scripts/versions.mjs`, `check-workflows.mjs`, `check-repository.mjs`, `licenses.mjs`, `tests/releases.test.mjs`.
- `docs/downloads.md`, this report, `THIRD_PARTY_NOTICES.md` and dependency attribution/source files under `third-party/`.

## Modified files

- Replaced README with public-project installation, architecture, usage, limitations, development/release/security documentation and actual GitHub links/badges.
- Expanded `.gitignore`; build outputs, installers, local credentials/config, caches and unused generated platform icons remain local.
- Updated `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `Cargo.lock`, `tauri.conf.json` to version 0.2.0; added MIT/publisher metadata and license resources; enabled NSIS and MSI.
- Updated native `main.rs`, `browser.rs`, `downloads.rs`, `security.rs` for request observation, preview, consent, validation, serialization, cleanup and stricter lint compliance.
- Updated `src/lib/api.ts`, `App.svelte`, `DownloadDestinationPicker.svelte`, `DownloadManager.svelte`, `main.ts` for advanced requests, renewed authorization and styling.
- Linked current verification from `QA.md`. Replaced the redundant old `.github/workflows/windows.yml` with CI/release workflows.

## Checks completed

| Check | Result |
|---|---|
| Rust formatting (`cargo fmt --check`) | Passed |
| Rust lint (`cargo clippy --locked --offline --all-targets -- -D warnings`) | Passed |
| Rust tests | **19 passed**, 0 failed/ignored; POSIX test bodies executed using Git for Windows |
| Frontend type/Svelte check and Vite production build | Passed, **0 errors / 0 warnings** |
| JavaScript tests (`node --test tests/*.test.mjs`) | **3 passed** |
| Version synchronization, including both lockfiles and `v0.2.0` | Passed |
| Incorrect release tag rejection | Passed |
| Workflow/issue/dependabot YAML parsing and workflow action SHA checks | Passed |
| Candidate-source secret-pattern/ignore scan | Passed; no recognized private-key/GitHub/AWS token pattern or generated installer in candidate source |
| Local README links and required MIT license | Checked |
| npm dependency audit during install/lockfile refresh | 0 reported vulnerabilities in the installed dependency set |
| Windows production build | NSIS and MSI generated successfully; final artifact sizes/hashes below |

The tests run actual shell/curl workers against controlled loopback HTTP endpoints. They verify public download bytes, destination non-overwrite, pre-launch cancellation, Cookie/Authorization forwarding, POST method/body preservation, refusal to follow an authenticated redirect to a separate listener, and request-file cleanup. Fixtures contain only synthetic credentials. Request-vault tests verify URL binding, per-request consent, stale/ambiguous capture rejection, body-unavailable rejection and header/config injection prevention. Existing host-key, IPC, terminal and cache tests also pass.

The Windows app's version resource identifies SSH Virtual Desktop 0.2.0 and publisher Madár Levente. Generated WiX metadata confirms the MSI manufacturer/version and per-machine scope. NSIS remains per user. Installers are **not signed**. The existing Tauri bundle-identifier warning concerns the `.app` suffix on macOS; the Windows application identity/config directory was preserved deliberately.

## Dependency and asset review

The locked Windows graph lists 356 third-party Rust packages; the npm lockfile contains 110 package entries including optional platform/build packages. The notice generator collected 679 license/attribution files. No GPL/AGPL-only Rust/npm dependency was identified in this graph. Unmodified MPL-2.0 sources for cssparser, cssparser-macros, dtoa-short, option-ext and selectors are included separately. MIT/Apache/BSD/ISC/Zlib/Unicode and other declared notices are retained; the application MIT license does not replace them. The original vector icon is in the existing source tree; no external stock assets/fonts were introduced. NSIS/WiX/WebView2 and external Linux download tools retain their separate licenses.

## Security changes reviewed

- Strict SSH host-key checks and local-origin/WebView-label RPC enforcement preserved; remote views receive no capabilities.
- No real account credentials or hardcoded production secret added; saved SSH secrets remain in Windows Credential Manager.
- Cookies come from the single observed request, not browser cookie storage. Captured secret values are never returned to the desktop's JavaScript.
- Authentication is opt-in, HTTPS-only and URL-bound. Backend-managed headers are rejected; general/sensitive/browser headers are handled separately. Ambiguous/expired context is not guessed, and known POST requests are never silently converted to GET.
- Sensitive request values are in mode-0600 SFTP request files inside mode-0700 UUID job directories. Curl configuration escaping is separate from shell quoting; secrets do not enter downloader arguments/logs.
- Downloader defaults are hardened against user config/.netrc surprises; aria2 RPC and automatic torrent/metalink following are disabled. Authenticated requests never follow redirects. POST is not retried automatically.
- Worker exit/cancel and reachable setup-failure cleanup remove temporary request files. Public error messages use status/exit information rather than raw downloader output containing URLs or request values.
- Job URLs may contain tokens and remain private server metadata for history/retry. Crashes/SIGKILL/network failure can prevent cleanup; see SECURITY.md.

## Not verified / known limits

No Linux SSH test account was supplied. Actual SSH/SFTP transfer, WebView2 event ordering/cookie selection/body availability, real HTTPS authenticated endpoints, interactive connected UI and site-specific downloads remain unverified. Controlled HTTP workers do not validate native browser interception or TLS certificate handling.

GitHub Actions have been prepared and YAML-validated locally, **not executed on GitHub**. Windows installers were built locally, **not installed/uninstalled in a clean VM**. Missing-WebView2 bootstrap, DPI/multi-monitor behavior and connected screenshot capture still need manual checks. No successful release badge or published release is claimed.

Authenticated redirects deliberately stop, even on the same origin; reopening the target and renewing consent is required. Separate-job/validator-safe resume is not implemented. Retry starts a fresh transfer; authenticated/header/body requests require fresh metadata. Blob/browser-generated files, uncloneable/large bodies, service-worker state, browser/IP-bound sessions, client certificates and anti-bot systems may not be delegable. No local-upload fallback is implemented.

## Before pushing or publishing

Review all candidate files with `git status`, run the documented checks, and complete the connected/clean-VM smoke tests before labeling the application production-verified. Enable GitHub Actions/private vulnerability reporting, protect `main` with CI, and configure the `release` environment with required reviewers and `v*` deployment-tag rules. Allow the publish job's scoped `contents: write` permission; no SSH/production secret is required. The release workflow uses `GITHUB_TOKEN`, fails on version mismatch, uploads only real installers/checksums and marks prerelease tags appropriately.

```powershell
git add .
git diff --cached --stat
git commit -m "Prepare 0.2.0 authenticated downloads and Windows releases"
git push -u origin main
# Only after reviewing and authorizing a release:
git tag v0.2.0
git push origin v0.2.0
```

The tag push triggers the release workflow. Do not tag this checkout v0.1.0; the README includes that exact syntax only as an example for a matching 0.1.0 checkout.

## Generated artifacts

The local, ignored `artifacts/windows-x64/` directory contains the final 0.2.0 standalone application, NSIS setup and MSI installer. `SHA256SUMS.txt` records their exact hashes; it also retains hashes for the older local builds. These outputs must not be added to Git. Sizes/hashes are recorded after the final build below.

| File | Bytes | SHA-256 |
|---|---:|---|
| SSH Virtual Desktop_0.2.0_x64-setup.exe | 3361976 | eb6ec1469ab1352702854a1065376f1966d3c946f5bda37ef46b9fd015ebd8c3 |
| SSH Virtual Desktop_0.2.0_x64_en-US.msi | 5332484 | 34c19fd7bc025a8db2e485fa5e0078efe64a3d18ccb7c7a13439f5c87e29e1b5 |
| SSH-Virtual-Desktop-0.2.0.exe | 12901888 | 598a81f53c13daf251c0c6bb3646777e6380f4f0b34d52aa123500864fd1cbc4 |
