//! Explicitly approved browser-generated downloads. File bytes pass through Windows.
use crate::{downloads::Job, security, ssh::Connection};
use anyhow::{bail, Context, Result};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::Manager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
struct Transfer {
    profile: String,
    job: Job,
    cancel: Arc<AtomicBool>,
}
#[derive(Default)]
pub struct LocalDownloads(Mutex<HashMap<String, Transfer>>);
impl LocalDownloads {
    pub fn list(&self, profile: &str) -> Vec<Job> {
        self.0
            .lock()
            .unwrap()
            .values()
            .filter(|t| t.profile == profile)
            .map(|t| t.job.clone())
            .collect()
    }
    pub fn cancel(&self, profile: &str, id: &str) -> bool {
        if let Some(t) = self
            .0
            .lock()
            .unwrap()
            .get(id)
            .filter(|t| t.profile == profile)
        {
            t.cancel.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }
    pub fn disconnect(&self) {
        for t in self.0.lock().unwrap().values() {
            t.cancel.store(true, Ordering::SeqCst);
        }
    }
    pub fn clear(&self, profile: &str) {
        self.0.lock().unwrap().retain(|_, t| {
            t.profile != profile || matches!(t.job.status.as_str(), "Starting" | "Downloading")
        });
    }
    fn update(&self, id: &str, change: impl FnOnce(&mut Job)) {
        if let Some(t) = self.0.lock().unwrap().get_mut(id) {
            change(&mut t.job);
        }
    }
}
pub fn clean_interrupted_cache(app: &tauri::AppHandle) {
    // Only unlink our fixed payload in UUID folders. Never recursively traverse caches.
    let Ok(root) = app
        .path()
        .app_cache_dir()
        .map(|p| p.join("approved-downloads"))
    else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    for entry in entries.flatten() {
        if uuid::Uuid::parse_str(&entry.file_name().to_string_lossy()).is_err() {
            continue;
        }
        if !entry
            .file_type()
            .is_ok_and(|t| t.is_dir() && !t.is_symlink())
        {
            continue;
        }
        let _ = std::fs::remove_file(entry.path().join("payload.part"));
        let _ = std::fs::remove_dir(entry.path());
    }
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub token: String,
    pub directory: String,
    pub filename: String,
    pub confirmed: bool,
    pub allow_client_transfer: bool,
}
fn consent(r: &Request) -> Result<()> {
    if !r.confirmed || !r.allow_client_transfer {
        bail!("Approve the Windows-download-and-SFTP-upload fallback and confirm its destination first")
    }
    Ok(())
}
pub async fn start(app: &tauri::AppHandle, c: Arc<Connection>, r: Request) -> Result<Job> {
    consent(&r)?;
    uuid::Uuid::parse_str(&r.token)?;
    security::path(&r.directory)?;
    security::filename(&r.filename)?;
    let directory = c.sftp.canonicalize(&r.directory).await?;
    if !c.sftp.metadata(&directory).await?.is_dir() {
        bail!("Destination is not a directory")
    }
    let target = security::join(&directory, &r.filename)?;
    if c.sftp.try_exists(&target).await? {
        bail!("Destination already exists; choose another filename")
    }
    let cache = app
        .path()
        .app_cache_dir()?
        .join("approved-downloads")
        .join(&r.token);
    tokio::fs::create_dir_all(&cache).await?;
    let file = cache.join("payload.part");
    let view = app.get_webview("main").context("Desktop closed")?;
    if let Err(e) = crate::native_browser::allow_local(&view, r.token.clone(), file.clone()).await {
        let _ = tokio::fs::remove_dir(&cache).await;
        return Err(e);
    }
    let job = Job {
        id: r.token.clone(),
        url: String::new(),
        directory,
        filename: r.filename,
        backend: "WebView2 → SFTP (client transfer)".into(),
        started: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
        status: "Starting".into(),
        downloaded: 0,
        total: None,
        error: None,
        persistent: false,
        method: "BROWSER".into(),
        authenticated: false,
        header_count: 0,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    app.state::<LocalDownloads>().0.lock().unwrap().insert(
        job.id.clone(),
        Transfer {
            profile: c.profile.id.clone(),
            job: job.clone(),
            cancel: cancel.clone(),
        },
    );
    let app = app.clone();
    let id = job.id.clone();
    let partial = security::join(&job.directory, &format!(".svd-local-{}.part", id))?;
    tauri::async_runtime::spawn(async move {
        let outcome = async {
            loop {
                if cancel.load(Ordering::SeqCst) {
                    bail!("Transfer cancelled")
                }
                let (state, bytes, total) =
                    crate::native_browser::local_state(&view, id.clone()).await?;
                app.state::<LocalDownloads>().update(&id, |j| {
                    j.downloaded = bytes;
                    j.total = total;
                });
                if state == "completed" {
                    break;
                }
                if state == "interrupted" {
                    bail!("The browser download was interrupted; reopen the source to retry")
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            let mut input = tokio::fs::File::open(&file).await?;
            let size = input.metadata().await?.len();
            app.state::<LocalDownloads>().update(&id, |j| {
                j.status = "Downloading".into();
                j.downloaded = 0;
                j.total = Some(size);
            });
            let mut output = c
                .sftp
                .open_with_flags_and_attributes(
                    &partial,
                    russh_sftp::protocol::OpenFlags::CREATE
                        | russh_sftp::protocol::OpenFlags::EXCLUDE
                        | russh_sftp::protocol::OpenFlags::WRITE,
                    russh_sftp::protocol::FileAttributes {
                        permissions: Some(0o600),
                        ..Default::default()
                    },
                )
                .await?;
            let mut buffer = vec![0; 65536];
            let mut sent = 0;
            loop {
                if cancel.load(Ordering::SeqCst) {
                    bail!("Transfer cancelled")
                };
                let n = input.read(&mut buffer).await?;
                if n == 0 {
                    break;
                }
                output.write_all(&buffer[..n]).await?;
                sent += n as u64;
                app.state::<LocalDownloads>()
                    .update(&id, |j| j.downloaded = sent);
            }
            output.shutdown().await?;
            if c.sftp.metadata(&partial).await?.size != Some(size) {
                bail!("Remote upload size did not match the browser file")
            }
            if cancel.load(Ordering::SeqCst) {
                bail!("Transfer cancelled")
            }
            // SFTP v3 rename refuses an existing destination; it never overwrites.
            c.sftp.rename(&partial, &target).await?;
            c.directory_cache.invalidate(&[&target]);
            Ok::<(), anyhow::Error>(())
        }
        .await;
        let _ = crate::native_browser::discard_local(&view, id.clone()).await;
        let _ = c.sftp.remove_file(&partial).await;
        let _ = tokio::fs::remove_file(&file).await;
        let _ = tokio::fs::remove_dir(&cache).await;
        app.state::<LocalDownloads>().update(&id,|j|match outcome {Ok(())=>{j.status="Completed".into();j.total=Some(j.downloaded);},Err(_)=>{j.status=if cancel.load(Ordering::SeqCst){"Cancelled"}else{"Failed"}.into();j.error=Some("Client transfer did not finish. Keep the browser tab and SSH connection open; check destination permissions, then retry from the source page.".into());}});
    });
    Ok(job)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn client_transfer_needs_both_explicit_approval_and_confirmation() {
        let mut r = Request {
            token: uuid::Uuid::nil().to_string(),
            directory: "/tmp".into(),
            filename: "file.txt".into(),
            confirmed: false,
            allow_client_transfer: false,
        };
        assert!(consent(&r).is_err());
        r.confirmed = true;
        assert!(consent(&r).is_err());
        r.allow_client_transfer = true;
        assert!(consent(&r).is_ok());
        r.confirmed = false;
        assert!(consent(&r).is_err());
    }
    #[test]
    fn cancellation_and_history_are_profile_scoped() {
        let downloads = LocalDownloads::default();
        let cancel = Arc::new(AtomicBool::new(false));
        let job = Job {
            id: uuid::Uuid::new_v4().to_string(),
            url: String::new(),
            directory: "/root".into(),
            filename: "file".into(),
            backend: "local".into(),
            started: 0,
            status: "Downloading".into(),
            downloaded: 42,
            total: None,
            error: None,
            persistent: false,
            method: "BROWSER".into(),
            authenticated: false,
            header_count: 0,
        };
        downloads.0.lock().unwrap().insert(
            job.id.clone(),
            Transfer {
                profile: "a".into(),
                job: job.clone(),
                cancel: cancel.clone(),
            },
        );
        assert!(!downloads.cancel("b", &job.id));
        assert!(!cancel.load(Ordering::SeqCst));
        assert!(downloads.list("b").is_empty());
        downloads.clear("a");
        assert_eq!(downloads.list("a").len(), 1);
        assert!(downloads.cancel("a", &job.id));
        assert!(cancel.load(Ordering::SeqCst));
        downloads.update(&job.id, |j| j.status = "Cancelled".into());
        downloads.clear("a");
        assert!(downloads.list("a").is_empty());
    }
}
