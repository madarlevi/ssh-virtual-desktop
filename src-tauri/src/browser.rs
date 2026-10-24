use crate::{num, security, string};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use tauri::{
    webview::{DownloadEvent, NewWindowResponse, PermissionResponse, WebviewBuilder},
    Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, WebviewUrl,
};

fn label(v: &Value) -> Result<String> {
    let id = string(v, "id")?;
    uuid::Uuid::parse_str(id)?;
    Ok(format!("browser-{id}"))
}
pub async fn open(app: &tauri::AppHandle, v: &Value) -> Result<()> {
    let label = label(v)?;
    if app.get_webview(&label).is_some() {
        bail!("Browser already open")
    }
    let url = security::http_url(string(v, "url")?)?;
    let id = string(v, "id")?.to_string();
    let event_id = id.clone();
    let page_id = id.clone();
    let title_id = id.clone();
    let builder = WebviewBuilder::new(&label, WebviewUrl::External(url))
        .on_navigation(|url| matches!(url.scheme(), "http" | "https"))
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_permission_request(|_, _| PermissionResponse::Deny)
        .on_document_title_changed(move |w, title| {
            let _ = w.app_handle().emit_to(EventTarget::webview("main"), "browser-title", json!({"id": title_id, "title": title}));
        })
        .on_page_load(move |w, payload| {
            let _ = w.app_handle().emit_to(EventTarget::webview("main"), "browser-page", json!({"id":page_id,"url":payload.url().as_str(),"loading":matches!(payload.event(), tauri::webview::PageLoadEvent::Started)}));
        })
        .on_download(move |w, event| {
            if let DownloadEvent::Requested { url, destination } = event {
                let filename = destination.file_name().map(|s|s.to_string_lossy().into_owned()).unwrap_or_else(|| "download".into());
                let supported = security::http_url(url.as_str()).is_ok();
                let app=w.app_handle().clone();let browser_id=event_id.clone();let url=url.to_string();
                // ResponseReceived and DownloadStarting are separate native events. Cancel
                // local saving now, then briefly await the matching metadata event.
                tauri::async_runtime::spawn(async move {
                    let mut offer=None;
                    for _ in 0..20 {
                        if app.get_webview(&format!("browser-{browser_id}")).is_none(){return;}
                        offer=app.state::<crate::download_request::RequestVault>().offer(&browser_id,&url);
                        if offer.is_some()||!supported {break;}
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                    let _=app.emit_to(EventTarget::webview("main"),"download-request",json!({"browserId":browser_id,"url":url,"filename":filename,"supported":supported,"offerId":offer}));
                });
            }
            // WebView2 cancels before any file body is saved on Windows.
            false
        });
    let window = app
        .get_window("main")
        .context("Desktop window unavailable")?;
    let child = window.add_child(
        builder,
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(1.0, 1.0),
    )?;
    child.hide()?;
    #[cfg(windows)]
    crate::browser_requests::install(&child, id).await?;
    Ok(())
}
pub fn bounds(app: &tauri::AppHandle, v: &Value) -> Result<()> {
    let w = app.get_webview(&label(v)?).context("Browser has closed")?;
    if !v.get("visible").and_then(Value::as_bool).unwrap_or(false) {
        w.hide()?;
        return Ok(());
    }
    let x = v.get("x").and_then(Value::as_f64).unwrap_or(0.).max(0.);
    let y = v.get("y").and_then(Value::as_f64).unwrap_or(0.).max(0.);
    let width = num(v, "width", 1).clamp(1, 20000) as f64;
    let height = num(v, "height", 1).clamp(1, 20000) as f64;
    w.set_position(LogicalPosition::new(x, y))?;
    w.set_size(LogicalSize::new(width, height))?;
    w.show()?;
    Ok(())
}
pub fn action(app: &tauri::AppHandle, v: &Value) -> Result<()> {
    let w = app.get_webview(&label(v)?).context("Browser has closed")?;
    match string(v, "action")? {
        "navigate" => w.navigate(security::http_url(string(v, "url")?)?)?,
        "reload" => w.reload()?,
        "focus" => w.set_focus()?,
        "close" => w.close()?,
        "hide" => w.hide()?,
        "back" | "forward" | "stop" => {
            let action = string(v, "action")?.to_string();
            #[cfg(target_os = "windows")]
            w.with_webview(move |platform| unsafe {
                if let Ok(core) = platform.controller().CoreWebView2() {
                    let _ = match action.as_str() {
                        "back" => core.GoBack(),
                        "forward" => core.GoForward(),
                        _ => core.Stop(),
                    };
                }
            })?;
            #[cfg(not(target_os = "windows"))]
            {
                let _ = action;
                bail!("Native browser controls require Windows");
            }
        }
        _ => bail!("Unsupported browser operation"),
    }
    Ok(())
}
pub fn close_all(app: &tauri::AppHandle) {
    for (name, w) in app.webviews() {
        if name.starts_with("browser-") {
            let _ = w.close();
        }
    }
}
