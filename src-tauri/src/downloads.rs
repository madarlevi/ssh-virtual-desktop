use crate::{
    download_request::{self, Input, RequestVault, Resolved},
    security::{self, quote},
    ssh::{self, Connection},
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
#[cfg(test)]
#[path = "download_test_http.rs"]
mod download_test_http;
#[cfg(test)]
#[path = "download_tests.rs"]
mod integration_tests;
#[cfg(test)]
#[path = "download_worker_tests.rs"]
mod worker_regressions;

#[derive(Deserialize)]
pub struct Request {
    #[serde(flatten)]
    pub request: Input,
    pub directory: String,
    pub filename: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub url: String,
    pub directory: String,
    pub filename: String,
    pub backend: String,
    pub started: u64,
    pub status: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub error: Option<String>,
    pub persistent: bool,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub authenticated: bool,
    #[serde(default)]
    pub header_count: usize,
}
fn default_method() -> String {
    "GET".into()
}
fn root(c: &Connection) -> String {
    format!(
        "{}/.local/state/ssh-virtual-desktop/jobs",
        c.home.trim_end_matches('/')
    )
}
async fn write(c: &Connection, path: &str, data: &[u8]) -> Result<()> {
    let mut f = c
        .sftp
        .open_with_flags_and_attributes(
            path,
            russh_sftp::protocol::OpenFlags::CREATE
                | russh_sftp::protocol::OpenFlags::TRUNCATE
                | russh_sftp::protocol::OpenFlags::WRITE,
            russh_sftp::protocol::FileAttributes {
                permissions: Some(0o600),
                ..Default::default()
            },
        )
        .await?;
    f.write_all(data).await?;
    f.shutdown().await?;
    Ok(())
}
async fn small(c: &Connection, path: &str) -> Result<String> {
    if c.sftp.metadata(path).await?.size.unwrap_or(u64::MAX) > 16384 {
        bail!("Job metadata exceeds limit")
    }
    Ok(String::from_utf8(c.sftp.read(path).await?)?)
}
pub async fn backends(c: &Connection) -> Result<Vec<String>> {
    Ok(c.remote.tools.clone())
}
pub async fn destination(c: &Connection, previous: Option<&str>) -> Result<String> {
    crate::remote::choose_directory(&c.home, previous, |candidate| async move {
        let path = c.sftp.canonicalize(candidate).await.ok()?;
        if c.sftp.metadata(&path).await.is_ok_and(|m| m.is_dir())
            && c.sftp.read_dir(&path).await.is_ok()
        {
            Some(path)
        } else {
            None
        }
    })
    .await
}
pub async fn clear_history(c: &Connection) -> Result<()> {
    for job in list(c, true).await? {
        if !matches!(job.status.as_str(), "Completed" | "Cancelled" | "Failed") {
            continue;
        }
        // A stale-heartbeat failure might still have a live worker; do not erase it.
        let dir = format!("{}/{}", root(c), job.id);
        let status = small(c, &format!("{dir}/status")).await.unwrap_or_default();
        if !matches!(status.trim(), "Completed" | "Cancelled" | "Failed") {
            continue;
        }
        for entry in c.sftp.read_dir(&dir).await? {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            security::filename(&name)?;
            if entry.metadata().is_dir() {
                bail!("Unexpected nested directory in job metadata; nothing outside the job was removed")
            };
            c.sftp.remove_file(format!("{dir}/{name}")).await?;
        }
        c.sftp.remove_dir(&dir).await?;
    }
    c.completed_jobs.lock().await.clear();
    Ok(())
}
pub fn select_backend(tools: &[String], advanced: bool) -> Result<&'static str> {
    if advanced {
        if tools.iter().any(|t| t == "curl") {
            return Ok("curl");
        };
        bail!("This request needs curl on the server (headers, authentication or POST). No packages were installed.")
    }
    ["aria2c", "curl", "wget", "fetch"]
        .into_iter()
        .find(|s| tools.iter().any(|t| t == s))
        .context("No downloader found: FreeBSD fetch, aria2c, curl or wget is required. No packages were installed.")
}
async fn private_directories(c: &Connection, jobdir: &str) -> Result<()> {
    for suffix in [
        ".local",
        ".local/state",
        ".local/state/ssh-virtual-desktop",
        ".local/state/ssh-virtual-desktop/jobs",
    ] {
        let dir = format!("{}/{suffix}", c.home.trim_end_matches('/'));
        let new = !c.sftp.try_exists(&dir).await?;
        if new {
            c.sftp.create_dir(&dir).await?;
        }
        let attrs = c.sftp.symlink_metadata(&dir).await?;
        if !attrs.is_dir() || attrs.is_symlink() {
            bail!(
                "The private download state path must contain real directories, not symbolic links"
            )
        }
        if new || suffix.contains("ssh-virtual-desktop") {
            c.sftp
                .set_metadata(
                    &dir,
                    russh_sftp::protocol::FileAttributes {
                        permissions: Some(0o700),
                        ..Default::default()
                    },
                )
                .await?;
        }
    }
    c.sftp.create_dir(jobdir).await?;
    c.sftp
        .set_metadata(
            jobdir,
            russh_sftp::protocol::FileAttributes {
                permissions: Some(0o700),
                ..Default::default()
            },
        )
        .await?;
    Ok(())
}
pub async fn start(c: &Connection, r: Request, vault: &RequestVault) -> Result<Job> {
    if !r.request.confirmed {
        bail!("Choose a destination and confirm Download to Server before starting any download")
    }
    let resolved = r.request.resolve(vault)?;
    let url = resolved.url.clone();
    security::path(&r.directory)?;
    security::filename(&r.filename)?;
    let directory = c.sftp.canonicalize(&r.directory).await?;
    let destination = security::join(&directory, &r.filename)?;
    if c.sftp.try_exists(&destination).await? {
        bail!("A file with that name already exists. Choose another name.")
    }
    if !c.sftp.metadata(&directory).await?.is_dir() {
        bail!("Destination must be a directory")
    }
    let capabilities = backends(c).await?;
    let has = |s: &str| capabilities.iter().any(|l| l == s);
    let backend = select_backend(
        &capabilities,
        resolved.method != "GET" || !resolved.headers.is_empty(),
    )?;
    for cmd in [
        "sh", "wc", "link", "mv", "mkdir", "date", "sleep", "rm", "cat", "rmdir",
    ] {
        if !has(cmd) {
            bail!("Remote background jobs require the '{cmd}' utility. No packages were installed.")
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let root = root(c);
    let jobdir = format!("{root}/{id}");
    // SFTP permissions avoid chmod's incompatible argument parsing on FreeBSD.
    let launch = c.remote.launch(&format!("{jobdir}/run.sh"))?;
    if backend == "fetch" && url::Url::parse(&url)?.query().is_some() {
        bail!("FreeBSD fetch exposes its URL in process arguments. Use curl, aria2c or wget for signed/query URLs.")
    }
    private_directories(c, &jobdir).await?;
    let job = Job {
        id: id.clone(),
        url: if resolved.authenticated
            || resolved.method != "GET"
            || !resolved.headers.is_empty()
            || url::Url::parse(&url)?.query().is_some()
        {
            String::new()
        } else {
            url
        },
        directory: directory.clone(),
        filename: r.filename.clone(),
        backend: backend.into(),
        started: ssh::exec(c, "date +%s").await?.trim().parse()?,
        status: "Starting".into(),
        downloaded: 0,
        total: resolved.total,
        error: None,
        persistent: true,
        method: resolved.method.clone(),
        authenticated: resolved.authenticated,
        header_count: resolved.headers.len(),
    };
    let script = worker(&job, &jobdir)?;
    let setup = async {
        write(c, &format!("{jobdir}/job.json"), &serde_json::to_vec(&job)?).await?;
        write(c, &format!("{jobdir}/run.sh"), script.as_bytes()).await?;
        if backend == "curl" {
            let config = curl_config(&job, &jobdir, &resolved)?;
            write(c, &format!("{jobdir}/request.conf"), config.as_bytes()).await?;
        } else {
            let input = url_input(backend, &job.id, &resolved.url)?;
            write(c, &format!("{jobdir}/request.urls"), input.as_bytes()).await?;
        }
        if backend == "fetch" {
            write(
                c,
                &format!("{jobdir}/fetch.sh"),
                fetch_script(&job, &resolved.url)?.as_bytes(),
            )
            .await?;
        }
        if let Some(body) = &resolved.body {
            write(c, &format!("{jobdir}/request.body"), body).await?;
        }
        ssh::exec(c, &launch).await?;
        Ok::<(), anyhow::Error>(())
    }
    .await;
    if let Err(error) = setup {
        for file in ["request.conf", "request.urls", "request.body", "fetch.sh"] {
            let _ = c.sftp.remove_file(format!("{jobdir}/{file}")).await;
        }
        return Err(error.context("Unable to prepare remote download; temporary request files were removed where reachable"));
    }
    if let Some(id) = &r.request.offer_id {
        vault.discard(id);
    }
    Ok(job)
}
fn partial_name(id: &str) -> Result<String> {
    // The backend input format has its own syntax: never put user filenames in it.
    let id = uuid::Uuid::parse_str(id)?;
    Ok(format!(".svd-{id}.part"))
}
fn url_input(backend: &str, id: &str, url: &str) -> Result<zeroize::Zeroizing<String>> {
    security::http_url(url)?;
    let mut input = zeroize::Zeroizing::new(format!("{url}\n"));
    if backend == "aria2c" {
        // aria2 ignores command-line --out with --input-file. Its unquoted,
        // per-URI option must name the same ASCII partial that publication uses.
        input.push_str(&format!("  out={}\n", partial_name(id)?));
    }
    Ok(input)
}
fn fetch_script(job: &Job, url: &str) -> Result<zeroize::Zeroizing<String>> {
    security::http_url(url)?;
    Ok(zeroize::Zeroizing::new(format!(
        "#!/bin/sh\nexec fetch -q -T 30 -N /dev/null -o {} {}\n",
        quote(&security::join(&job.directory, &partial_name(&job.id)?)?)?,
        quote(url)?
    )))
}
fn curl_config(job: &Job, jobdir: &str, request: &Resolved) -> Result<zeroize::Zeroizing<String>> {
    use download_request::config_word as q;
    let partial = security::join(&job.directory, &partial_name(&job.id)?)?;
    let mut config=zeroize::Zeroizing::new(format!("url = {}\noutput = {}\nrequest = {}\nheader = \"Accept-Encoding: identity\"\nconnect-timeout = 30\nmax-redirs = 10\nproto = \"=http,https\"\nproto-redir = \"=http,https\"\nsilent\nwrite-out = \"%{{http_code}}\"\n",q(&request.url)?,q(&partial)?,q(&request.method)?));
    // Request identity encoding, but decode if a server nevertheless encodes its response.
    config.push_str("compressed\n");
    if !request.authenticated && request.method == "GET" {
        config.push_str("location\nretry = 2\nretry-delay = 1\n");
    }
    for h in &request.headers {
        config.push_str(&format!(
            "header = {}\n",
            q(&format!("{}: {}", h.name, h.value))?
        ));
    }
    if request.body.is_some() {
        config.push_str(&format!(
            "data-binary = {}\n",
            q(&format!("@{jobdir}/request.body"))?
        ));
    }
    Ok(config)
}
fn worker(job: &Job, jobdir: &str) -> Result<String> {
    let partial_name = partial_name(&job.id)?;
    let partial = security::join(&job.directory, &partial_name)?;
    let target = security::join(&job.directory, &job.filename)?;
    let download = match job.backend.as_str() {
        "aria2c" => format!("aria2c --no-conf=true --no-netrc=true --enable-rpc=false --follow-torrent=false --follow-metalink=false --split=1 --max-connection-per-server=1 --enable-mmap=false --allow-overwrite=false --auto-file-renaming=false --file-allocation=none --check-certificate=true --max-tries=3 --retry-wait=1 --summary-interval=0 --show-console-readout=false --console-log-level=error --download-result=hide --dir={} --input-file=\"$job/request.urls\"", quote(&job.directory)?),
        "wget" => format!("wget --no-config --no-netrc --quiet --tries=3 --timeout=30 --max-redirect=10 --output-document={} --input-file=\"$job/request.urls\"",quote(&partial)?),
        "curl" => "curl --disable --config \"$job/request.conf\" > \"$job/http-status\"".into(),
        "fetch" => "/bin/sh \"$job/fetch.sh\"".into(),
        _ => bail!("Unsupported backend"),
    };
    // Values enter the shell exclusively as POSIX-quoted words. Cancellation uses
    // a private marker, never a stale PID that could refer to another process.
    Ok(format!(
        r#"#!/bin/sh
umask 077
job={jobdir}
part={partial}
target={target}
lock="$target.svd-lock"
backend={backend}
request_cleanup() {{ rm -f "$job/request.conf" "$job/request.body" "$job/request.urls" "$job/fetch.sh" "$job/backend.raw" "$job/publication.raw"; }}
trap request_cleanup EXIT
state() {{
  # Observers may inspect cleanup as soon as a terminal state becomes visible.
  case "$1" in Completed|Failed|Cancelled) request_cleanup;; esac
  printf '%s\n' "$1" > "$job/status.next"
  mv -f "$job/status.next" "$job/status"
}}
# Backend output may contain signed URLs or credentials. Retain only allowlisted
# classifications, never raw output or an interpolated line from a server.
diagnose() {{
  reason='No recognized diagnostic; raw output was removed to protect request credentials.'
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
      *'Permission denied'*) reason='Permission denied writing the output or reading request metadata.'; break;;
      *'No such file or directory'*) reason='A required input file or output directory is missing.'; break;;
      *'No space left on device'*) reason='No space left on the destination filesystem.'; break;;
      *'Could not resolve'*|*'could not resolve'*|*'Name or service not known'*|*'Name does not resolve'*) reason='Remote host name resolution failed.'; break;;
      *'Connection refused'*|*'Failed to connect'*) reason='The remote HTTP endpoint refused the connection.'; break;;
      *'timed out'*|*'Timeout'*) reason='The remote HTTP connection timed out.'; break;;
      *'certificate'*|*'Certificate'*) reason='TLS certificate verification failed.'; break;;
      *'404'*|*'Not Found'*) reason='The HTTP endpoint reported that the resource was not found.'; break;;
    esac
  done < "$1"
  printf '%s\n' "$reason" > "$job/backend.stderr.safe"
}}
state Starting
date +%s > "$job/heartbeat"
if ! mkdir "$lock" 2>/dev/null; then printf '%s\n' 'Destination reserved by another job; choose another filename.' > "$job/error"; state Failed; exit 1; fi
cleanup() {{ rm -f "$part" "$part.aria2"; rmdir "$lock" 2>/dev/null; }}
abort() {{ trap '' INT TERM; kill "$child" "$watcher" 2>/dev/null; wait "$child" 2>/dev/null; cleanup; state Failed; exit 1; }}
trap abort INT TERM
if [ -e "$target" ] || [ -L "$target" ]; then printf '%s\n' 'Destination already exists' > "$job/error"; cleanup; state Failed; exit 1; fi
if [ -f "$job/cancel" ]; then cleanup; state Cancelled; exit 0; fi
state Downloading
( exec {download} ) > "$job/backend.raw" 2>&1 &
child=$!
(
  while kill -0 "$child" 2>/dev/null; do
    date +%s > "$job/heartbeat"
    if [ -f "$job/cancel" ]; then kill "$child" 2>/dev/null; exit; fi
    sleep 1
  done
) &
watcher=$!
wait "$child"
code=$?
kill "$watcher" 2>/dev/null
wait "$watcher" 2>/dev/null
printf '%s\n' "$code" > "$job/backend.exit"
diagnose "$job/backend.raw"
if [ -f "$job/cancel" ]; then cleanup; state Cancelled; exit 0; fi
if [ -f "$job/http-status" ]; then
  http=$(cat "$job/http-status")
  case "$http" in
    2??) ;;
    3??) printf '%s\n' 'Redirect not followed: reopen the target in the browser and approve a new request. Authentication is never forwarded through redirects.' > "$job/error"; code=1;;
    *) printf '%s\n' "HTTP request failed (status $http). Authentication may be missing or the URL expired." > "$job/error"; code=1;;
  esac
