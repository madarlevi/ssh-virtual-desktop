use crate::store::Profile;
use anyhow::{bail, Context, Result};
use russh::{client, ChannelMsg, Disconnect};
use russh_sftp::client::SftpSession;
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex as StdMutex},
    time::Duration,
};
use tauri::{Emitter, EventTarget};
use tokio::sync::{mpsc, Mutex};

pub struct HostHandler {
    pub expected: Option<String>,
    pub observed: Arc<StdMutex<Option<String>>>,
}
impl client::Handler for HostHandler {
    type Error = russh::Error;
    async fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let fingerprint = key
            .public_key()
            .fingerprint(russh::keys::HashAlg::Sha256)
            .to_string();
        *self.observed.lock().unwrap() = Some(fingerprint.clone());
        Ok(self.expected.as_ref() == Some(&fingerprint))
    }
}
pub struct Connection {
    pub remote: crate::remote::RemoteCapabilities,
    pub handle: client::Handle<HostHandler>,
    pub sftp: Arc<SftpSession>,
    pub home: String,
    pub profile: Profile,
    pub completed_jobs: Mutex<HashMap<String, crate::downloads::Job>>,
    pub directory_cache: Arc<crate::directory_cache::DirectoryCache>,
}
pub enum TerminalInput {
    Data(Vec<u8>),
    Resize(u32, u32),
    Close,
}
#[derive(Default)]
pub struct Ssh {
    pub connection: Mutex<Option<Arc<Connection>>>,
    pub terminals: Mutex<HashMap<String, mpsc::Sender<TerminalInput>>>,
    pub pending_host: Mutex<Option<(String, String)>>,
}
impl Ssh {
    pub async fn get(&self) -> Result<Arc<Connection>> {
        let conn = self
            .connection
            .lock()
            .await
            .clone()
            .context("Connect to an SSH server first")?;
        if conn.handle.is_closed() {
            bail!("SSH connection closed. Reconnect from the desktop menu.")
        }
        Ok(conn)
    }
    pub async fn connect(
        &self,
        profile: Profile,
        secret: &str,
        passphrase: &str,
        expected: Option<String>,
    ) -> Result<Arc<Connection>> {
        if profile.host.trim().is_empty() || profile.username.is_empty() || profile.port == 0 {
            bail!("Hostname, username and a valid port are required")
        }
        let observed = Arc::new(StdMutex::new(None));
        let config = client::Config {
            keepalive_interval: Some(Duration::from_secs(20)),
            keepalive_max: 3,
            inactivity_timeout: None,
            ..Default::default()
        };
        let result = tokio::time::timeout(
            Duration::from_secs(20),
            client::connect(
                Arc::new(config),
                (profile.host.as_str(), profile.port),
                HostHandler {
                    expected: expected.clone(),
                    observed: observed.clone(),
                },
            ),
        )
        .await
        .context("SSH connection timed out")?;
        let mut handle = match result {
            Ok(h) => h,
            Err(e) => {
                let fingerprint = observed.lock().unwrap().clone();
                if let Some(fp) = fingerprint {
                    if expected.is_none() {
                        *self.pending_host.lock().await = Some((host_id(&profile), fp.clone()));
                        bail!("HOST_UNKNOWN:{fp}")
                    } else if expected.as_ref() != Some(&fp) {
                        bail!("HOST_CHANGED:{fp}")
                    }
                }
                return Err(e.into());
            }
        };
        let authenticated = match profile.auth.as_str() {
            "password" => handle
                .authenticate_password(&profile.username, secret)
                .await?
                .success(),
            "key" => {
                let key = russh::keys::load_secret_key(
                    &profile.key_path,
                    if passphrase.is_empty() {
                        None
                    } else {
                        Some(passphrase)
                    },
                )?;
                let hash = handle
                    .best_supported_rsa_hash()
                    .await?
                    .flatten()
                    .or(Some(russh::keys::HashAlg::Sha512));
                handle
                    .authenticate_publickey(
                        &profile.username,
                        russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                    )
                    .await?
                    .success()
            }
            _ => bail!("Unsupported authentication method"),
        };
        if !authenticated {
            bail!("Authentication failed. Check username, password or private key.")
        }
        let channel = handle.channel_open_session().await?;
        channel.request_subsystem(true, "sftp").await?;
        let sftp = Arc::new(SftpSession::new(channel.into_stream()).await?);
        let home = match sftp.canonicalize(".").await {
            Ok(home) => home,
            Err(_) => sftp.canonicalize("/").await?,
        };
        let mut conn = Connection {
            remote: crate::remote::RemoteCapabilities::default(),
            handle,
            sftp,
            home,
            profile,
            completed_jobs: Mutex::new(HashMap::new()),
            directory_cache: Arc::new(crate::directory_cache::DirectoryCache::default()),
        };
        conn.remote =
            crate::remote::RemoteCapabilities::parse(&exec(&conn, crate::remote::DETECT).await?);
        let conn = Arc::new(conn);
        self.disconnect().await;
        *self.connection.lock().await = Some(conn.clone());
        Ok(conn)
    }
    pub async fn disconnect(&self) {
        for (_, tx) in self.terminals.lock().await.drain() {
            let _ = tx.send(TerminalInput::Close).await;
        }
        if let Some(c) = self.connection.lock().await.take() {
            c.directory_cache.stop();
            let _ = tokio::time::timeout(Duration::from_secs(2), c.sftp.close()).await;
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                c.handle
                    .disconnect(Disconnect::ByApplication, "Client disconnected", "en"),
            )
            .await;
        }
    }
    pub async fn terminal(
        &self,
        app: tauri::AppHandle,
        id: String,
        cols: u32,
        rows: u32,
        cwd: Option<String>,
    ) -> Result<()> {
        uuid::Uuid::parse_str(&id)?;
        if self.terminals.lock().await.contains_key(&id) {
            bail!("Terminal already exists")
        }
        let conn = self.get().await?;
        let mut channel = conn.handle.channel_open_session().await?;
        channel
            .request_pty(true, "xterm-256color", cols.max(1), rows.max(1), 0, 0, &[])
            .await?;
        channel
            .exec(true, crate::terminal_shell::command(cwd.as_deref())?)
            .await?;
        let (tx, mut rx) = mpsc::channel(128);
        self.terminals.lock().await.insert(id.clone(), tx);
        tokio::spawn(async move {
            use base64::Engine;
            let event = format!("terminal-{id}");
            loop {
                tokio::select! {
                    input = rx.recv() => match input {
                        Some(TerminalInput::Data(d)) => { if channel.data(&d[..]).await.is_err() { break } },
                        Some(TerminalInput::Resize(c,r)) => { let _ = channel.window_change(c.max(1),r.max(1),0,0).await; },
                        _ => { let _ = channel.eof().await; let _ = channel.close().await; break }
                    },
                    output = channel.wait() => match output {
                        Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                            let _ = app.emit_to(EventTarget::webview("main"), &event, TerminalEvent { data: Some(base64::engine::general_purpose::STANDARD.encode(&data)), closed: false });
                        },
                        Some(ChannelMsg::Close) | None => break,
                        _ => {}
                    }
                }
            }
            let _ = app.emit_to(
                EventTarget::webview("main"),
                &event,
                TerminalEvent {
                    data: None,
                    closed: true,
                },
            );
        });
        Ok(())
    }
    pub async fn send(&self, id: &str, input: TerminalInput) -> Result<()> {
        let tx = self
            .terminals
            .lock()
            .await
            .get(id)
            .cloned()
            .context("Terminal is closed")?;
        tx.send(input).await.context("Terminal channel is closed")
    }
}
#[derive(Clone, Serialize)]
struct TerminalEvent {
    data: Option<String>,
    closed: bool,
}
pub fn host_id(p: &Profile) -> String {
    format!("[{}]:{}", p.host.to_lowercase(), p.port)
}

