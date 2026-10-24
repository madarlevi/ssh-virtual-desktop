//! Controlled HTTP fixtures use synthetic session values only; no browser or SSH account.
use super::*;
use crate::download_request::Header;
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn authenticated_get_post_and_redirect_do_not_leak_or_retain_credentials() {
    let shell = if cfg!(windows) {
        "C:/Program Files/Git/bin/sh.exe"
    } else {
        "/bin/sh"
    };
    if !std::path::Path::new(shell).exists() {
        return;
    }
    let base = std::env::current_dir()
        .unwrap()
        .join("target/auth-worker-tests")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&base).unwrap();
    let posix = |p: &std::path::Path| {
        let s = p.to_string_lossy().replace('\\', "/");
        if cfg!(windows) {
            format!("/{}{}", s[..1].to_lowercase(), &s[2..])
        } else {
            s
        }
    };
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    server.set_nonblocking(true).unwrap();
    let address = server.local_addr().unwrap();
    let sink = TcpListener::bind("127.0.0.1:0").unwrap();
    sink.set_nonblocking(true).unwrap();
    let sink_address = sink.local_addr().unwrap();
    let endpoint = std::thread::spawn(move || {
        let mut requests = vec![];
        let begin = Instant::now();
        while requests.len() < 3 && begin.elapsed() < Duration::from_secs(15) {
            let Ok((mut stream, _)) = server.accept() else {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut data = vec![];
            loop {
                let mut b = [0; 4096];
                let n = stream.read(&mut b).unwrap();
                if n == 0 {
                    break;
                }
                data.extend_from_slice(&b[..n]);
                if let Some(i) = data.windows(4).position(|v| v == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&data[..i]);
                    let len = header
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if data.len() >= i + 4 + len {
                        break;
                    }
                }
                assert!(data.len() < 65536);
            }
            let text = String::from_utf8(data).unwrap();
            let number = requests.len();
            requests.push(text);
            if number == 2 {
                write!(stream,"HTTP/1.1 302 Found\r\nLocation: http://{sink_address}/must-not-receive-session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            } else {
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 18\r\nConnection: close\r\n\r\ncontrolled payload").unwrap();
            }
        }
        requests
    });
    for (i, method) in ["GET", "POST", "GET"].into_iter().enumerate() {
        let job = Job {
            id: uuid::Uuid::new_v4().to_string(),
            url: format!("http://{address}/fixture-{i}"),
            directory: posix(&base),
            filename: format!("fixture-{i}.bin"),
            backend: "curl".into(),
            started: 0,
            status: "Starting".into(),
            downloaded: 0,
            total: None,
            error: None,
            persistent: true,
            method: method.into(),
            authenticated: true,
            header_count: 2,
        };
        let dir = base.join(&job.id);
        std::fs::create_dir(&dir).unwrap();
        let r = Resolved {
            url: job.url.clone(),
            method: method.into(),
            headers: vec![
                Header {
                    name: "Authorization".into(),
                    value: "Bearer SYNTHETIC-FIXTURE-TOKEN".into(),
                },
                Header {
                    name: "Cookie".into(),
                    value: "fixture_session=NOT-A-REAL-SESSION".into(),
                },
            ],
            body: (method == "POST")
                .then(|| zeroize::Zeroizing::new(b"test=synthetic-body".to_vec())),
            authenticated: true,
            total: None,
        };
        let mut config = curl_config(&job, &posix(&dir), &r).unwrap().to_string();
        if cfg!(windows) {
            config = config.replace(&posix(&base), &base.to_string_lossy().replace('\\', "/"));
        }
        std::fs::write(dir.join("request.conf"), config).unwrap();
        if let Some(body) = &r.body {
            std::fs::write(dir.join("request.body"), body).unwrap();
        }
        let script = worker(&job, &posix(&dir)).unwrap();
        assert!(!script.contains("SYNTHETIC-FIXTURE-TOKEN"));
        assert!(!script.contains("NOT-A-REAL-SESSION"));
        std::fs::write(dir.join("run.sh"), script).unwrap();
        let mut cmd = Command::new(shell);
        cmd.arg(posix(&dir.join("run.sh")))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if cfg!(windows) {
            cmd.env(
                "PATH",
                format!(
                    "C:\\Program Files\\Git\\usr\\bin;C:\\Program Files\\Git\\mingw64\\bin;{}",
                    std::env::var("PATH").unwrap_or_default()
                ),
            );
        }
        let mut child = cmd.spawn().unwrap();
        let start = Instant::now();
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if start.elapsed() > Duration::from_secs(15) {
                let _ = child.kill();
                panic!("Authenticated worker timed out");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let status = std::fs::read_to_string(dir.join("status")).unwrap();
        assert_eq!(status.trim(), if i == 2 { "Failed" } else { "Completed" });
        for secret in ["request.conf", "request.body", "request.urls"] {
            assert!(
                !dir.join(secret).exists(),
                "temporary credential file retained"
            );
        }
        if i < 2 {
            assert_eq!(
                std::fs::read(base.join(&job.filename)).unwrap(),
                b"controlled payload"
            );
        } else {
            assert!(std::fs::read_to_string(dir.join("error"))
                .unwrap()
                .contains("Redirect not followed"));
            assert!(
                sink.accept().is_err(),
                "another origin received the authenticated redirect"
            );
        }
    }
    let requests = endpoint.join().unwrap();
    assert_eq!(requests.len(), 3);
    for r in &requests {
        assert!(r.contains("Authorization: Bearer SYNTHETIC-FIXTURE-TOKEN"));
        assert!(r.contains("Cookie: fixture_session=NOT-A-REAL-SESSION"));
        assert!(r.contains("Accept-Encoding: identity"));
    }
    assert!(requests[1].starts_with("POST "));
    assert!(requests[1].ends_with("test=synthetic-body"));
}

#[test]
fn active_cancellation_uses_private_marker_and_portable_base_options() {
    let shell = if cfg!(windows) {
        "C:/Program Files/Git/bin/sh.exe"
    } else {
        "/bin/sh"
    };
    if !std::path::Path::new(shell).exists() {
        return;
    }
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/cancel-tests")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&base).unwrap();
    let posix = |p: &std::path::Path| {
        let s = p.to_string_lossy().replace('\\', "/");
        if cfg!(windows) {
            format!("/{}{}", s[..1].to_lowercase(), &s[2..])
        } else {
            s
        }
    };
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    server.set_nonblocking(true).unwrap();
    let address = server.local_addr().unwrap();
    let endpoint = std::thread::spawn(move || {
        let start = Instant::now();
        loop {
            if let Ok((mut stream, _)) = server.accept() {
                let mut b = [0; 4096];
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let _ = stream.read(&mut b);
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 819200\r\nConnection: close\r\n\r\n",
                );
                for _ in 0..200 {
                    if stream.write_all(&[b'x'; 4096]).is_err() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                return;
            }
            if start.elapsed() > Duration::from_secs(10) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    let job = Job {
        id: uuid::Uuid::new_v4().to_string(),
        url: format!("http://{address}/slow"),
        directory: posix(&base),
        filename: "-cancel résumé.txt".into(),
        backend: "curl".into(),
        started: 0,
        status: "Starting".into(),
        downloaded: 0,
        total: None,
        error: None,
        persistent: true,
        method: "GET".into(),
        authenticated: false,
        header_count: 0,
    };
    let dir = base.join(&job.id);
    std::fs::create_dir(&dir).unwrap();
    let request = Resolved {
        url: job.url.clone(),
        method: "GET".into(),
        headers: vec![],
        body: None,
        authenticated: false,
        total: None,
    };
    let mut config = curl_config(&job, &posix(&dir), &request)
        .unwrap()
        .to_string();
    if cfg!(windows) {
        config = config.replace(&posix(&base), &base.to_string_lossy().replace('\\', "/"));
    }
    std::fs::write(dir.join("request.conf"), config).unwrap();
    // Reject the exact GNU-only base-utility syntax that broke the FreeBSD worker.
    let shims = base.join("shims");
    std::fs::create_dir(&shims).unwrap();
    for command in ["chmod", "mv", "rm", "mkdir", "link"] {
        let script=format!("#!/bin/sh\nfor arg do case \"$arg\" in --|-T|-fT|--*) printf 'GNU-only argument rejected\\n' >&2; exit 98;; esac; done\nexec /usr/bin/{command} \"$@\"\n");
        let file = shims.join(command);
        std::fs::write(&file, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
    let script = worker(&job, &posix(&dir)).unwrap();
    assert!(!script.contains("chmod"));
    std::fs::write(dir.join("run.sh"), script).unwrap();
    let mut cmd = Command::new(shell);
    cmd.arg(posix(&dir.join("run.sh")))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if cfg!(windows) {
        cmd.env(
            "PATH",
            format!(
                "{};C:\\Program Files\\Git\\usr\\bin;C:\\Program Files\\Git\\mingw64\\bin;{}",
                shims.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        );
    } else {
        cmd.env(
            "PATH",
            format!(
                "{}:{}",
                shims.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        );
    }
    let partial = base.join(format!(".svd-{}.part", job.id));
    let mut child = cmd.spawn().unwrap();
    let start = Instant::now();
    while std::fs::metadata(&partial).map(|m| m.len()).unwrap_or(0) == 0 {
        if start.elapsed() > Duration::from_secs(10) || child.try_wait().unwrap().is_some() {
            let _ = child.kill();
            panic!("No live download bytes were observed");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    std::fs::write(dir.join("cancel"), b"cancel\n").unwrap();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(15) {
            let _ = child.kill();
            panic!("Cancellation did not terminate the downloader");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    endpoint.join().unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("status")).unwrap().trim(),
        "Cancelled"
    );
    assert!(!partial.exists());
    assert!(!base.join(&job.filename).exists());
    assert!(!dir.join("request.conf").exists());
    let resolved = base.canonicalize().unwrap();
    assert!(resolved.starts_with(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/cancel-tests")
            .canonicalize()
            .unwrap()
    ));
    std::fs::remove_dir_all(resolved).unwrap();
}
