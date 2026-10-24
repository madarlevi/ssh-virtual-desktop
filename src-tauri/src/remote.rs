use crate::security::{path, quote};
use anyhow::{bail, Result};
use serde::Serialize;
pub async fn choose_directory<F, Fut>(
    home: &str,
    previous: Option<&str>,
    mut validate: F,
) -> Result<String>
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = Option<String>>,
{
    for candidate in previous.into_iter().chain([home, "/tmp", "/var/tmp", "/"]) {
        if path(candidate).is_err() {
            continue;
        }
        if let Some(canonical) = validate(candidate.to_owned()).await {
            path(&canonical)?;
            return Ok(canonical);
        }
    }
    bail!("No accessible remote directory found. Check SFTP permissions.")
}

#[derive(Clone, Default, Debug, PartialEq, Serialize)]
pub enum RemotePlatform {
    FreeBSD,
    Linux,
    #[default]
    Other,
}
#[derive(Clone, Default, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCapabilities {
    pub platform: RemotePlatform,
    pub tools: Vec<String>,
}
pub const DETECT:&str="uname -s; for x in aria2c curl wget fetch daemon setsid nohup sh wc link mv cp mkdir date sleep rm cat rmdir; do command -v \"$x\" >/dev/null 2>&1 && printf '%s\\n' \"$x\"; done; if ! command -v daemon >/dev/null 2>&1 && test -x /usr/sbin/daemon; then printf 'daemon\\n'; fi; :";
impl RemoteCapabilities {
    pub fn parse(output: &str) -> Self {
        let mut lines = output.lines();
        let platform = match lines.next().unwrap_or("").trim() {
            "FreeBSD" => RemotePlatform::FreeBSD,
            "Linux" => RemotePlatform::Linux,
            _ => RemotePlatform::Other,
        };
        Self {
            platform,
            tools: lines
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
        }
    }
    pub fn has(&self, tool: &str) -> bool {
        self.tools.iter().any(|t| t == tool)
    }
    pub fn launch(&self, script: &str) -> Result<String> {
        path(script)?;
        let script = quote(script)?;
        if self.platform == RemotePlatform::FreeBSD && self.has("daemon") {
            return Ok(format!("umask 077; /usr/sbin/daemon -f /bin/sh {script}"));
        }
        if self.has("setsid") && self.has("nohup") {
            return Ok(format!(
                "umask 077; nohup setsid /bin/sh {script} </dev/null >/dev/null 2>&1 &"
            ));
        }
        bail!("Detached jobs need FreeBSD daemon(8), or setsid and nohup. No packages were installed.")
    }
    pub fn copy(&self, source: &str, destination: &str) -> Result<String> {
        path(source)?;
        path(destination)?;
        let options = match self.platform {
            RemotePlatform::FreeBSD => "-R -P -p -n",
            RemotePlatform::Linux => "-a -n",
            RemotePlatform::Other => bail!(
                "Server-side copy is supported on FreeBSD and Linux; this platform is unrecognized"
            ),
        };
        Ok(format!(
            "cp {options} {} {}",
            quote(source)?,
            quote(destination)?
        ))
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backend {
    pub name: &'static str,
    pub headers: bool,
    pub cookies: bool,
    pub post: bool,
    pub redirects: bool,
    pub resume: bool,
    pub progress: &'static str,
}
pub fn backend_info(name: &'static str) -> Backend {
    let advanced = name == "curl";
    Backend {
        name,
        headers: advanced,
        cookies: advanced,
        post: advanced,
        redirects: true,
        resume: false,
        progress: "SFTP file bytes",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn remembered_directory_is_validated_before_home_and_fallbacks() {
        let home = "/usr/home/test";
        assert_eq!(
            choose_directory(home, Some("/root/downloads"), |p| async move {
                (p == "/root/downloads").then_some(p)
            })
            .await
            .unwrap(),
            "/root/downloads"
        );
        assert_eq!(
            choose_directory(home, Some("/missing-or-unreadable"), |p| async move {
                (p == home).then_some(p)
            })
            .await
            .unwrap(),
            home
        );
        assert_eq!(
            choose_directory(home, Some("/../unsafe"), |p| async move {
                (p == "/tmp").then_some(p)
            })
            .await
            .unwrap(),
            "/tmp"
        );
        assert!(choose_directory(home, None, |_| async { None })
            .await
            .is_err());
    }
    #[test]
    fn os_and_tools_are_detected_without_linux_files() {
        let b = RemoteCapabilities::parse("FreeBSD\r\nfetch\ndaemon\nlink\n");
        assert_eq!(b.platform, RemotePlatform::FreeBSD);
        assert!(b.has("fetch"));
        assert!(!b.has("curl"));
        assert_eq!(
            RemoteCapabilities::parse("Linux\ncurl\n").platform,
            RemotePlatform::Linux
        );
        assert!(!DETECT.contains("/etc/os-release"));
    }
    #[test]
    fn freebsd_detach_and_copy_have_no_gnu_flags() {
        let b = RemoteCapabilities::parse("FreeBSD\ndaemon\ncp\n");
        let launch = b.launch("/usr/home/test/a ' ;/job.sh").unwrap();
        assert!(launch.contains("/usr/sbin/daemon -f /bin/sh"));
        assert!(!launch.contains("setsid"));
        let copy = b.copy("/tmp/-file", "/tmp/résumé '24").unwrap();
        assert!(copy.starts_with("cp -R -P -p -n "));
        assert!(!copy.contains(" --"));
        assert!(b.copy("-option", "/tmp/a").is_err());
    }
    #[test]
    fn linux_detach_stays_supported() {
        let p = RemoteCapabilities::parse("Linux\nnohup\nsetsid\n");
        assert!(p
            .launch("/tmp/job/run.sh")
            .unwrap()
            .contains("nohup setsid /bin/sh"));
        assert!(RemoteCapabilities::default().launch("/tmp/j").is_err());
    }
}