fi
if [ "$code" -eq 0 ]; then
  if [ ! -f "$part" ] || [ -L "$part" ]; then
    printf '%s\n' "$backend exited successfully but the expected regular partial file is missing. Nothing was published." > "$job/error"
    cleanup; state Failed; exit 1
  fi
  # Hard-link publication is atomic and fails if the destination appears meanwhile.
  # Partial and final file are in the same directory and filesystem.
  if link "$part" "$target" 2> "$job/publication.raw"; then
    wc -c < "$target" > "$job/bytes"
    cleanup; state Completed; exit 0
  else
    diagnose "$job/publication.raw"
    printf '%s\n' 'Atomic publication failed; the destination may exist or the filesystem may not support hard links. No existing file was overwritten.' > "$job/error"
    cleanup; state Failed; exit 1
  fi
else
  if [ ! -s "$job/error" ]; then printf '%s\n' "$backend failed (exit $code). $reason" > "$job/error"; fi
  cleanup; state Failed; exit 1
fi
"#,
        jobdir = quote(jobdir)?,
        partial = quote(&partial)?,
        target = quote(&target)?,
        backend = quote(&job.backend)?,
        download = download
    ))
}
pub async fn list(c: &Connection, refresh: bool) -> Result<Vec<Job>> {
    if refresh {
        c.completed_jobs.lock().await.clear();
    }
    let root = root(c);
    if !c.sftp.try_exists(&root).await? {
        return Ok(Vec::new());
    }
    let server_time = ssh::exec(c, "date +%s").await?.trim().parse::<u64>()?;
    let mut jobs = Vec::new();
    let mut entries: Vec<_> = c.sftp.read_dir(&root).await?.collect();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.metadata().mtime.unwrap_or(0)));
    for entry in entries.into_iter().take(500) {
        let id = entry.file_name();
        if uuid::Uuid::parse_str(&id).is_err() || !entry.metadata().is_dir() {
            continue;
        }
        if let Some(job) = c.completed_jobs.lock().await.get(&id).cloned() {
            jobs.push(job);
            continue;
        }
        let dir = format!("{root}/{id}");
        let Ok(data) = small(c, &format!("{dir}/job.json")).await else {
            continue;
        };
        let Ok(mut job) = serde_json::from_str::<Job>(&data) else {
            continue;
        };
        if job.id != id {
            continue;
        }
        if security::path(&job.directory).is_err() || security::filename(&job.filename).is_err() {
            continue;
        }
        if let Ok(status) = small(c, &format!("{dir}/status")).await {
            job.status = status.trim().to_string();
        }
        if !matches!(
            job.status.as_str(),
            "Starting" | "Waiting" | "Downloading" | "Completed" | "Failed" | "Cancelled"
        ) {
            job.status = "Failed".into();
        }
        if matches!(job.status.as_str(), "Starting" | "Waiting" | "Downloading") {
            let heartbeat = small(c, &format!("{dir}/heartbeat"))
                .await
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
                .unwrap_or(job.started);
            if server_time.saturating_sub(heartbeat) > 60 {
                job.status = "Failed".into();
                job.error=Some("Worker heartbeat stopped. The server may have restarted or terminated the job; inspect the job directory before retrying.".into());
            }
        }
        let target = if job.status == "Completed" {
            security::join(&job.directory, &job.filename)?
        } else {
            format!(
                "{}/.svd-{}.part",
                job.directory.trim_end_matches('/'),
                job.id
            )
        };
        job.downloaded = c
            .sftp
            .metadata(&target)
            .await
            .ok()
            .and_then(|m| m.size)
            .unwrap_or(0);
        if job.status == "Completed" {
            job.total = Some(job.downloaded);
        }
        if job.status == "Failed" && job.error.is_none() {
            job.error = small(c, &format!("{dir}/error")).await.ok();
        }
        if matches!(job.status.as_str(), "Completed" | "Failed" | "Cancelled") {
            c.completed_jobs
                .lock()
                .await
                .insert(job.id.clone(), job.clone());
        }
        jobs.push(job);
    }
    jobs.sort_by_key(|j| std::cmp::Reverse(j.started));
    Ok(jobs)
}
pub async fn cancel(c: &Connection, id: &str) -> Result<()> {
    uuid::Uuid::parse_str(id)?;
    let dir = format!("{}/{id}", root(c));
    if !c.sftp.try_exists(&format!("{dir}/job.json")).await? {
        bail!("Download job not found")
    }
    write(c, &format!("{dir}/cancel"), b"cancel\n").await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_workers_for_native_freebsd_and_linux_ci() {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/compatibility-fixtures");
        std::fs::create_dir_all(&directory).unwrap();
        for backend in ["curl", "fetch", "wget", "aria2c"] {
            let job = Job {
                id: uuid::Uuid::nil().to_string(),
                url: String::new(),
                directory: "/SVD_FIXTURE_DEST".into(),
                filename: "-résumé ' ;$(touch nope).txt".into(),
                backend: backend.into(),
                started: 0,
                status: "Waiting".into(),
                downloaded: 0,
                total: None,
                error: None,
                persistent: true,
                method: "GET".into(),
                authenticated: false,
                header_count: 0,
            };
            let partial = format!("/SVD_FIXTURE_DEST/.svd-{}.part", job.id);
            let target = security::join(&job.directory, &job.filename).unwrap();
            let script = worker(&job, "/SVD_FIXTURE_JOB")
                .unwrap()
                .replace(&quote("/SVD_FIXTURE_JOB").unwrap(), "\"$SVD_JOB\"")
                .replace(
                    &quote(&partial).unwrap(),
                    &format!("\"$SVD_DEST/.svd-{}.part\"", job.id),
                )
                .replace(
                    &quote(&target).unwrap(),
                    &format!("\"$SVD_DEST\"/{}", quote(&job.filename).unwrap()),
                )
                .replace(&quote(&job.directory).unwrap(), "\"$SVD_DEST\"");
            assert!(!script.contains("chmod"));
            assert!(!script.contains("mv -fT"));
            assert!(!script.contains("ln -T"));
            assert!(!script.contains(" -- \""));
            std::fs::write(directory.join(format!("worker-{backend}.sh")), script).unwrap();
            // Native tests must use the real request serializer too: duplicating
            // only the shell worker would miss aria2's per-URI output options.
            std::fs::write(
                directory.join(format!("request-{backend}.urls")),
                url_input(backend, &job.id, "https://example.invalid/fixture")
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
            if backend == "fetch" {
                let fetch = fetch_script(&job, "https://example.invalid/fixture")
                    .unwrap()
                    .replace(
                        &quote(&partial).unwrap(),
                        &format!("\"$SVD_DEST/.svd-{}.part\"", job.id),
                    )
                    .replace(
                        &quote("https://example.invalid/fixture").unwrap(),
                        "\"$SVD_URL\"",
                    );
                std::fs::write(directory.join("fetch-command.sh"), fetch).unwrap();
            }
            std::fs::write(directory.join("filename.txt"), &job.filename).unwrap();
        }
    }
    #[test]
    fn confirmation_is_explicit_and_fetch_is_available_without_gnu_tools() {
        let input: Input =
            serde_json::from_value(serde_json::json!({"url":"https://example.com"})).unwrap();
        assert!(!input.confirmed);
        assert_eq!(select_backend(&["fetch".into()], false).unwrap(), "fetch");
        assert!(select_backend(&["fetch".into()], true).is_err());
        assert_eq!(
            select_backend(&["fetch".into(), "curl".into()], true).unwrap(),
            "curl"
        );
    }
    #[test]
    fn worker_quotes_untrusted_values() {
        let job = Job {
            id: uuid::Uuid::nil().to_string(),
            url: "https://example.com/a?q=';touch /tmp/pwn".into(),
            directory: "/tmp/a'b".into(),
            filename: "a;$(whoami).txt".into(),
            backend: "curl".into(),
            started: 0,
            status: "Waiting".into(),
            downloaded: 0,
            total: None,
            error: None,
            persistent: true,
            method: "GET".into(),
            authenticated: false,
            header_count: 0,
        };
        let script = worker(&job, "/tmp/job").unwrap();
        assert!(!script.contains(&job.url));
        assert!(script.contains("link \"$part\" \"$target\""));
        assert!(!script.contains("eval "));
    }

    #[test]
    fn worker_downloads_and_never_overwrites() -> Result<()> {
        use super::download_test_http::{Fixture, Response};
        use std::{
            process::{Command, Stdio},
            time::{Duration, Instant},
        };
        let shell = if cfg!(windows) {
            "C:/Program Files/Git/bin/sh.exe"
        } else {
            "/bin/sh"
        };
        anyhow::ensure!(
            std::path::Path::new(shell).is_file(),
            "The real worker test requires a POSIX shell (Git for Windows on Windows)"
        );
        let base = std::env::current_dir()
            .unwrap()
            .join("target/worker-tests")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&base).unwrap();
        let posix = |p: &std::path::Path| {
            let s = p.to_string_lossy().replace('\\', "/");
            if cfg!(windows) {
                format!("/{}{}", s[..1].to_lowercase(), &s[2..])
            } else {
                s
            }
        };
        let payload = b"A real HTTP response, fetched by the shell download worker.\n";
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            payload.len()
        )
        .into_bytes();
        response.extend_from_slice(payload);
        let server = Fixture::start(vec![Response::Complete(response)])?;
        let address = server.address;
        let mut job = Job {
            id: uuid::Uuid::new_v4().to_string(),
            url: format!("http://{address}/file?a=1&b=2"),
            directory: posix(&base),
            filename: "résumé ' ;$(printf nope).txt".into(),
            backend: "curl".into(),
            started: 0,
            status: "Waiting".into(),
            downloaded: 0,
            total: None,
            error: None,
            persistent: true,
            method: "GET".into(),
            authenticated: false,
            header_count: 0,
        };
        let run = |job: &Job, cancel: bool| {
            let dir = base.join(&job.id);
            std::fs::create_dir(&dir).unwrap();
            if cancel {
                std::fs::write(dir.join("cancel"), b"cancel").unwrap();
            }
            std::fs::write(dir.join("run.sh"), worker(job, &posix(&dir)).unwrap()).unwrap();
            let r = Resolved {
                url: job.url.clone(),
                method: job.method.clone(),
                headers: vec![],
                body: None,
                authenticated: false,
                total: None,
            };
            let mut config = curl_config(job, &posix(&dir), &r).unwrap().to_string();
            // Git's native curl does not MSYS-convert paths inside configuration files.
            if cfg!(windows) {
                config = config.replace(&posix(&base), &base.to_string_lossy().replace('\\', "/"));
            }
            std::fs::write(dir.join("request.conf"), config.as_bytes()).unwrap();
            let mut cmd = Command::new(shell);
            cmd.arg(posix(&dir.join("run.sh")))
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if cfg!(windows) {
                cmd.env(
                    "PATH",
                    format!(
                        "C:\\Program Files\\Git\\usr\\bin;C:\\Program Files\\Git\\mingw64\\bin;{}",
                        std::env::var("PATH").unwrap_or_default()
                    ),
                );
            }
            let mut child = cmd.spawn().unwrap();
            let start = Instant::now();
            loop {
                if child.try_wait().unwrap().is_some() {
                    break;
                }
                if start.elapsed() > Duration::from_secs(15) {
                    let _ = child.kill();
                    panic!("Download worker timed out");
                }
                std::thread::sleep(Duration::from_millis(30));
            }
            std::fs::read_to_string(dir.join("status")).unwrap()
        };
        let status = run(&job, false);
        server.next_request()?;
        assert_eq!(
            status.trim(),
            "Completed",
            "{}",
            std::fs::read_to_string(base.join(&job.id).join("error")).unwrap_or_default()
        );
        assert_eq!(std::fs::read(base.join(&job.filename)).unwrap(), payload);
        server.finish()?;
        // A second invocation must leave an existing destination intact.
        job.id = uuid::Uuid::new_v4().to_string();
        assert_eq!(run(&job, false).trim(), "Failed");
        assert_eq!(std::fs::read(base.join(&job.filename)).unwrap(), payload);
        // Pre-start cancellation never contacts the URL or publishes a file.
        job.id = uuid::Uuid::new_v4().to_string();
        job.filename = "cancelled.txt".into();
        assert_eq!(run(&job, true).trim(), "Cancelled");
        assert!(!base.join("cancelled.txt").exists());
        let resolved = base.canonicalize().unwrap();
        assert!(resolved.starts_with(
            std::env::current_dir()
                .unwrap()
                .canonicalize()
                .unwrap()
                .join("target/worker-tests")
        ));
        std::fs::remove_dir_all(resolved).unwrap();
        Ok(())
    }
}
