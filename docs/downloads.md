# Remote download architecture (0.3.0)

## Native interception and isolation

Every native child tab begins at `about:blank`. Rust installs `WebResourceResponseReceived`, `DownloadStarting`, popup, keyboard and favicon handlers before navigating to a site. DownloadStarting is the primary trigger, regardless of URL extension or click mechanism. HTTP/HTTPS events set **Cancel=true and Handled=true**, suppressing local saving and the normal Save As UI, then offer the remote picker. The separate response observer provides request context where available. No response bodies or browser cookie database are exported.

WebView2 can raise downloads for Content-Disposition attachments, redirects, JavaScript-initiated requests, download links and newly created tabs. That does not mean every site's content is replayable. Missing/ambiguous metadata, non-cloneable POST streams, blob objects and client-bound sessions have explicit limits. Interception occurs after some browser network buffering; it cannot prevent every initial response byte reaching Windows.

Tauri's download wrapper only cancels a rejected download in the resolved Wry version. This integration uses WebView2 directly so it can also set Handled. Native context menus are disabled to avoid Save Page/Image As paths that may bypass DownloadStarting; Ctrl+S opens the remote URL workflow. Complete-page/MHTML saving is not implemented. Other WebView2 Save As APIs are not advertised as universally intercepted.

Untrusted browser labels never receive desktop capabilities. RPC verifies both the `main` label and its expected local origin. Events target main only. HTTP/HTTPS/about:blank and blob popups use deferred NewWindowRequested, an environment shared with the opener, and SetNewWindow after hooks are installed. Closing a tab releases handlers and pending offers. Site permissions are denied. Tabs share a local persistent browser profile; hiding/switching does not reload a page. Find/favicons depend on native runtime capabilities.

## Captured request context and consent

Candidate attachment/application responses enter a bounded native RAM vault: 32 recent requests for five seconds; 32 offered requests for ten minutes, purged every 30 seconds. Matching is per tab and exact final URL, only when one fresh context matches. Download interception waits up to one second for the response callback. Ambiguous, stale or absent contexts require explicit direct-GET confirmation or manual reconstruction. Redirect chains are not reconstructed.

Cookie headers come only from the corresponding observed request, where WebView2 applied domain/path/Secure/HttpOnly/SameSite rules. Missing headers cannot be inferred. Only cloneable, seekable, completely readable request streams up to 1 MiB can supply POST bodies. A browser POST already happened; replay may repeat an action. Incomplete bodies are blocked, never converted to GET.

The UI shows method, names/counts, cookie count and body size, not captured values. Both optional general-header forwarding and authentication forwarding start unchecked. Without opting into optional headers, a captured public GET uses no browser headers, so FreeBSD fetch alone can handle it. General headers are Accept, Accept-Language, User-Agent, Content-Type and Origin; Referer, Cookie, Authorization and custom headers require explicit authentication consent. Approval permits only this exact captured URL/request. POST bodies require consent and HTTPS. Manual request fields follow the same sensitivity rules.

Host, transfer/connection headers, compression negotiation, conditional/range headers, proxy authentication and Sec-* headers are not forwarded. Curl generates Content-Length and negotiates identity encoding, with decoding support if a server nevertheless compresses. Header names/values are validated and bounded to 64 headers/64 KiB. Shell quoting and curl-config escaping are separate serializers. Credentials enter mode-0600 SFTP configuration/body files inside mode-0700 UUID directories, not process arguments or logs. Query URLs and requests with headers/body/authentication are omitted from permanent job URLs. Curl configuration ignores user curlrc. Secrets are removed on normal completion/failure/cancel; crashes/SIGKILL can require manual cleanup.

## FreeBSD and Linux execution

Login runs `uname -s` and capability detection through explicit `/bin/sh`, even when the account uses tcsh/fish. SFTP resolves home directories and filesystem metadata. Permission creation uses SFTP attributes and setstat, eliminating the incompatible `chmod 700 -- PATH` command entirely. Native BSD/GNU cp adapters are used only for server-side copy. Editor replacement uses the OpenSSH SFTP posix-rename extension.

| Backend | Selection / limits |
| --- | --- |
| aria2c | First public GET choice; protected input file, bounded same-job retries; RPC/torrent/metalink handling disabled |
| curl | Preferred fallback and required for headers, cookies, authorization or POST; protected config/body files |
| GNU wget | Public GET through protected URL input file; bounded retries |
| FreeBSD fetch | Native public GET with no extra headers/body; netrc disabled; rejects query/signed URLs because its URL is necessarily visible in process arguments |

