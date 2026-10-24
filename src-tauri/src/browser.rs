use crate::{num, security, string};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Mutex};
use tauri::{
    webview::{PermissionResponse, WebviewBuilder},
    Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, WebviewUrl,
};
pub type Builder = WebviewBuilder<tauri::Wry>;
#[derive(Clone)]
struct Tab {
    owner: String,
    wanted: bool,
    shown: bool,
}
#[derive(Default)]
struct Inner {
    tabs: HashMap<String, Tab>,
    owner: Option<String>,
    blocked: bool,
}
#[derive(Default)]
pub struct BrowserRegistry(Mutex<Inner>);
impl Inner {
    fn visibility_changes(&mut self) -> Vec<(String, bool)> {
        let mut changes = Vec::new();
        for (id, tab) in &mut self.tabs {
            let visible = !self.blocked && self.owner.as_deref() == Some(&tab.owner) && tab.wanted;
            if visible != tab.shown {
                tab.shown = visible;
                changes.push((id.clone(), visible));
            }
        }
        changes
    }
}
fn label(id: &str) -> Result<String> {
    uuid::Uuid::parse_str(id)?;
    Ok(format!("browser-{id}"))
}
fn builder(app: &tauri::AppHandle, id: &str) -> Result<Builder> {
    let page = id.to_owned();
    let title = id.to_owned();
    Ok(WebviewBuilder::new(label(id)?,WebviewUrl::External("about:blank".parse()?))
 .data_directory(app.path().app_local_data_dir()?.join("browser-profile"))
 .on_navigation(|u|matches!(u.scheme(),"http"|"https"|"blob"|"data")||u.as_str()=="about:blank")
 .on_permission_request(|_,_|PermissionResponse::Deny)
 .on_document_title_changed(move|w,t|{let _=w.app_handle().emit_to(EventTarget::webview("main"),"browser-title",json!({"id":title,"title":t}));})
 .on_page_load(move|w,p|{let _=w.app_handle().emit_to(EventTarget::webview("main"),"browser-page",json!({"id":page,"url":p.url().as_str(),"loading":matches!(p.event(),tauri::webview::PageLoadEvent::Started)}));}))
}
pub async fn open(app: &tauri::AppHandle, v: &Value) -> Result<()> {
    if !cfg!(windows) {
        bail!("The native browser requires the Windows client")
    }
    let id = string(v, "id")?;
    let owner = string(v, "owner")?;
    uuid::Uuid::parse_str(owner)?;
    label(id)?;
    {
        let registry = app.state::<BrowserRegistry>();
        let mut state = registry.0.lock().unwrap();
        if state.tabs.contains_key(id) {
            bail!("Tab already exists")
        };
        if state.tabs.len() >= 64 || state.tabs.values().filter(|t| t.owner == owner).count() >= 24
        {
            bail!("Browser tab limit reached (24 per window)")
        };
        state.tabs.insert(
            id.into(),
            Tab {
                owner: owner.into(),
                wanted: false,
                shown: false,
            },
        );
    }
    let result = async {
        let mut build = builder(app, id)?;
        #[cfg(windows)]
        if let Some(parent) = v.get("popupParent").and_then(Value::as_str) {
            let parent = app
                .get_webview(&label(parent)?)
                .context("Popup opener closed")?;
            build = crate::native_browser::popup_builder(&parent, id.to_owned(), build).await?;
        }
        let child = app
            .get_window("main")
            .context("Desktop unavailable")?
            .add_child(
                build,
                LogicalPosition::new(0., 0.),
                LogicalSize::new(1., 1.),
            )?;
        child.hide()?;
        if !app
            .state::<BrowserRegistry>()
            .0
            .lock()
            .unwrap()
            .tabs
            .contains_key(id)
        {
            bail!("Tab was closed while opening")
        }
        #[cfg(windows)]
        {
            crate::browser_requests::install(&child, id.into()).await?;
            crate::native_browser::install(&child, id.into(), owner.into()).await?;
            if v.get("popupParent").is_some() {
                crate::native_browser::finish_popup(&child, id.into()).await?;
            }
        }
        if v.get("popupParent").is_none() {
            if let Some(url) = v
                .get("url")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                child.navigate(security::http_url(url)?)?;
            }
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;
    if result.is_err() {
        close(app, id);
    }
    result
}
fn apply_visibility(app: &tauri::AppHandle) {
    let list = {
        let r = app.state::<BrowserRegistry>();
        let mut s = r.0.lock().unwrap();
        s.visibility_changes()
    };
    for (id, show) in list {
        if let Ok(name) = label(&id) {
            if let Some(w) = app.get_webview(&name) {
                if show {
                    let _ = w.show();
                } else {
                    let _ = w.hide();
                }
            }
        }
    }
}
pub fn desktop(app: &tauri::AppHandle, v: &Value) -> Result<()> {
    let owner = v.get("owner").and_then(Value::as_str);
    if let Some(owner) = owner {
        uuid::Uuid::parse_str(owner)?;
    }
    {
        let r = app.state::<BrowserRegistry>();
        let mut s = r.0.lock().unwrap();
        s.owner = owner.map(str::to_owned);
        s.blocked = v.get("blocked").and_then(Value::as_bool).unwrap_or(true);
    }
    apply_visibility(app);
    Ok(())
}
pub fn bounds(app: &tauri::AppHandle, v: &Value) -> Result<()> {
    let id = string(v, "id")?;
    let w = app.get_webview(&label(id)?).context("Tab has closed")?;
    let visible = v.get("visible").and_then(Value::as_bool).unwrap_or(false);
    {
        let r = app.state::<BrowserRegistry>();
        let mut s = r.0.lock().unwrap();
        let owner = s.tabs.get(id).context("Unknown tab")?.owner.clone();
        for t in s.tabs.values_mut().filter(|t| t.owner == owner) {
            t.wanted = false;
        }
        s.tabs.get_mut(id).unwrap().wanted = visible;
    }
    if visible {
        let x = v.get("x").and_then(Value::as_f64).unwrap_or(0.).max(0.);
        let y = v.get("y").and_then(Value::as_f64).unwrap_or(0.).max(0.);
        w.set_position(LogicalPosition::new(x, y))?;
        w.set_size(LogicalSize::new(
            num(v, "width", 1).clamp(1, 20000) as f64,
            num(v, "height", 1).clamp(1, 20000) as f64,
        ))?;
    }
    apply_visibility(app);
    Ok(())
}
pub fn action(app: &tauri::AppHandle, v: &Value) -> Result<()> {
    let id = string(v, "id")?;
    label(id)?;
    if string(v, "action")? == "close" {
        close(app, id);
        return Ok(());
    }
    let w = app.get_webview(&label(id)?).context("Tab has closed")?;
    match string(v, "action")? {
        "navigate" => w.navigate(security::http_url(string(v, "url")?)?)?,
        "reload" => w.reload()?,
        "focus" => w.set_focus()?,
        "close" => close(app, id),
        "hide" => {
            if let Some(t) = app
                .state::<BrowserRegistry>()
                .0
                .lock()
                .unwrap()
                .tabs
                .get_mut(id)
            {
                t.wanted = false;
            }
            apply_visibility(app);
        }
        "back" | "forward" | "stop" => {
            let action = string(v, "action")?.to_owned();
            #[cfg(windows)]
            w.with_webview(move |p| unsafe {
                if let Ok(c) = p.controller().CoreWebView2() {
                    let _ = match action.as_str() {
                        "back" => c.GoBack(),
                        "forward" => c.GoForward(),
                        _ => c.Stop(),
                    };
                }
            })?;
        }
        _ => bail!("Unsupported browser operation"),
    };
    Ok(())
}
pub fn close(app: &tauri::AppHandle, id: &str) {
    app.state::<BrowserRegistry>()
        .0
        .lock()
        .unwrap()
        .tabs
        .remove(id);
    if let Ok(name) = label(id) {
        if let Some(w) = app.get_webview(&name) {
            #[cfg(windows)]
            crate::native_browser::cleanup(&w, id.to_owned());
            let _ = w.close();
        } else {
            #[cfg(windows)]
            if let Some(w) = app.get_webview("main") {
                crate::native_browser::cleanup(&w, id.to_owned());
            }
        }
    }
}
pub fn close_all(app: &tauri::AppHandle) {
    let ids = app
        .state::<BrowserRegistry>()
        .0
        .lock()
        .unwrap()
        .tabs
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    for id in ids {
        close(app, &id);
    }
}
pub fn download(
    app: tauri::AppHandle,
    id: String,
    url: String,
    suggested: String,
    disposition: Option<String>,
    mime: Option<String>,
) {
    let supported = security::http_url(&url).is_ok();
    let filename = crate::download_name::suggested(
        disposition.as_deref(),
        Some(&suggested),
        &url,
        mime.as_deref(),
    );
    tauri::async_runtime::spawn(async move {
        let mut offer = None;
        for _ in 0..20 {
            if app.get_webview(&format!("browser-{id}")).is_none() {
                return;
            }
            offer = app
                .state::<crate::download_request::RequestVault>()
                .offer(&id, &url);
            if offer.is_some() || !supported {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let _=app.emit_to(EventTarget::webview("main"),"download-request",json!({"browserId":id,"url":url,"filename":filename,"supported":supported,"offerId":offer}));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hidden_tabs_do_not_receive_repeated_native_visibility_updates() {
        let mut state = Inner {
            owner: Some("owner".into()),
            ..Default::default()
        };
        for i in 0..24 {
            state.tabs.insert(
                i.to_string(),
                Tab {
                    owner: "owner".into(),
                    wanted: i == 0,
                    shown: false,
                },
            );
        }
        assert_eq!(state.visibility_changes(), vec![("0".into(), true)]);
        assert!(state.visibility_changes().is_empty());
        state.blocked = true;
        assert_eq!(state.visibility_changes(), vec![("0".into(), false)]);
        assert!(state.visibility_changes().is_empty());
        state.tabs.get_mut("0").unwrap().wanted = false;
        state.tabs.get_mut("1").unwrap().wanted = true;
        state.blocked = false;
        assert_eq!(state.visibility_changes(), vec![("1".into(), true)]);
        state.owner = None;
        assert_eq!(state.visibility_changes(), vec![("1".into(), false)]);
    }
}
