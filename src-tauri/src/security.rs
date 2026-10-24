use anyhow::{bail, Result};

/// A POSIX shell word, including apostrophes and newlines. SSH exec is NOT argv.
pub fn quote(s: &str) -> Result<String> {
    if s.contains('\0') {
        bail!("NUL is not allowed in shell arguments")
    }
    Ok(format!("'{}'", s.replace('\'', "'\"'\"'")))
}
pub fn filename(s: &str) -> Result<()> {
    if s.is_empty()
        || s == "."
        || s == ".."
        || s.contains('/')
        || s.contains('\0')
        || s.chars().any(char::is_control)
        || s.len() > 240
    {
        bail!("Use a filename of 1–240 bytes without slashes or control characters")
    }
    Ok(())
}
pub fn path(s: &str) -> Result<()> {
    if !s.starts_with('/') || s.len() > 4096 || s.contains('\0') || s.split('/').any(|p| p == "..")
    {
        bail!("An absolute remote path without '..' is required")
    }
    Ok(())
}
pub fn http_url(s: &str) -> Result<url::Url> {
    if s.len() > 8192 || s.chars().any(char::is_control) {
        bail!("URL is too long or contains control characters")
    }
    let u = url::Url::parse(s)?;
    if !matches!(u.scheme(), "http" | "https")
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
    {
        bail!("Only HTTP/HTTPS URLs without embedded credentials can be sent to the server. Blob/data URLs cannot be delegated.")
    }
    Ok(u)
}
pub fn join(dir: &str, name: &str) -> Result<String> {
    path(dir)?;
    filename(name)?;
    Ok(format!("{}/{}", dir.trim_end_matches('/'), name))
}
pub fn ipc_origin(label: &str, url: &url::Url, development: bool) -> bool {
    if label != "main" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    (url.scheme() == "tauri" && url.host_str() == Some("localhost") && url.port().is_none())
        || (url.scheme() == "http"
            && url.host_str() == Some("tauri.localhost")
            && url.port().is_none())
        || (development
            && url.scheme() == "http"
            && url.host_str() == Some("127.0.0.1")
            && url.port() == Some(1420))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shell_words() {
        assert_eq!(
            quote("a'b;$(touch /tmp/pwn)\n").unwrap(),
            "'a'\"'\"'b;$(touch /tmp/pwn)\n'"
        );
        assert!(quote("a\0b").is_err());
    }
    #[test]
    fn destinations() {
        for n in ["..", "", "a/b", "\nfile"] {
            assert!(filename(n).is_err());
        }
        assert!(join("/tmp", "résumé '24.txt").is_ok());
        assert!(path("/tmp/../etc").is_err());
        assert!(http_url("file:///etc/passwd").is_err());
        assert!(http_url("https://user:secret@example.com/a").is_err());
        assert!(http_url("https://example.com/a?x=1&y=';foo").is_ok());
    }
    #[test]
    fn shell_quoting_round_trips_without_expansion() {
        let sh = if cfg!(windows) {
            "C:/Program Files/Git/bin/sh.exe"
        } else {
            "/bin/sh"
        };
        if !std::path::Path::new(sh).exists() {
            return;
        }
        for value in [
            "",
            "résumé '24.txt",
            "a;$(printf injected) & `printf unsafe`",
            "line one\nline two",
            "--help",
        ] {
            let out = std::process::Command::new(sh)
                .arg("-c")
                .arg(format!("printf '%s' {}", quote(value).unwrap()))
                .output()
                .unwrap();
            assert!(out.status.success());
            assert_eq!(out.stdout, value.as_bytes());
        }
    }
    #[test]
    fn remote_webviews_never_receive_desktop_authority() {
        let local = url::Url::parse("http://tauri.localhost/index.html").unwrap();
        assert!(ipc_origin("main", &local, false));
        assert!(!ipc_origin("browser-123", &local, false));
        for origin in [
            "https://example.com",
            "http://tauri.localhost.evil/",
            "http://127.0.0.1:1420/",
            "file:///C:/index.html",
        ] {
            assert!(!ipc_origin(
                "main",
                &url::Url::parse(origin).unwrap(),
                false
            ));
        }
        let dev = url::Url::parse("http://127.0.0.1:1420/").unwrap();
        assert!(ipc_origin("main", &dev, true));
        assert!(!ipc_origin("browser-123", &dev, true));
    }
}
