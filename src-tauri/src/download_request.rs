//! Request credentials stay in native RAM; the desktop receives only summaries.
use crate::security;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::{Duration, Instant},
};
use zeroize::{Zeroize, Zeroizing};

pub const MAX_BODY: usize = 1024 * 1024;
#[derive(Clone, Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}
impl Drop for Header {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}
pub fn sensitive(name: &str) -> bool {
    !matches!(
        name.to_ascii_lowercase().as_str(),
        "accept" | "accept-language" | "user-agent" | "content-type" | "origin"
    )
}
pub fn allowed(name: &str) -> bool {
    !matches!(
        name.to_ascii_lowercase().as_str(),
        "host"
            | "connection"
            | "content-length"
            | "transfer-encoding"
            | "accept-encoding"
            | "proxy-authorization"
            | "proxy-connection"
            | "range"
            | "if-range"
            | "if-match"
            | "if-none-match"
            | "if-modified-since"
            | "if-unmodified-since"
            | "upgrade"
            | "expect"
            | "te"
            | "trailer"
    ) && !name.to_ascii_lowercase().starts_with("sec-")
}
pub fn validate_header(h: &Header) -> Result<()> {
    if h.name.is_empty()
        || h.name.len() > 128
        || !h
            .name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
        || h.value.len() > 8192
        || h.value.chars().any(char::is_control)
    {
        bail!("Invalid HTTP header name or value (control characters are forbidden)")
    }
    if !allowed(&h.name) {
        bail!("A browser-managed or backend-managed header cannot be forwarded")
    }
    Ok(())
}
#[derive(Clone)]
pub struct ContextData {
    pub browser: String,
    pub url: String,
    pub method: String,
    pub headers: Vec<Header>,
    pub body: Option<Zeroizing<Vec<u8>>>,
    pub body_unavailable: bool,
    pub disposition: Option<String>,
    pub total: Option<u64>,
}
struct Timed {
    data: ContextData,
    at: Instant,
}
#[derive(Default)]
pub struct RequestVault {
    recent: Mutex<VecDeque<Timed>>,
    offers: Mutex<HashMap<String, Timed>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub method: String,
    pub captured: bool,
    pub header_names: Vec<String>,
    pub cookie_count: usize,
    pub authentication_detected: bool,
    pub body_bytes: usize,
    pub body_unavailable: bool,
    pub content_disposition: Option<String>,
    pub total: Option<u64>,
}
impl RequestVault {
    pub fn purge(&self) {
        self.recent
            .lock()
            .unwrap()
            .retain(|t| t.at.elapsed() < Duration::from_secs(5));
        self.offers
            .lock()
            .unwrap()
            .retain(|_, t| t.at.elapsed() < Duration::from_secs(600));
    }
    pub fn capture(&self, data: ContextData) {
        let mut recent = self.recent.lock().unwrap();
        recent.retain(|t| t.at.elapsed() < Duration::from_secs(5));
        if recent.len() >= 32 {
            recent.pop_front();
        }
        recent.push_back(Timed {
            data,
            at: Instant::now(),
        });
    }
    pub fn offer(&self, browser: &str, url: &str) -> Option<String> {
        let mut recent = self.recent.lock().unwrap();
        recent.retain(|t| t.at.elapsed() < Duration::from_secs(5));
        if recent
            .iter()
            .filter(|t| t.data.browser == browser && t.data.url == url)
            .count()
            != 1
        {
            return None;
        }
        let i = recent
            .iter()
            .rposition(|t| t.data.browser == browser && t.data.url == url)?;
        let timed = recent.remove(i)?;
        let mut offers = self.offers.lock().unwrap();
        offers.retain(|_, t| t.at.elapsed() < Duration::from_secs(600));
        if offers.len() >= 32 {
            offers.clear();
        }
        let id = uuid::Uuid::new_v4().to_string();
        offers.insert(
            id.clone(),
            Timed {
                at: Instant::now(),
                ..timed
            },
        );
        Some(id)
    }
    fn context(&self, id: &str, url: &str) -> Result<ContextData> {
        let mut offers = self.offers.lock().unwrap();
        offers.retain(|_, t| t.at.elapsed() < Duration::from_secs(600));
        let t = offers.get(id).context(
            "Browser request expired. Click the download again to capture a fresh request.",
        )?;
        if t.data.url != url {
            bail!("Browser request does not match this URL. Clear the captured request before editing the URL.")
        }
        Ok(t.data.clone())
    }
    pub fn discard(&self, id: &str) {
        self.offers.lock().unwrap().remove(id);
    }
    pub fn clear(&self) {
        self.recent.lock().unwrap().clear();
        self.offers.lock().unwrap().clear();
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Input {
    pub url: String,
    #[serde(default)]
    pub offer_id: Option<String>,
    #[serde(default = "get")]
    pub method: String,
    #[serde(default)]
    pub headers: Vec<Header>,
    pub body: Option<String>,
    #[serde(default)]
    pub allow_authentication: bool,
    #[serde(default)]
    pub browser_request: bool,
    #[serde(default)]
    pub confirm_direct: bool,
}
fn get() -> String {
    "GET".into()
}
pub struct Resolved {
    pub url: String,
    pub method: String,
    pub headers: Vec<Header>,
    pub body: Option<Zeroizing<Vec<u8>>>,
    pub authenticated: bool,
    pub total: Option<u64>,
}
impl Input {
    fn data(&self, vault: &RequestVault) -> Result<ContextData> {
        let url = security::http_url(&self.url)?.to_string();
        if let Some(id) = &self.offer_id {
            return vault.context(id, &url);
        }
        if self.headers.len() > 64
            || self
                .headers
                .iter()
                .map(|h| h.name.len() + h.value.len())
                .sum::<usize>()
                > 65536
        {
            bail!("Request headers exceed the 64 KiB limit")
        }
        for h in &self.headers {
            validate_header(h)?;
        }
        if self.body.as_ref().is_some_and(|b| b.len() > MAX_BODY) {
            bail!("Request body exceeds 1 MiB")
        }
        Ok(ContextData {
            browser: String::new(),
            url,
            method: self.method.to_uppercase(),
            headers: self.headers.clone(),
            body: self
                .body
                .as_ref()
                .map(|b| Zeroizing::new(b.as_bytes().to_vec())),
            body_unavailable: false,
            disposition: None,
            total: None,
        })
    }
    pub fn summary(&self, vault: &RequestVault) -> Result<Summary> {
        let c = self.data(vault)?;
        Ok(Summary {
            method: c.method,
            captured: self.offer_id.is_some(),
            header_names: c.headers.iter().map(|h| h.name.clone()).collect(),
            cookie_count: c
                .headers
                .iter()
                .filter(|h| h.name.eq_ignore_ascii_case("cookie"))
                .map(|h| h.value.split(';').filter(|v| !v.trim().is_empty()).count())
                .sum(),
            authentication_detected: c.headers.iter().any(|h| sensitive(&h.name))
                || c.body.is_some(),
            body_bytes: c.body.as_ref().map_or(0, |b| b.len()),
            body_unavailable: c.body_unavailable,
            content_disposition: c.disposition,
            total: c.total,
        })
    }
    pub fn resolve(&self, vault: &RequestVault) -> Result<Resolved> {
        if self.browser_request && self.offer_id.is_none() && !self.confirm_direct {
            bail!("Request metadata was unavailable. Explicitly confirm a direct GET URL or recreate the request manually.")
        }
        let c = self.data(vault)?;
        if !matches!(c.method.as_str(), "GET" | "POST") {
            bail!("Only GET and POST downloads are supported")
        }
        if c.body_unavailable {
            bail!("WebView2 did not expose a complete replayable request body. Recreate the POST explicitly or use a direct download URL; it will not be replayed as GET.")
        }
        if c.method == "GET" && c.body.is_some() {
            bail!("GET request bodies are not supported")
        }
        if c.method == "POST" && !self.allow_authentication {
            bail!("Approve forwarding the POST body before sending it to the server")
        }
        let headers: Vec<_> = c
            .headers
            .into_iter()
            .filter(|h| self.allow_authentication || !sensitive(&h.name))
            .collect();
        for h in &headers {
            validate_header(h)?;
        }
        let authenticated = headers.iter().any(|h| sensitive(&h.name)) || c.body.is_some();
        if authenticated && !c.url.starts_with("https://") {
            bail!("Forwarded credentials and POST bodies require HTTPS")
        }
        Ok(Resolved {
            url: c.url,
            method: c.method,
            headers,
            body: c.body,
            authenticated,
            total: c.total,
        })
    }
}
/// curl configuration is a different serialization language from shell quoting.
pub fn config_word(value: &str) -> Result<String> {
    if value.chars().any(char::is_control) {
        bail!("Control characters cannot appear in curl configuration")
    }
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> Input {
        serde_json::from_value(serde_json::json!({"url":"https://example.com/a"})).unwrap()
    }
    #[test]
    fn consent_is_per_request_and_url_bound() {
        let v = RequestVault::default();
        v.capture(ContextData {
            browser: "b".into(),
            url: "https://example.com/a".into(),
            method: get(),
            headers: vec![Header {
                name: "Cookie".into(),
                value: "session=test-only".into(),
            }],
            body: None,
            body_unavailable: false,
            disposition: None,
            total: None,
        });
        let id = v.offer("b", "https://example.com/a").unwrap();
        let mut i = input();
        i.offer_id = Some(id.clone());
        assert_eq!(i.summary(&v).unwrap().cookie_count, 1);
        assert!(i.resolve(&v).unwrap().headers.is_empty());
        i.allow_authentication = true;
        assert!(i.resolve(&v).unwrap().authenticated);
        i.url = "https://other.example/a".into();
        assert!(i.resolve(&v).is_err());
        v.discard(&id);
        i.url = "https://example.com/a".into();
        assert!(i.resolve(&v).is_err());
    }
    #[test]
    fn headers_and_config_cannot_inject() {
        for (name, value) in [
            ("Authorization", "token\r\nX: injected"),
            ("Host", "evil"),
            ("Accept-Encoding", "gzip"),
            ("Sec-Fetch-Site", "same-origin"),
        ] {
            assert!(validate_header(&Header {
                name: name.into(),
                value: value.into()
            })
            .is_err());
        }
        assert_eq!(config_word("a\\b\"c").unwrap(), "\"a\\\\b\\\"c\"");
        assert!(config_word("x\nurl=evil").is_err());
    }
    #[test]
    fn post_without_body_capture_is_never_changed_to_get() {
        let mut i = input();
        i.method = "POST".into();
        i.body = Some("test=fixture".into());
        assert!(i.resolve(&RequestVault::default()).is_err());
        i.allow_authentication = true;
        assert_eq!(i.resolve(&RequestVault::default()).unwrap().method, "POST");
    }
    #[test]
    fn ambiguous_or_stale_browser_requests_are_not_guessed() {
        let v = RequestVault::default();
        let context = ContextData {
            browser: "b".into(),
            url: "https://example.com/a".into(),
            method: "POST".into(),
            headers: vec![],
            body: None,
            body_unavailable: true,
            disposition: None,
            total: None,
        };
        v.capture(context.clone());
        v.capture(context.clone());
        assert!(v.offer("b", "https://example.com/a").is_none());
        v.clear();
        v.capture(context);
        v.recent.lock().unwrap()[0].at = Instant::now() - Duration::from_secs(6);
        assert!(v.offer("b", "https://example.com/a").is_none());
    }
    #[test]
    fn unavailable_post_stream_is_rejected_even_with_approval() {
        let v = RequestVault::default();
        v.capture(ContextData {
            browser: "b".into(),
            url: "https://example.com/a".into(),
            method: "POST".into(),
            headers: vec![],
            body: None,
            body_unavailable: true,
            disposition: None,
            total: None,
        });
        let mut i = input();
        i.offer_id = v.offer("b", "https://example.com/a");
        i.allow_authentication = true;
        assert!(i
            .resolve(&v)
            .err()
            .unwrap()
            .to_string()
            .contains("complete replayable request body"));
    }
}
