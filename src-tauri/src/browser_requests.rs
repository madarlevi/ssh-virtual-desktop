//! Native observation only: no injected script, response body reads or cookie DB export.
use crate::download_request::{allowed, ContextData, Header, RequestVault, MAX_BODY};
use tauri::Manager;
use webview2_com::{
    CoTaskMemPWSTR, Microsoft::Web::WebView2::Win32::*, WebResourceResponseReceivedEventHandler,
};
use windows::{
    core::{Interface, PWSTR},
    Win32::System::Com::{STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET},
};
use zeroize::Zeroizing;

unsafe fn text(
    read: impl FnOnce(*mut PWSTR) -> windows::core::Result<()>,
) -> windows::core::Result<String> {
    let mut p = PWSTR::null();
    let result = read(&mut p);
    let owned = CoTaskMemPWSTR::from(p);
    result?;
    Ok(owned.to_string())
}
pub async fn install(view: &tauri::Webview, browser: String) -> anyhow::Result<()> {
    let app = view.app_handle().clone();
    let (send, receive) = tokio::sync::oneshot::channel();
    view.with_webview(move |platform| unsafe {
        let install = (|| -> windows::core::Result<()> {
            let core = platform
                .controller()
                .CoreWebView2()?
                .cast::<ICoreWebView2_2>()?;
            let handler =
                WebResourceResponseReceivedEventHandler::create(Box::new(move |_, args| {
                    if let Some(args) = args {
                        let _ = capture(&app, &browser, &args);
                    }
                    Ok(())
                }));
            let mut token = 0;
            core.add_WebResourceResponseReceived(&handler, &mut token)?;
            Ok(())
        })();
        let _ = send.send(
            install.map_err(|_| anyhow::anyhow!("WebView2 request observation is unavailable")),
        );
    })?;
    tokio::time::timeout(std::time::Duration::from_secs(5), receive).await???;
    Ok(())
}
unsafe fn capture(
    app: &tauri::AppHandle,
    browser: &str,
    args: &ICoreWebView2WebResourceResponseReceivedEventArgs,
) -> windows::core::Result<()> {
    let response = args.Response()?;
    let rh = response.Headers()?;
    let mut disposition = None;
    let mut total = None;
    let mut encoded = false;
    let mut candidate = false;
    let it = rh.GetIterator()?;
    let mut more = windows::core::BOOL(0);
    it.HasCurrentHeader(&mut more)?;
    while more.as_bool() {
        let mut n = PWSTR::null();
        let mut v = PWSTR::null();
        it.GetCurrentHeader(&mut n, &mut v)?;
        let name = CoTaskMemPWSTR::from(n).to_string().to_ascii_lowercase();
        let value = CoTaskMemPWSTR::from(v).to_string();
        match name.as_str() {
            "content-disposition" => {
                candidate = true;
                disposition = Some(
                    value
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(1024)
                        .collect(),
                );
            }
            "content-type" => {
                candidate |= value.starts_with("application/")
                    && !value.starts_with("application/json")
                    && !value.contains("javascript");
            }
            "content-length" => total = value.parse::<u64>().ok(),
            "content-encoding" => encoded = !value.eq_ignore_ascii_case("identity"),
            _ => {}
        }
        it.MoveNext(&mut more)?;
    }
    if !candidate {
        return Ok(());
    }
    let request = args.Request()?;
    let url = text(|p| request.Uri(p))?;
    if crate::security::http_url(&url).is_err() {
        return Ok(());
    }
    let method = text(|p| request.Method(p))?;
    let mut headers = Vec::new();
    let it = request.Headers()?.GetIterator()?;
    it.HasCurrentHeader(&mut more)?;
    let mut size = 0;
    while more.as_bool() {
        let mut n = PWSTR::null();
        let mut v = PWSTR::null();
        it.GetCurrentHeader(&mut n, &mut v)?;
        let name = CoTaskMemPWSTR::from(n).to_string();
        let value = CoTaskMemPWSTR::from(v).to_string();
        size += name.len() + value.len();
        if size > 65536 || headers.len() >= 64 {
            return Ok(());
        }
        if allowed(&name) {
            let h = Header { name, value };
            if crate::download_request::validate_header(&h).is_ok() {
                headers.push(h);
            }
        }
        it.MoveNext(&mut more)?;
    }
    let mut body = None;
    let mut body_unavailable = method != "GET";
    if method == "POST" {
        if let Ok(stream) = request.Content().and_then(|s| s.Clone()) {
            let mut stat = STATSTG::default();
            if stream.Stat(&mut stat, STATFLAG_NONAME).is_ok()
                && stat.cbSize <= MAX_BODY as u64
                && stream.Seek(0, STREAM_SEEK_SET, None).is_ok()
            {
                let mut data = Zeroizing::new(vec![0u8; stat.cbSize as usize]);
                let mut offset = 0;
                while offset < data.len() {
                    let mut read = 0;
                    if stream
                        .Read(
                            data[offset..].as_mut_ptr().cast(),
                            (data.len() - offset) as u32,
                            Some(&mut read),
                        )
                        .is_err()
                        || read == 0
                    {
                        break;
                    }
                    offset += read as usize;
                }
                if offset == data.len() {
                    body = Some(data);
                    body_unavailable = false;
                }
            }
        }
    }
    app.state::<RequestVault>().capture(ContextData {
        browser: browser.into(),
        url,
        method,
        headers,
        body,
        body_unavailable,
        disposition,
        total: if encoded { None } else { total },
    });
    Ok(())
}
