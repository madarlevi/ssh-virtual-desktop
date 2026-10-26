# Remote-worker compatibility fixes

Verified on 26 October 2026, application version 0.3.0. These changes update the existing project. No commit, push, tag or GitHub release was created.

## Linux: aria2 used a different output filename

The production worker supplied both `--input-file=request.urls` and command-line `--out=.svd-UUID.part`. aria2 ignores command-line `--out` for downloads supplied through an input file; output options belong to each URI in that file. This is documented in the [aria2 manual](https://aria2.github.io/manual/en/html/aria2c.html#cmdoption-o).

The original worker was reproduced with native aria2 1.37.0 on Ubuntu 26.04, using the original special-character destination. curl and wget passed. aria2 returned success and created `attachment-without-extension` (16 bytes), while `link` attempted to publish the nonexistent UUID partial file. This was an output-option error, not Unicode re-encoding or shell expansion. Cancellation also left a URL-derived data file and its `.aria2` control file because cleanup looked for the UUID name.

The protected input is now serialized as:

```text
https://example.com/attachment-without-extension
  out=.svd-00000000-0000-0000-0000-000000000000.part
```

The ASCII output name is derived from a validated UUID. The URL is preserved without decoding/re-encoding and remains outside process arguments. Newlines/control characters in URLs are rejected. The final user filename remains a separately POSIX-quoted path, preserving Unicode, spaces, quotes, leading hyphens and shell metacharacters.

Publication checks that the expected partial is a regular, non-symlink file before using `link`. The same-directory hard link remains atomic and fails if the destination appears meanwhile; existing files and destination symlinks are never overwritten. Cleanup removes the correctly named partial and `.aria2` control file. No fallback searches for or publishes an arbitrary URL-derived filename.

## FreeBSD: a fixed-delay assertion ran before fetch flushed

The unchanged harness was run on a disposable native FreeBSD 14.4-RELEASE VM with base utilities and no added packages. It failed exactly at the bare `[ -s "$SVD_DEST/$part" ]` after a two-second sleep. Before that failure, the real fetch request, loopback HTTP reachability, 16-byte payload, special-character filename, mode 0600, existing-file protection and pre-start cancellation all passed.

The slow fixture sent only 4 KiB per second. Native fetch buffers its reads/output, so a Downloading job can still have a zero-length partial after two seconds. `set -e` then exited on the bare assertion without identifying it. The original production fetch worker passes with the corrected harness, isolating the failure to the test fixture rather than fetch command syntax or output-path handling.

The harness now uses a larger response streamed in 256 KiB chunks, checks actual file growth while status remains Downloading, and fails with diagnostics if no bytes appear within 20 seconds. It waits for the listening socket using native `sockstat`/`ss`, without consuming the one-shot HTTP connection. It still requires real partial bytes, correct cancellation, cleanup, permissions and the exact payload. It additionally checks symlink collisions and leftover URL-derived aria2 files.

Local native tests used **FreeBSD 14.4**, not the CI-pinned 14.3. The 14.3 image archive could not be reached here. The original failure was reproduced on 14.4 and the final worker/harness passed twice consecutively; four additional passes preceded the final cleanup-order refinement.

## Diagnostics and regression coverage

- Failed workers return nonzero and retain the numeric backend exit status. Missing-source and publication failures have explicit messages.
- Backend stdout/stderr and publication stderr are captured privately under umask 077, classified into fixed messages, and removed. Raw output, signed URLs, headers, cookies and request bodies are never printed or retained as diagnostic history. Unterminated stderr lines are handled too.
- Request/raw files are removed before Completed, Failed or Cancelled becomes visible. The EXIT trap remains an additional cleanup path. Forced termination such as SIGKILL can still interrupt cleanup, as with existing request files.
- Failure output names the test phase and prints sanitized reasons, expected paths and actual filesystem metadata. It never dumps request files or raw backend output.
- The Rust fixture exporter now exports the actual request serializers and fetch command as well as the worker. Native tests no longer duplicate aria2/fetch request construction.
- Six new Rust regressions cover per-URI naming/input injection, fetch quoting, a successful backend with missing output, secret-bearing failure output, a destination created during publication, and cleanup at the exact atomic terminal-state boundary. Existing authenticated GET/POST/redirect assertions remain intact.

## Final verification

| Check | Result |
| --- | --- |
| Windows Rust suite | 45 passed, 0 failed, 0 ignored |
| Rust formatting | Passed |
| Clippy, all targets, warnings denied | Passed |
| Native Linux curl, wget, aria2c | All passed, including special filenames, payload, 0600 mode, file/symlink collisions, detached progress/cancellation and cleanup |
| Native FreeBSD 14.4 fetch | Final worker passed twice consecutively with 491,520 real partial bytes observed before cancellation |
| Native fetch connection-refused fixture | Nonzero worker/backend exits, safe classification and private-file cleanup passed |
| Frontend tests | 6 passed |
| Svelte/TypeScript | 0 errors, 0 warnings |
| Vite production build | Passed |
| GitHub workflow YAML / pinned actions | Passed; existing fixture artifact wildcard includes the added templates |
| Version synchronization | 0.3.0 / v0.3.0 passed |
| Windows release executable, NSIS and MSI | Built successfully |

The native VM used a verified official FreeBSD image, a temporary synthetic SSH key and a pinned host key; it was shut down after testing. No user server credentials were used. Tests run the actual generated remote worker with a controlled HTTP endpoint; they do not replace connected browser/SFTP/desktop acceptance tests. GitHub Actions has not been rerun on these unpushed changes. Clean-machine installer installation/uninstallation and signing were not tested.

Commands used from the repository root:

```powershell
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --offline --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --offline --manifest-path src-tauri/Cargo.toml
node scripts/frontend.mjs build
node --test tests/*.test.mjs
node scripts/check-workflows.mjs
node scripts/versions.mjs v0.3.0
node scripts/check-repository.mjs
node node_modules/@tauri-apps/cli/tauri.js build --bundles 'nsis,msi' --ci -- --locked --offline
```

On each native Unix test host, after copying the complete generated fixture directory:

```sh
/bin/sh tests/remote-worker.sh src-tauri/target/compatibility-fixtures
```

## Local Windows artifacts

Final copies are in the ignored `artifacts/windows-x64/remote-worker-fix/` directory with `SHA256SUMS.txt`. They are unsigned local builds, not a published release.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| SSH-Virtual-Desktop-0.3.0.exe | 13326848 | 52f7732589f6918db6d4e277ebe51f44c1f9370cfdbd2b8bf5ab290fd6f3865f |
| SSH Virtual Desktop_0.3.0_x64-setup.exe | 3468755 | 36bdb35ebf12b172306877d6e71012d68d5084907262905f257fd8c02034f858 |
| SSH Virtual Desktop_0.3.0_x64_en-US.msi | 5484036 | c19ead3b9f0aacb591d58650485d82b74614dd7380ec235060beb8e3c9d621da |
