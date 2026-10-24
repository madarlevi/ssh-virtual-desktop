use crate::{
    security::{self, quote},
    ssh::{self, Connection},
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const LIMIT: u64 = 2 * 1024 * 1024;
#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_link: bool,
    size: u64,
    modified: u32,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub path: String,
    pub entries: Vec<Entry>,
    pub cached: bool,
    pub updated_at: u64,
}
pub async fn list(
    c: &Connection,
    path: &str,
    refresh: bool,
    app: &tauri::AppHandle,
) -> Result<Listing> {
    security::path(path)?;
    if !refresh {
        if let Some(listing) = c.directory_cache.get(path) {
            return Ok(listing);
        }
    }
    let listing = read_listing(c, path).await?;
    if c.directory_cache.remember(listing.clone()) {
        use tauri::Emitter;
        let _ = app.emit_to(
            tauri::EventTarget::webview("main"),
            "directory-cache-updated",
            serde_json::json!({"path":listing.path}),
        );
    }
    Ok(listing)
}
pub async fn read_listing(c: &Connection, path: &str) -> Result<Listing> {
    security::path(path)?;
    let path = c.sftp.canonicalize(path).await?;
    let mut entries = Vec::new();
    for e in c.sftp.read_dir(&path).await? {
        let name = e.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let md = e.metadata();
        entries.push(Entry {
            path: format!("{}/{}", path.trim_end_matches('/'), name),
            name,
            is_dir: md.is_dir(),
            is_link: md.is_symlink(),
            size: md.size.unwrap_or(0),
            modified: md.mtime.unwrap_or(0),
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Listing {
        path,
        entries,
        cached: false,
        updated_at: crate::directory_cache::now(),
    })
}
#[derive(Serialize, Deserialize)]
pub struct TextFile {
    pub path: String,
    pub text: String,
    pub revision: String,
}
pub async fn read(c: &Connection, path: &str) -> Result<TextFile> {
    security::path(path)?;
    if c.sftp.metadata(path).await?.size.unwrap_or(LIMIT + 1) > LIMIT {
        bail!("Text editor limit is 2 MiB")
    }
    let mut file = c.sftp.open(path).await?;
    let mut data = Vec::new();
    (&mut file).take(LIMIT + 1).read_to_end(&mut data).await?;
    if data.len() as u64 > LIMIT || data.contains(&0) {
        bail!("This is a binary or oversized file")
    }
    let revision = format!("{:x}", Sha256::digest(&data));
    let text = String::from_utf8(data)
        .context("This file is not valid UTF-8; editing it could corrupt its contents")?;
    Ok(TextFile {
        path: path.into(),
        text,
        revision,
    })
}
pub async fn save(c: &Connection, path: &str, text: &str, revision: &str) -> Result<TextFile> {
    if text.len() as u64 > LIMIT || text.contains('\0') {
        bail!("Text must be UTF-8 without NUL, at most 2 MiB")
    }
    // Hash detects same-size/same-mtime modifications too. A concurrent write in the
    // interval after this check is still possible (SFTP has no compare-and-swap).
    let current = read(c, path).await?;
    if current.revision != revision {
        bail!("Conflict: this file changed on the server. Reopen it before saving.")
    }
    let canonical = c.sftp.canonicalize(path).await?;
    let attrs = c.sftp.metadata(&canonical).await?;
    let parent = canonical
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .context("Invalid canonical path")?;
    let temp = format!("{}/.svd-edit-{}", parent, uuid::Uuid::new_v4());
    let mut f = c
        .sftp
        .open_with_flags_and_attributes(
            &temp,
            russh_sftp::protocol::OpenFlags::CREATE
                | russh_sftp::protocol::OpenFlags::EXCLUDE
                | russh_sftp::protocol::OpenFlags::WRITE,
            russh_sftp::protocol::FileAttributes {
                permissions: Some(0o600),
                ..Default::default()
            },
        )
        .await?;
    f.write_all(text.as_bytes()).await?;
    f.shutdown().await?;
    // Preserve permissions, then use same-filesystem POSIX rename, not delete+write.
    c.sftp
        .set_metadata(
            &temp,
            russh_sftp::protocol::FileAttributes {
                permissions: attrs.permissions,
                ..Default::default()
            },
        )
        .await?;
    if let Err(e) = ssh::exec(
        c,
        &format!("mv -fT -- {} {}", quote(&temp)?, quote(&canonical)?),
    )
    .await
    {
        let _ = c.sftp.remove_file(&temp).await;
        return Err(e);
    }
    Ok(TextFile {
        path: path.into(),
        text: text.into(),
        revision: format!("{:x}", Sha256::digest(text.as_bytes())),
    })
}
pub async fn operation(
    c: &Connection,
    kind: &str,
    source: &str,
    destination: Option<&str>,
) -> Result<()> {
    security::path(source)?;
    match kind {
        "mkdir" => c.sftp.create_dir(source).await?,
        "delete" => {
            let md = c.sftp.symlink_metadata(source).await?;
            // Empty directories only; recursive deletion is deliberately unavailable.
            if md.is_dir() {
                c.sftp.remove_dir(source).await?;
            } else {
                c.sftp.remove_file(source).await?;
            }
        }
        "move" | "rename" | "copy" => {
            let destination = destination.context("Destination required")?;
            security::path(destination)?;
            if c.sftp.try_exists(destination).await? {
                bail!("Destination already exists")
            }
            if kind == "copy" {
                // Entire copy runs on Linux. GNU cp is capability-tested by command failure.
                ssh::exec(
                    c,
                    &format!(
                        "cp -a --no-clobber -- {} {}",
                        quote(source)?,
                        quote(destination)?
                    ),
                )
                .await?;
            } else {
                c.sftp.rename(source, destination).await?;
            }
        }
        _ => bail!("Unsupported file operation"),
    }
    Ok(())
}
