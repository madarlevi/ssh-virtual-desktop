//! WebView2 callbacks stay on the UI COM apartment. Websites never receive IPC bindings.
use anyhow::{anyhow, Result};
use base64::Engine;
use serde_json::json;
use std::{cell::RefCell, collections::HashMap};
use tauri::{Emitter, EventTarget, Manager};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::{
    core::{Interface, PCWSTR, PWSTR},
    Win32::{
        System::Com::{STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET},
        UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT},
    },
};

struct Popup {
    parent: String,
    args: ICoreWebView2NewWindowRequestedEventArgs,
    deferral: ICoreWebView2Deferral,
    environment: ICoreWebView2Environment,
}
struct Hooks {
    core: ICoreWebView2,
    controller: ICoreWebView2Controller,
    download: i64,
    popup: i64,
    keyboard: i64,
    favicon: Option<i64>,
}
struct LocalOffer {
    parent: String,
    args: Option<ICoreWebView2DownloadStartingEventArgs>,
    deferral: Option<ICoreWebView2Deferral>,
    operation: ICoreWebView2DownloadOperation,
}
thread_local! {
    static POPUPS: RefCell<HashMap<String,Popup>> = RefCell::new(HashMap::new());
    static HOOKS: RefCell<HashMap<String,Hooks>> = RefCell::new(HashMap::new());
    static LOCAL: RefCell<HashMap<String,LocalOffer>> = RefCell::new(HashMap::new());
}
unsafe fn text(read: impl FnOnce(*mut PWSTR) -> windows::core::Result<()>) -> String {
    let mut value = PWSTR::null();
    let result = read(&mut value);
    let value = CoTaskMemPWSTR::from(value);
    if result.is_ok() {
        value.to_string()
    } else {
        String::new()
    }
}
fn emit(app: &tauri::AppHandle, event: &str, value: serde_json::Value) {
    let _ = app.emit_to(EventTarget::webview("main"), event, value);
}