/// Executes one bounded control operation. Download bodies never traverse this path.
pub async fn exec(conn: &Connection, command: &str) -> Result<String> {
    tokio::time::timeout(Duration::from_secs(45), async {
        let mut channel = conn.handle.channel_open_session().await?;
        // FreeBSD accounts commonly use tcsh. The control language is always sh.
        channel
            .exec(
                true,
                format!("/bin/sh -c {}", crate::security::quote(command)?),
            )
            .await?;
        let mut data = Vec::new();
        let mut error = Vec::new();
        let mut status = None;
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data: d } => data.extend_from_slice(&d),
                ChannelMsg::ExtendedData { data: d, .. } => error.extend_from_slice(&d),
                ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                _ => {}
            }
            if data.len() + error.len() > 1024 * 1024 {
                let _ = channel.close().await;
                bail!("Remote command output exceeded 1 MiB")
            }
        }
        if status != Some(0) {
            bail!(
                "Remote command failed ({}): {}",
                status
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "no exit status".into()),
                String::from_utf8_lossy(&error)
            )
        }
        Ok(String::from_utf8_lossy(&data).into_owned())
    })
    .await
    .context("Remote control command timed out")?
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::client::Handler;
    #[tokio::test]
    async fn host_keys_require_an_exact_pin() {
        let key: russh::keys::PublicKeyOrCertificate = russh::keys::PublicKey::from_openssh(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILM+jvN+ot98qgEN796jTiQfZfG1KaT0PtFDJ/XFSqti",
        )
        .unwrap()
        .into();
        let fingerprint = key
            .public_key()
            .fingerprint(russh::keys::HashAlg::Sha256)
            .to_string();
        for (expected, accepted) in [
            (None, false),
            (Some("SHA256:wrong".into()), false),
            (Some(fingerprint.clone()), true),
        ] {
            let observed = Arc::new(StdMutex::new(None));
            let mut handler = HostHandler {
                expected,
                observed: observed.clone(),
            };
            assert_eq!(handler.check_server_key(&key).await.unwrap(), accepted);
            assert_eq!(*observed.lock().unwrap(), Some(fingerprint.clone()));
        }
    }
}
