# Security policy

## Supported versions

This is a pre-1.0 project. Security fixes target the latest version on `main` and the newest release only. Older builds should be upgraded. No audit or security certification is claimed.

## Reporting a vulnerability

Use GitHub's private **Report a vulnerability** feature on the [Security page](https://github.com/madarlevi/ssh-virtual-desktop/security/advisories/new) after the maintainer enables private vulnerability reporting. Do not post credentials, exploit details or sensitive logs in public issues. If private reporting is not yet enabled, open an issue asking for a private reporting channel without vulnerability details. No private email address has been designated. There is no promised response-time SLA.

## Security design

- SSH host keys require explicit first-use fingerprint verification; changed keys block authentication.
- Saved SSH passwords and key passphrases use Windows Credential Manager. Settings contain profile metadata, key paths, appearance, per-profile remote download directories and pinned fingerprints, never those plaintext secrets.
- Only the local `main` WebView and its expected origin may invoke native RPC. Remote browser labels have no Tauri capabilities. Native events containing privileged metadata target `main`; websites must never receive SSH, SFTP or request-vault access.
- Authenticated download forwarding is off by default and approved per download. Relevant cookies come only from the observed native HTTP request; the browser applies domain/path/Secure/HttpOnly/SameSite selection. Cookie storage is not exported. Capture can be incomplete; unavailable methods/bodies are disclosed rather than guessed.
- Authorization, cookies, non-general headers and POST bodies require consent and HTTPS. The request URL is bound to captured metadata. Authenticated requests do not follow redirects, including same-origin redirects. Reopen the target in the browser and approve a fresh request. Do not bypass this with `--location-trusted`.
- Captured values remain in native RAM and are bounded/expired. The frontend receives names, counts and metadata, never captured secret values. Manual request fields are hidden until explicitly opened.
- Approved request files are transferred using SSH/SFTP to private UUID job directories (0700), with 0600 files. Credentials never appear in downloader arguments or logs. Worker exit/cancellation removes request files. SIGKILL, server failure, disk failure or loss of SSH during setup can prevent cleanup; the server account/root can read material while a job runs. Forward only to a trusted server. Filesystem deletion cannot guarantee forensic erasure on SSD/journaled storage.
- URLs can contain bearer tokens. They remain in private server job metadata for retry/history and are visible in the local destination dialog. Do not share job directories or screenshots with tokenized URLs.
- Dynamic shell values use POSIX word serialization. Curl configuration uses separate escaping; backend-managed headers are rejected. Files publish atomically without overwriting a destination. Destructive UI operations require confirmation.
- No production credentials or repository secrets are used in PR CI. Release publishing alone has `contents: write`. GitHub Actions are pinned to reviewed full commit hashes.

## Before public distribution

Review third-party notices, test connected workflows on disposable FreeBSD and Linux accounts, and verify installer/uninstaller/WebView2 bootstrap in clean Windows VMs. Copyright/publisher identify Madár Levente (madarlevi), as supplied by the maintainer. Installers are unsigned; the project does not distribute a signing certificate.

## Update 3 execution boundaries

Private remote modes are set via SFTP, with umask 077 in portable workers. FreeBSD uses daemon(8); Linux uses nohup/setsid. Cancellation is directed by a private job marker to worker-owned children, never a discovered stale PID. Native DownloadStarting handlers cancel and mark handled before offering remote execution. Native Save As context menus are disabled; browser tabs and popup callbacks retain no privileged RPC authority.

A separately approved blob/data fallback temporarily writes complete file contents to the Windows app cache and uploads through SFTP. It is labeled client-mediated and nonpersistent; it never starts from authentication consent alone. Cancelling the picker declines the pending native operation. Cache files are removed after transfer where possible and interrupted payloads are pruned next startup. Remote crashes/SIGKILL can prevent worker cleanup; inspect stale private job files only after verifying the job ended.

Optional captured general headers are also off by default. Fetch handles public plain GET but query/signed URLs are blocked with that backend because its URL cannot be hidden from process arguments. Advanced requests require curl and existing per-download HTTPS/consent checks. See [download security and limits](docs/downloads.md).