pub async fn install(view: &tauri::Webview, id: String, owner: String) -> Result<()> {
    let app = view.app_handle().clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_webview(move|p|unsafe {
        let result=(||->windows::core::Result<()> {
            let controller=p.controller();let core=controller.CoreWebView2()?;let four=core.cast::<ICoreWebView2_4>()?;
            // Native Save Page/Image As menu entries bypass download interception on some runtimes.
            // Disable that menu; Ctrl+S routes through our explicit remote workflow.
            core.Settings()?.SetAreDefaultContextMenusEnabled(false)?;
            let app_download=app.clone();let download_id=id.clone();let mut download=0;
            four.add_DownloadStarting(&DownloadStartingEventHandler::create(Box::new(move|_,args|{
                if let Some(args)=args {
                    // Both flags are necessary: Cancel prevents bytes being saved; Handled suppresses Save As.
                    args.SetCancel(true)?;args.SetHandled(true)?;
                    if let Ok(op)=args.DownloadOperation(){
                        let url=text(|v|op.Uri(v));let filename=text(|v|args.ResultFilePath(v));
                        let disposition=text(|v|op.ContentDisposition(v));let mime=text(|v|op.MimeType(v));
                        if (url.starts_with("blob:")||url.starts_with("data:"))&&LOCAL.with(|m|m.borrow().len()<20) {
                            let token=uuid::Uuid::new_v4().to_string();let deferral=args.GetDeferral()?;
                            let filename=crate::download_name::suggested(Some(&disposition),Some(&filename),"",Some(&mime));
                            LOCAL.with(|m|m.borrow_mut().insert(token.clone(),LocalOffer{parent:download_id.clone(),args:Some(args),deferral:Some(deferral),operation:op}));
                            emit(&app_download,"download-request",json!({"browserId":download_id,"url":"Browser-generated file","filename":filename,"supported":false,"localToken":token}));return Ok(());
                        }
                        args.SetCancel(true)?;
                        crate::browser::download(app_download.clone(),download_id.clone(),url,filename,Some(disposition),Some(mime));
                    }else{args.SetCancel(true)?;}
                }Ok(())
            })),&mut download)?;
            let app_popup=app.clone();let parent=id.clone();let popup_owner=owner.clone();let environment=p.environment();let mut popup=0;
            core.add_NewWindowRequested(&NewWindowRequestedEventHandler::create(Box::new(move|_,args|{
                if let Some(args)=args {
                    args.SetHandled(true)?;
                    let url=text(|v|args.Uri(v));
                    if url!="about:blank"&&!url.starts_with("blob:")&&crate::security::http_url(&url).is_err(){return Ok(());}
                    let count=POPUPS.with(|m|m.borrow().len());if count>=24{return Ok(());}
                    let deferral=args.GetDeferral()?;let token=uuid::Uuid::new_v4().to_string();
                    POPUPS.with(|m|m.borrow_mut().insert(token.clone(),Popup{parent:parent.clone(),args,deferral,environment:environment.clone()}));
                    emit(&app_popup,"browser-new-tab",json!({"owner":popup_owner,"parent":parent,"id":token,"url":url}));
                }Ok(())
            })),&mut popup)?;
            let app_keys=app.clone();let keys_id=id.clone();let keys_owner=owner;let mut keyboard=0;
            controller.add_AcceleratorKeyPressed(&AcceleratorKeyPressedEventHandler::create(Box::new(move|_,args|{
                if let Some(args)=args {
                    let mut kind=COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN;args.KeyEventKind(&mut kind)?;
                    if kind!=COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN&&kind!=COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN{return Ok(());}
                    let mut key=0;args.VirtualKey(&mut key)?;
                    let ctrl=GetKeyState(VK_CONTROL.0 as i32)<0;let shift=GetKeyState(VK_SHIFT.0 as i32)<0;let alt=GetKeyState(VK_MENU.0 as i32)<0;
                    if let Some(action)=shortcut(key,ctrl,shift,alt){args.SetHandled(true)?;if let Some(main)=app_keys.get_webview("main"){let _=main.set_focus();}emit(&app_keys,"browser-shortcut",json!({"owner":keys_owner,"id":keys_id,"action":action}));}
                }Ok(())
            })),&mut keyboard)?;
            let favicon=if let Ok(fifteen)=core.cast::<ICoreWebView2_15>() {
                let app_icon=app;let icon_id=id.clone();let mut token=0;
                fifteen.add_FaviconChanged(&FaviconChangedEventHandler::create(Box::new(move|sender,_|{
                    if let Some(sender)=sender {if let Ok(c)=sender.cast::<ICoreWebView2_15>() {
                        let app=app_icon.clone();let id=icon_id.clone();
                        c.GetFavicon(COREWEBVIEW2_FAVICON_IMAGE_FORMAT_PNG,&GetFaviconCompletedHandler::create(Box::new(move|result,stream|{
                            if result.is_ok(){if let Some(stream)=stream {
                                let mut stat=STATSTG::default();
                                if stream.Stat(&mut stat,STATFLAG_NONAME).is_ok()&&stat.cbSize>0&&stat.cbSize<=262144&&stream.Seek(0,STREAM_SEEK_SET,None).is_ok(){
                                    let mut data=vec![0u8;stat.cbSize as usize];let mut read=0;
                                    if stream.Read(data.as_mut_ptr().cast(),data.len() as u32,Some(&mut read)).is_ok()&&read as usize==data.len(){emit(&app,"browser-favicon",json!({"id":id,"data":format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(data))}));}
                                }
                            }}Ok(())
                        })))?;
                    }}Ok(())
                })),&mut token)?;Some(token)
            }else{None};
            HOOKS.with(|m|m.borrow_mut().insert(id,Hooks{core,controller,download,popup,keyboard,favicon}));Ok(())
        })();let _=tx.send(result.map_err(|_|anyhow!("Native WebView2 download interception could not be installed")));
    })?;
    tokio::time::timeout(std::time::Duration::from_secs(10), rx).await???;
    Ok(())
}
pub fn shortcut(key: u32, ctrl: bool, shift: bool, alt: bool) -> Option<String> {
    if alt && !ctrl {
        return match key {
            37 => Some("back".into()),
            39 => Some("forward".into()),
            _ => None,
        };
    }
    if !ctrl || alt {
        return None;
    }
    Some(match key {
        84 => "new".into(),
        87 => "close".into(),
        9 => if shift { "previous" } else { "next" }.into(),
        49..=57 => format!("tab-{}", key - 49),
        76 => "address".into(),
        82 => "reload".into(),
        74 => "downloads".into(),
        70 => "find".into(),
        83 => "save".into(),
        _ => return None,
    })
}
pub async fn popup_builder(
    parent: &tauri::Webview,
    token: String,
    builder: crate::browser::Builder,
) -> Result<crate::browser::Builder> {
    let parent_id = parent.label().trim_start_matches("browser-").to_owned();
    let (tx, rx) = tokio::sync::oneshot::channel();
    parent.with_webview(move |_| {
        let environment = POPUPS.with(|m| {
            m.borrow()
                .get(&token)
                .filter(|p| p.parent == parent_id)
                .map(|p| p.environment.clone())
        });
        let _ = tx.send(
            environment
                .map(|e| builder.with_environment(e))
                .ok_or_else(|| anyhow!("Popup expired or opener closed")),
        );
    })?;
    tokio::time::timeout(std::time::Duration::from_secs(10), rx).await??
}
pub async fn finish_popup(child: &tauri::Webview, token: String) -> Result<()> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    child.with_webview(move |p| unsafe {
        let result = POPUPS
            .with(|m| m.borrow_mut().remove(&token))
            .ok_or_else(|| anyhow!("Popup expired"))
            .and_then(|popup| {
                let result = p
                    .controller()
                    .CoreWebView2()
                    .and_then(|c| popup.args.SetNewWindow(&c));
                let _ = popup.deferral.Complete();
                result.map_err(|_| anyhow!("WebView2 could not attach the popup tab"))
            });
        let _ = tx.send(result);
    })?;
    tokio::time::timeout(std::time::Duration::from_secs(10), rx).await???;
    Ok(())
}
pub fn cleanup(view: &tauri::Webview, id: String) {
    let _ = view.with_webview(move |_| unsafe {
        if let Some(h) = HOOKS.with(|m| m.borrow_mut().remove(&id)) {
            if let Ok(c) = h.core.cast::<ICoreWebView2_4>() {
                let _ = c.remove_DownloadStarting(h.download);
            }
            let _ = h.core.remove_NewWindowRequested(h.popup);
            let _ = h.controller.remove_AcceleratorKeyPressed(h.keyboard);
            if let (Ok(c), Some(t)) = (h.core.cast::<ICoreWebView2_15>(), h.favicon) {
                let _ = c.remove_FaviconChanged(t);
            }
        }
        POPUPS.with(|m| {
            m.borrow_mut().retain(|token, p| {
                if p.parent == id || token == &id {
                    let _ = p.deferral.Complete();
                    false
                } else {
                    true
                }
            })
        });
        LOCAL.with(|m| {
            m.borrow_mut().retain(|_, offer| {
                if offer.parent == id {
                    if let Some(args) = &offer.args {
                        let _ = args.SetCancel(true);
                    }
                    if let Some(d) = &offer.deferral {
                        let _ = d.Complete();
                    }
                    let _ = offer.operation.Cancel();
                    false
                } else {
                    true
                }
            })
        });
    });
}
pub async fn allow_local(
    view: &tauri::Webview,
    token: String,
    path: std::path::PathBuf,
) -> Result<()> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_webview(move |_| unsafe {
        let result = LOCAL.with(|m| {
            let mut m = m.borrow_mut();
            let offer = m
                .get_mut(&token)
                .ok_or_else(|| anyhow!("Browser file offer expired; reopen the source"))?;
            let args = offer
                .args
                .take()
                .ok_or_else(|| anyhow!("Browser transfer already started"))?;
            let deferral = offer.deferral.take();
            let path: Vec<u16> = path
                .to_string_lossy()
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let result = args
                .SetResultFilePath(PCWSTR(path.as_ptr()))
                .and_then(|_| args.SetCancel(false));
            if result.is_err() {
                let _ = args.SetCancel(true);
            }
            if let Some(d) = deferral {
                let _ = d.Complete();
            }
            result.map_err(|_| anyhow!("WebView2 could not start the approved client transfer"))
        });
        let _ = tx.send(result);
    })?;
    tokio::time::timeout(std::time::Duration::from_secs(10), rx).await???;
    Ok(())
}
pub async fn local_state(
    view: &tauri::Webview,
    token: String,
) -> Result<(String, u64, Option<u64>)> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_webview(move |_| unsafe {
        let result = LOCAL.with(|m| {
            let m = m.borrow();
            let o = m.get(&token).ok_or_else(|| anyhow!("Browser tab closed"))?;
            let mut state = COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS;
            let mut bytes = 0;
            let mut total = 0;
            o.operation.State(&mut state)?;
            o.operation.BytesReceived(&mut bytes)?;
            o.operation.TotalBytesToReceive(&mut total)?;
            Ok::<_, anyhow::Error>((
                if state == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED {
                    "completed"
                } else if state == COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED {
                    "interrupted"
                } else {
                    "active"
                }
                .to_owned(),
                bytes.max(0) as u64,
                (total > 0).then_some(total as u64),
            ))
        });
        let _ = tx.send(result);
    })?;
    tokio::time::timeout(std::time::Duration::from_secs(10), rx).await??
}
pub async fn discard_local(view: &tauri::Webview, token: String) -> Result<()> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_webview(move |_| unsafe {
        LOCAL.with(|m| {
            if let Some(o) = m.borrow_mut().remove(&token) {
                if let Some(args) = o.args {
                    let _ = args.SetCancel(true);
                }
                if let Some(d) = o.deferral {
                    let _ = d.Complete();
                }
                let _ = o.operation.Cancel();
            }
        });
        let _ = tx.send(());
    })?;
    tokio::time::timeout(std::time::Duration::from_secs(10), rx).await??;
    Ok(())
}
pub async fn find(view: &tauri::Webview, term: String, action: String) -> Result<()> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_webview(move |p| unsafe {
        let result = (|| -> windows::core::Result<()> {
            let find = p
                .controller()
                .CoreWebView2()?
                .cast::<ICoreWebView2_28>()?
                .Find()?;
            match action.as_str() {
                "stop" => find.Stop(),
                "next" => find.FindNext(),
                "previous" => find.FindPrevious(),
                _ => {
                    find.Stop()?;
                    let options = p
                        .environment()
                        .cast::<ICoreWebView2Environment15>()?
                        .CreateFindOptions()?;
                    let term: Vec<u16> = term.encode_utf16().chain(Some(0)).collect();
                    options.SetFindTerm(PCWSTR(term.as_ptr()))?;
                    options.SetShouldHighlightAllMatches(true)?;
                    find.Start(
                        &options,
                        &FindStartCompletedHandler::create(Box::new(|_| Ok(()))),
                    )
                }
            }
        })();
        let _ =
            tx.send(result.map_err(|_| anyhow!("Find on page requires a newer WebView2 runtime")));
    })?;
    tokio::time::timeout(std::time::Duration::from_secs(10), rx).await???;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_shortcuts_do_not_capture_plain_terminal_keys() {
        assert_eq!(shortcut(84, true, false, false).as_deref(), Some("new"));
        assert_eq!(shortcut(9, true, true, false).as_deref(), Some("previous"));
        assert_eq!(shortcut(84, false, false, false), None);
    }
}
