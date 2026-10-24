fn decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if i + 2 >= b.len() {
                return None;
            }
            out.push((b[i + 1] as char).to_digit(16).and_then(|h| {
                (b[i + 2] as char)
                    .to_digit(16)
                    .map(|l| ((h << 4) | l) as u8)
            })?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}
fn safe(s: &str) -> Option<String> {
    let s = s.rsplit(['/', '\\']).next()?.trim();
    let mut name = String::new();
    for c in s.chars().filter(|c| !c.is_control()) {
        if name.len() + c.len_utf8() > 240 {
            break;
        }
        name.push(c);
    }
    if name.is_empty() || name == "." || name == ".." {
        None
    } else {
        Some(name)
    }
}
pub fn suggested(
    disposition: Option<&str>,
    suggestion: Option<&str>,
    url: &str,
    mime: Option<&str>,
) -> String {
    if let Some(header) = disposition {
        let mut parts = Vec::new();
        let mut part = String::new();
        let mut quoted = false;
        let mut escape = false;
        for c in header.chars() {
            if escape {
                part.push(c);
                escape = false;
            } else if c == '\\' && quoted {
                escape = true;
            } else if c == '"' {
                quoted = !quoted;
            } else if c == ';' && !quoted {
                parts.push(std::mem::take(&mut part));
            } else {
                part.push(c);
            }
        }
        parts.push(part);
        for extended in [true, false] {
            for p in &parts {
                if let Some((key, value)) = p.trim().split_once('=') {
                    if key.trim().eq_ignore_ascii_case(if extended {
                        "filename*"
                    } else {
                        "filename"
                    }) {
                        let decoded = if extended {
                            value
                                .trim()
                                .split_once("''")
                                .filter(|(charset, _)| charset.eq_ignore_ascii_case("utf-8"))
                                .and_then(|(_, value)| decode(value))
                        } else {
                            Some(value.trim().to_owned())
                        };
                        if let Some(name) = decoded.and_then(|v| safe(&v)) {
                            return name;
                        }
                    }
                }
            }
        }
    }
    if let Some(name) = suggestion.and_then(safe) {
        return name;
    }
    if let Some(name) = url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.path_segments()
                .and_then(|mut s| s.next_back())
                .and_then(decode)
        })
        .and_then(|s| safe(&s))
    {
        return name;
    }
    format!(
        "download{}",
        match mime.unwrap_or("").split(';').next().unwrap_or("").trim() {
            "application/pdf" => ".pdf",
            "application/zip" => ".zip",
            "text/plain" => ".txt",
            "application/json" => ".json",
            "image/png" => ".png",
            _ => ".bin",
        }
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filenames_prefer_headers_and_never_use_query_or_paths() {
        assert_eq!(
            suggested(
                Some("attachment; filename=old.txt; filename*=UTF-8''r%C3%A9sum%C3%A9%20%2724.txt"),
                Some("browser"),
                "https://example.com/?token=secret",
                None
            ),
            "résumé '24.txt"
        );
        assert_eq!(
            suggested(
                Some("attachment; filename=\"../../a;b.txt\""),
                None,
                "https://e.com/",
                None
            ),
            "a;b.txt"
        );
        assert_eq!(
            suggested(None, None, "https://e.com/a%20b?x=secret", None),
            "a b"
        );
        assert_eq!(
            suggested(None, None, "blob:https://e.com/id", Some("application/pdf")),
            "download.pdf"
        );
        assert!(safe("-file.txt").is_some());
        assert!(decode("%é").is_none());
        assert!(decode("bad%xy").is_none());
    }
}