FreeBSD jobs detach with `/usr/sbin/daemon -f /bin/sh SCRIPT`; Linux uses `nohup setsid /bin/sh SCRIPT`. The script does not need executable permission. No Bash, systemd, `/proc`, public RPC listener or permanent custom server daemon is used. Linux-specific flags do not enter FreeBSD base commands. `date +%s` is supported by both target systems; all file sizes come through SFTP rather than GNU/BSD stat parsing.

Each worker owns its downloader/watcher children and private cancel marker. No discovered/stale PID is used as authority to kill a process. Status, heartbeat and exit result live in private job files. The parent checks downloader exit status and curl HTTP result before publication; growth alone never means success. `link SOURCE TARGET` publishes atomically on the same filesystem and refuses collisions. Filesystems without hard-link support report failure. A private target lock also prevents two app jobs reserving the same name. Cancellation removes the partial file and credential files.

Jobs survive SSH/client disconnection where server policies allow, but not server reboots or external termination. Stale heartbeat detection reports uncertainty; clearing history only removes metadata with a persisted terminal worker status. Inspect stale locks/request material only after confirming the worker ended.

## Destination memory, progress and retry

Every start requires explicit confirmation. Picker history is keyed by stable saved profile UUID in Windows settings. Before displaying a remembered path, SFTP canonicalizes it, verifies directory metadata and attempts listing. Fallback order is remote home, /tmp, /var/tmp, /. Nothing missing is automatically created. Confirmation of a successfully submitted valid request updates history; cancelling never does. Folder creation requires its own UI action. Existing destination files are never silently overwritten.

Progress is actual SFTP partial-file bytes, sampled every two seconds for active transfers, with idle history refresh every 30 seconds. Speed derives from samples; unknown totals stay indeterminate. Native response length can differ on replay; final size replaces it on completion. The toolbar count and completion notifications use a shared monitor independent of the Downloads window.

Retry always reopens the picker. Plain public requests can be retried with a new job. Authenticated/header/POST/signed requests require reopening their source and capturing/approving fresh metadata. **Resume across failed/cancelled jobs is not implemented**; capability summaries report resume=false. Aria2/wget/curl may retry within their existing worker; this is not advertised as cross-job resume. Authenticated or POST redirects stop even on the same origin; open the redirect target in the browser and approve a new request. No credential-bearing cross-origin continuation is automatic.

## Explicit client-mediated fallback

For blob/data download events that expose an operation, WebView2 is deferred with local saving cancelled. The picker offers a separately unchecked **Allow downloading this file through Windows and uploading it via SFTP** checkbox. Only that approval plus Download to Server releases the operation into a UUID app-cache folder, not Windows Downloads or Save As. Cancellation/closing the picker cancels the pending browser event.

After WebView2 reports Completed, Rust uploads the file through the existing SFTP connection in chunks, checks uploaded size and uses non-overwriting SFTP rename. Transfers are labeled WebView2 → SFTP and nonpersistent; byte progress reflects the browser phase followed by the SFTP phase. Closing the source tab, disconnecting SSH or exiting can interrupt it. Local cache/remote partial cleanup is attempted on success/failure/cancel; stale fixed payloads in UUID cache folders are removed at next startup. This fallback's history is in memory and does not survive restart. It is currently offered only for exposed blob/data operations, not every unsupported HTTP/POST download.

## References and verification

- [WebView2 DownloadStartingEventArgs](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2downloadstartingeventargs)
- [WebView2 NewWindowRequested / deferral / SetNewWindow](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2newwindowrequestedeventargs)
- [WebView2 request IStream](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2webresourcerequest)
- [FreeBSD fetch(1)](https://man.freebsd.org/cgi/man.cgi?query=fetch&sektion=1)
- [FreeBSD daemon(8)](https://man.freebsd.org/cgi/man.cgi?query=daemon&sektion=8)
- [FreeBSD link/ln(1)](https://man.freebsd.org/cgi/man.cgi?query=ln&sektion=1)
- [curl configuration](https://curl.se/docs/manpage.html#-K)

Controlled HTTP fixtures use synthetic credentials to execute the actual shell/curl worker: GET, POST, redirect isolation, collisions, pre-start and active cancellation, cleanup and portable-option rejection. Generated workers are uploaded by CI for native Linux and FreeBSD base/fetch tests. Those jobs have not run locally. See [FreeBSD test instructions](freebsd.md), [Update 3 verification](update-3-verification.md) and [QA](../QA.md) for the distinction between checks performed and unverified live workflows.
