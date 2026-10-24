#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod browser;
#[cfg(windows)]
mod browser_requests;
mod directory_cache;
mod download_name;
mod download_request;
mod downloads;
mod files;
#[cfg(windows)]
mod local_downloads;
#[cfg(windows)]
mod native_browser;
mod remote;
mod security;
mod ssh;
mod store;
mod terminal_shell;

use crate::store::Profile;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{Manager, State};
use tokio::sync::Mutex;
use zeroize::Zeroizing;

#[derive(Default)]
struct AppState {
    ssh: ssh::Ssh,
    store: Mutex<store::Store>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RpcError {
    message: String,
    kind: String,
    fingerprint: Option<String>,
}
impl From<anyhow::Error> for RpcError {
    fn from(e: anyhow::Error) -> Self {
        let message = format!("{e:#}");
        let (kind, fingerprint) = if let Some(fp) = message.strip_prefix("HOST_UNKNOWN:") {
            ("hostUnknown", Some(fp.to_string()))
        } else if let Some(fp) = message.strip_prefix("HOST_CHANGED:") {
            ("hostChanged", Some(fp.to_string()))
        } else {
            ("error", None)
        };
        Self {
            message,
            kind: kind.into(),
            fingerprint,
        }
    }
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("Missing {key}"))
}
fn num(v: &Value, key: &str, default: u64) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(default)
}
fn trusted(webview: &tauri::Webview) -> Result<()> {
    let url = webview.url()?;
    if !security::ipc_origin(webview.label(), &url, cfg!(debug_assertions)) {
        bail!("Privileged commands are restricted to the local desktop WebView")
    }
    Ok(())
}
#[tauri::command]
async fn rpc(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    op: String,
    args: Value,
) -> Result<Value, RpcError> {
    trusted(&webview)?;
    dispatch(&app, &state, &op, args).await.map_err(Into::into)
}
async fn dispatch(
    app: &tauri::AppHandle,
    state: &AppState,
    op: &str,
    args: Value,
) -> Result<Value> {
    match op {
        "initialize" => {
            let store = state.store.lock().await;
            Ok(json!({ "profiles": store.profiles, "appearance": store.appearance }))
        }
        "save_profile" => {
            let mut profile: Profile = serde_json::from_value(args)?;
            if profile.id.is_empty() {
                profile.id = uuid::Uuid::new_v4().to_string();
            }
            uuid::Uuid::parse_str(&profile.id)?;
            let mut store = state.store.lock().await;
            if let Some(old) = store.profiles.iter().find(|p| p.id == profile.id) {
                if old.host != profile.host
                    || old.port != profile.port
                    || old.username != profile.username
                    || old.auth != profile.auth
                    || old.key_path != profile.key_path
                {
                    profile.id = uuid::Uuid::new_v4().to_string();
                    profile.name = format!("{}@{}", profile.username, profile.host);
                }
            }
            store.profiles.retain(|p| p.id != profile.id);
            store.profiles.push(profile.clone());
            store::save(app, &store)?;
            Ok(json!(profile))
        }
        "delete_profile" => {
            let id = string(&args, "id")?;
            let mut store = state.store.lock().await;
            store.profiles.retain(|p| p.id != id);
            store::save(app, &store)?;
            for field in ["password", "passphrase"] {
                let _ = store::credential(id, field)?.delete_credential();
            }
            Ok(Value::Null)
        }
        "trust_host" => {
            let fingerprint = string(&args, "fingerprint")?;
            let (host, fp) = state
                .ssh
                .pending_host
                .lock()
                .await
                .take()
                .context("No pending host trust request")?;
            if fp != fingerprint {
                bail!("Fingerprint does not match the pending host")
            }
            let mut store = state.store.lock().await;
            if store.hosts.contains_key(&host) {
                bail!("Existing host keys cannot be replaced by first-use confirmation")
            }
            store.hosts.insert(host, fp);
            store::save(app, &store)?;
            Ok(Value::Null)
        }
        "connect" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Connect {
                profile: Profile,
                secret: Option<String>,
                passphrase: Option<String>,
                remember: bool,
            }
            let req: Connect = serde_json::from_value(args)?;
            let secret = Zeroizing::new(
                req.secret
                    .filter(|s| !s.is_empty())
                    .or_else(|| {
                        store::credential(&req.profile.id, "password")
                            .ok()?
                            .get_password()
                            .ok()
                    })
                    .unwrap_or_default(),
            );
            let pass = Zeroizing::new(
                req.passphrase
                    .filter(|s| !s.is_empty())
                    .or_else(|| {
                        store::credential(&req.profile.id, "passphrase")
                            .ok()?
                            .get_password()
                            .ok()
                    })
                    .unwrap_or_default(),
            );
            let expected = state
                .store
                .lock()
                .await
                .hosts
                .get(&ssh::host_id(&req.profile))
                .cloned();
            let conn = tokio::time::timeout(
                std::time::Duration::from_secs(60),
                state
                    .ssh
                    .connect(req.profile.clone(), &secret, &pass, expected),
            )
            .await
            .context("SSH authentication or SFTP initialization timed out")??;
            if req.remember {
                if req.profile.auth == "password" {
                    store::credential(&req.profile.id, "password")?.set_password(&secret)?;
                }
                if !pass.is_empty() {
                    store::credential(&req.profile.id, "passphrase")?.set_password(&pass)?;
                }
            }
            Ok(json!({ "home": conn.home, "profile": conn.profile, "remote":conn.remote }))
        }
        "disconnect" => {
            app.state::<download_request::RequestVault>().clear();
            #[cfg(windows)]
            app.state::<local_downloads::LocalDownloads>().disconnect();
            state.ssh.disconnect().await;
            browser::close_all(app);
            Ok(Value::Null)
        }
        "health" => {
            let c = state.ssh.get().await?;
            Ok(json!({"connected": !c.handle.is_closed()}))
        }
        "terminal_open" => {
            state
                .ssh
                .terminal(
                    app.clone(),
                    string(&args, "id")?.into(),
                    num(&args, "cols", 80) as u32,
                    num(&args, "rows", 24) as u32,
                    args.get("cwd").and_then(Value::as_str).map(Into::into),
                )
                .await?;
            Ok(Value::Null)
        }
        "terminal_write" => {
            state
                .ssh
                .send(
                    string(&args, "id")?,
                    ssh::TerminalInput::Data(string(&args, "data")?.as_bytes().to_vec()),
                )
                .await?;
            Ok(Value::Null)
        }
        "terminal_resize" => {
            state
                .ssh
                .send(
                    string(&args, "id")?,
                    ssh::TerminalInput::Resize(
                        num(&args, "cols", 80) as u32,
                        num(&args, "rows", 24) as u32,
                    ),
                )
                .await?;
            Ok(Value::Null)
        }
        "terminal_close" => {
            if let Some(tx) = state
                .ssh
                .terminals
                .lock()
                .await
                .remove(string(&args, "id")?)
            {
                let _ = tx.send(ssh::TerminalInput::Close).await;
            }
            Ok(Value::Null)
        }
        "list_dir" => {
            let c = state.ssh.get().await?;
            Ok(serde_json::to_value(
                files::list(
                    &c,
                    string(&args, "path")?,
                    args.get("refresh")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    app,
                )
                .await?,
            )?)
        }
        "directory_scan_start" => {
            let c = state.ssh.get().await?;
            Ok(serde_json::to_value(
                directory_cache::DirectoryCache::start(
                    c,
                    app.clone(),
                    string(&args, "path")?,
                    args.get("depth")
                        .and_then(Value::as_u64)
                        .context("Depth must be a whole number from 0 to 16")?,
                )
                .await?,
            )?)
        }
        "directory_scan_status" => {
            let c = state.ssh.get().await?;
            Ok(serde_json::to_value(c.directory_cache.status())?)
        }
        "directory_scan_stop" => {
            let c = state.ssh.get().await?;
            c.directory_cache.stop();
            use tauri::Emitter;
            app.emit_to(
                tauri::EventTarget::webview("main"),
                "directory-scan",
                c.directory_cache.status(),
            )?;
            app.emit_to(
                tauri::EventTarget::webview("main"),
                "directory-cache-updated",
                json!({"path":null}),
            )?;
            Ok(Value::Null)
        }
        "file_op" => {
            let c = state.ssh.get().await?;
            let outcome = files::operation(
                &c,
                string(&args, "kind")?,
                string(&args, "source")?,
                args.get("destination").and_then(Value::as_str),
            )
            .await;
            let mut paths = vec![string(&args, "source")?];
            if let Some(dest) = args.get("destination").and_then(Value::as_str) {
                paths.push(dest);
            }
            c.directory_cache.invalidate(&paths);
            outcome?;
            Ok(Value::Null)
        }
        "read_text" => {
            let c = state.ssh.get().await?;
            Ok(serde_json::to_value(
                files::read(&c, string(&args, "path")?).await?,
            )?)
        }
        "save_text" => {
            let c = state.ssh.get().await?;
            let saved = files::save(
                &c,
                string(&args, "path")?,
                string(&args, "text")?,
                string(&args, "revision")?,
            )
            .await?;
            c.directory_cache.invalidate(&[string(&args, "path")?]);
            Ok(serde_json::to_value(saved)?)
        }
        "download_start" => {
            let c = state.ssh.get().await?;
            let job = downloads::start(
                &c,
                serde_json::from_value(args)?,
                &app.state::<download_request::RequestVault>(),
            )
            .await?;
            let mut store = state.store.lock().await;
            store.remember_download(&c.profile.id, &job.directory, true)?;
            if store::save(app, &store).is_err() {
                use tauri::Emitter;
                let _ = app.emit_to(
                    tauri::EventTarget::webview("main"),
                    "settings-warning",
                    "Download started, but its destination could not be saved locally.",
                );
            }
            Ok(serde_json::to_value(job)?)
        }
        "download_destination" => {
            let c = state.ssh.get().await?;
            let previous = state
                .store
                .lock()
                .await
                .last_downloads
                .get(&c.profile.id)
                .cloned();
            let path = downloads::destination(&c, previous.as_deref()).await?;
            Ok(json!({"path":path,"fallback":previous.as_ref().is_some_and(|p|p!=&path)}))
        }
        "download_clear" => {
            let c = state.ssh.get().await?;
            downloads::clear_history(&c).await?;
            #[cfg(windows)]
            app.state::<local_downloads::LocalDownloads>()
                .clear(&c.profile.id);
            Ok(Value::Null)
        }
        "download_preview" => {
            let input: download_request::Input = serde_json::from_value(args)?;
            let summary = input.summary(&app.state::<download_request::RequestVault>())?;
            let c = state.ssh.get().await?;
            let tools = downloads::backends(&c).await?;
            let advanced = summary.method != "GET"
                || (input.offer_id.is_none()
                    || input.forward_headers
                    || input.allow_authentication)
                    && summary.header_names.iter().any(|name| {
                        input.allow_authentication || !download_request::sensitive(name)
                    });
            let backend = downloads::select_backend(&tools, advanced)?;
            if backend == "fetch" && url::Url::parse(&input.url)?.query().is_some() {
                bail!("FreeBSD fetch cannot protect query/signed URLs in its process arguments. Use curl, aria2c or wget for this link.")
            }
            Ok(
                json!({"request":summary,"backend":backend,"capabilities":remote::backend_info(backend),"platform":c.remote.platform}),
            )
        }
        "download_discard" => {
            app.state::<download_request::RequestVault>()
                .discard(string(&args, "id")?);
            Ok(Value::Null)
        }
        "download_local_start" => {
            #[cfg(windows)]
            {
                let c = state.ssh.get().await?;
                let job =
                    local_downloads::start(app, c.clone(), serde_json::from_value(args)?).await?;
                let mut settings = state.store.lock().await;
                settings.remember_download(&c.profile.id, &job.directory, true)?;
                if store::save(app, &settings).is_err() {
                    use tauri::Emitter;
                    let _ = app.emit_to(
                        tauri::EventTarget::webview("main"),
                        "settings-warning",
                        "Transfer started, but the destination could not be saved locally.",
                    );
                }
                Ok(serde_json::to_value(job)?)
            }
            #[cfg(not(windows))]
            bail!("Client transfers require Windows WebView2")
        }
        "download_local_discard" => {
            #[cfg(windows)]
            if let Some(view) = app.get_webview("main") {
                native_browser::discard_local(&view, string(&args, "token")?.to_owned()).await?;
            }
            Ok(Value::Null)
        }
        "download_list" => {
            let c = state.ssh.get().await?;
            let mut jobs = downloads::list(
                &c,
                args.get("refresh")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            )
            .await?;
            #[cfg(windows)]
            jobs.extend(
                app.state::<local_downloads::LocalDownloads>()
                    .list(&c.profile.id),
            );
            jobs.sort_by_key(|j| std::cmp::Reverse(j.started));
            Ok(serde_json::to_value(jobs)?)
        }
        "download_cancel" => {
            let c = state.ssh.get().await?;
            let id = string(&args, "id")?;
            #[cfg(windows)]
            if app
                .state::<local_downloads::LocalDownloads>()
                .cancel(&c.profile.id, id)
            {
                return Ok(Value::Null);
            }
            downloads::cancel(&c, id).await?;
            Ok(Value::Null)
        }
        "browser_open" => {
            browser::open(app, &args).await?;
            Ok(Value::Null)
        }
        "browser_bounds" => {
            browser::bounds(app, &args)?;
            Ok(Value::Null)
        }
        "browser_desktop" => {
            browser::desktop(app, &args)?;
            Ok(Value::Null)
        }
        "browser_action" => {
            browser::action(app, &args)?;
            Ok(Value::Null)
        }
        "browser_find" => {
            #[cfg(windows)]
            {
                let id = string(&args, "id")?;
                uuid::Uuid::parse_str(id)?;
                let view = app
                    .get_webview(&format!("browser-{id}"))
                    .context("Tab has closed")?;
                native_browser::find(
                    &view,
                    string(&args, "term")?.to_owned(),
                    string(&args, "action")?.to_owned(),
                )
                .await?;
            }
            Ok(Value::Null)
        }
        "appearance" => {
            let mut store = state.store.lock().await;
            store.appearance = args;
            store::save(app, &store)?;
            Ok(Value::Null)
        }
        "pick_key" => {
            let path = rfd::AsyncFileDialog::new()
                .set_title("Choose SSH private key")
                .pick_file()
                .await;
            Ok(json!(path.map(|f| f.path().to_string_lossy().into_owned())))
        }
        "pick_wallpaper" => {
            use base64::Engine;
            let file = rfd::AsyncFileDialog::new()
                .add_filter("Wallpaper", &["png", "jpg", "jpeg", "webp"])
                .pick_file()
                .await;
            if let Some(file) = file {
                let data = tokio::fs::read(file.path()).await?;
                if data.len() > 8 * 1024 * 1024 {
                    bail!("Choose an image smaller than 8 MiB")
                }
                let ext = file
                    .path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let mime = match ext.as_str() {
                    "png" => "image/png",
                    "jpg" | "jpeg" => "image/jpeg",
                    "webp" => "image/webp",
                    _ => bail!("Unsupported image type"),
                };
                Ok(json!(format!(
                    "data:{mime};base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(data)
                )))
            } else {
                Ok(Value::Null)
            }
        }
        _ => bail!("Unknown application operation"),
    }
}
fn main() {
    let builder = tauri::Builder::default();
    #[cfg(windows)]
    let builder = builder.manage(local_downloads::LocalDownloads::default());
    builder
        .manage(browser::BrowserRegistry::default())
        .manage(download_request::RequestVault::default())
        .manage(AppState::default())
        .setup(|app| {
            #[cfg(windows)]
            local_downloads::clean_interrupted_cache(app.handle());
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    handle.state::<download_request::RequestVault>().purge();
                }
            });
            let loaded = store::load(app.handle())?;
            *app.state::<AppState>().store.blocking_lock() = loaded;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![rpc])
        .build(tauri::generate_context!())
        .expect("Failed to initialize SSH Virtual Desktop")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                let app = app.clone();
                tauri::async_runtime::block_on(async move {
                    app.state::<AppState>().ssh.disconnect().await;
                });
            }
        });
}
