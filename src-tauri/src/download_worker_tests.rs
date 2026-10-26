//! Regression coverage for backend output naming, atomic publication and private diagnostics.
use super::*;
use std::{
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

fn posix(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        format!("/{}{}", value[..1].to_lowercase(), &value[2..])
    } else {
        value
    }
}

struct WorkerFixture {
    base: PathBuf,
    dir: PathBuf,
    job: Job,
}
impl WorkerFixture {
    fn new() -> Result<Self> {
        let base = std::env::current_dir()?
            .join("target/worker-regressions")
            .join(uuid::Uuid::new_v4().to_string());
        let dir = base.join("private-job");
        std::fs::create_dir_all(&dir)?;
        let job = Job {
            id: uuid::Uuid::new_v4().to_string(),
            url: String::new(),
            directory: posix(&base),
            filename: "-résumé ' ;$(touch nope).txt".into(),
            backend: "fetch".into(),
            started: 0,
            status: "Waiting".into(),
            downloaded: 0,
            total: None,
            error: None,
            persistent: true,
            method: "GET".into(),
            authenticated: false,
            header_count: 0,
        };
        Ok(Self { base, dir, job })
    }

    fn partial(&self) -> Result<PathBuf> {
        Ok(self.base.join(partial_name(&self.job.id)?))
    }

    fn run(&self, backend_script: &str) -> Result<ExitStatus> {
        self.run_with_hook(backend_script, "")
    }

    fn run_with_hook(&self, backend_script: &str, hook: &str) -> Result<ExitStatus> {
        let shell = if cfg!(windows) {
            "C:/Program Files/Git/bin/sh.exe"
        } else {
            "/bin/sh"
        };
        anyhow::ensure!(Path::new(shell).is_file(), "A POSIX shell is required");
        std::fs::write(self.dir.join("fetch.sh"), backend_script)?;
        for file in ["request.conf", "request.body", "request.urls"] {
            std::fs::write(self.dir.join(file), "SYNTHETIC-REQUEST-SECRET")?;
        }
        std::fs::write(
            self.dir.join("run.sh"),
            format!("{hook}\n{}", worker(&self.job, &posix(&self.dir))?),
        )?;
        let mut command = Command::new(shell);
        command
            .arg(posix(&self.dir.join("run.sh")))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if cfg!(windows) {
            command.env(
                "PATH",
                format!(
                    "C:\\Program Files\\Git\\usr\\bin;C:\\Program Files\\Git\\mingw64\\bin;{}",
                    std::env::var("PATH").unwrap_or_default()
                ),
            );
        }
        let mut child = command.spawn()?;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                bail!("Regression worker exceeded its bounded execution deadline");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn metadata(&self, name: &str) -> Result<String> {
        Ok(std::fs::read_to_string(self.dir.join(name))?)
    }

    fn assert_private_cleanup(&self) -> Result<()> {
        for name in [
            "request.conf",
            "request.body",
            "request.urls",
            "fetch.sh",
            "backend.raw",
            "publication.raw",
        ] {
            assert!(
                !self.dir.join(name).exists(),
                "Retained private file: {name}"
            );
        }
        assert!(!self.partial()?.exists());
        assert!(!PathBuf::from(format!("{}.aria2", self.partial()?.display())).exists());
        assert!(!self.base.join("nope").exists());
        assert!(!self
            .base
            .join(format!("{}.svd-lock", self.job.filename))
            .exists());
        Ok(())
    }
}
impl Drop for WorkerFixture {
    fn drop(&mut self) {
        let Ok(base) = self.base.canonicalize() else {
            return;
        };
        let Ok(root) = std::env::current_dir()
            .and_then(|cwd| cwd.join("target/worker-regressions").canonicalize())
        else {
            return;
        };
        if base.starts_with(&root) && base != root {
            let _ = std::fs::remove_dir_all(base);
        }
    }
}

#[test]
fn aria2_input_sets_per_uri_partial_without_reencoding_the_url() -> Result<()> {
    let fixture = WorkerFixture::new()?;
    let id = &fixture.job.id;
    let url = "https://example.com/attachment?signature=a%2Fb&name=r%C3%A9sum%C3%A9";
    let input = url_input("aria2c", id, url)?;
    assert_eq!(&*input, &format!("{url}\n  out=.svd-{id}.part\n"));
    assert_eq!(&*url_input("wget", id, url)?, &format!("{url}\n"));
    for invalid in [
        "https://example.com/file\n  out=another-file",
        "https://example.com/file\r\n  dir=/tmp",
        "file:///tmp/secret",
    ] {
        assert!(url_input("aria2c", id, invalid).is_err());
    }
    assert!(url_input("aria2c", "../escaped\n  out=other", url).is_err());
    let mut job = fixture.job.clone();
    job.backend = "aria2c".into();
    let script = worker(&job, &posix(&fixture.dir))?;
    assert!(!script.contains(" --out="));
    assert!(script.contains("--input-file=\"$job/request.urls\""));
    assert!(script.contains(&quote(&posix(&fixture.partial()?))?));
    assert!(!script.contains(url));
    Ok(())
}

#[test]
fn fetch_script_uses_native_flags_and_quotes_the_exact_partial_path() -> Result<()> {
    let fixture = WorkerFixture::new()?;
    let mut job = fixture.job.clone();
    job.directory = "/tmp/-résumé ' folder;$(touch nope)".into();
    let url = "https://example.com/attachment-without-extension";
    assert_eq!(
        &*fetch_script(&job, url)?,
        &format!(
            "#!/bin/sh\nexec fetch -q -T 30 -N /dev/null -o {} {}\n",
            quote(&security::join(&job.directory, &partial_name(&job.id)?)?)?,
            quote(url)?
        )
    );
    Ok(())
}

#[test]
fn successful_backend_without_expected_partial_fails_before_publication() -> Result<()> {
    let fixture = WorkerFixture::new()?;
    assert!(!fixture.run("#!/bin/sh\nexit 0\n")?.success());
    assert_eq!(fixture.metadata("status")?.trim(), "Failed");
    assert_eq!(fixture.metadata("backend.exit")?.trim(), "0");
    assert!(fixture
        .metadata("error")?
        .contains("expected regular partial file is missing"));
    assert!(!fixture.base.join(&fixture.job.filename).exists());
    fixture.assert_private_cleanup()?;
    Ok(())
}

#[test]
fn backend_failure_preserves_exit_code_but_never_retains_secret_output() -> Result<()> {
    let fixture = WorkerFixture::new()?;
    let partial = posix(&fixture.partial()?);
    let backend = format!(
        "#!/bin/sh\nprintf '%s' partial > {}\nprintf '%s' control > {}\nprintf '%s\\n' 'SYNTHETIC-STDOUT-SECRET'\nprintf '%s' 'Permission denied https://example.com/?token=SYNTHETIC-STDERR-SECRET' >&2\nexit 7\n",
        quote(&partial)?, quote(&format!("{partial}.aria2"))?
    );
    assert!(!fixture.run(&backend)?.success());
    assert_eq!(fixture.metadata("status")?.trim(), "Failed");
    assert_eq!(fixture.metadata("backend.exit")?.trim(), "7");
    assert!(fixture
        .metadata("backend.stderr.safe")?
        .contains("Permission denied"));
    assert!(fixture.metadata("error")?.contains("fetch failed (exit 7)"));
    for entry in std::fs::read_dir(&fixture.dir)? {
        let entry = entry?;
        let bytes = std::fs::read(entry.path())?;
        let contents = String::from_utf8_lossy(&bytes);
        assert!(
            !contents.contains("SYNTHETIC-"),
            "Secret retained in job metadata"
        );
        assert!(!contents.contains("https://example.com"));
    }
    assert!(!fixture.base.join(&fixture.job.filename).exists());
    fixture.assert_private_cleanup()?;
    Ok(())
}

#[test]
fn target_created_during_download_is_never_overwritten_by_publication() -> Result<()> {
    let fixture = WorkerFixture::new()?;
    let backend = format!(
        "#!/bin/sh\nprintf '%s' downloaded > {}\nprintf '%s' existing > {}\nexit 0\n",
        quote(&posix(&fixture.partial()?))?,
        quote(&posix(&fixture.base.join(&fixture.job.filename)))?
    );
    assert!(!fixture.run(&backend)?.success());
    assert_eq!(fixture.metadata("status")?.trim(), "Failed");
    assert!(fixture
        .metadata("error")?
        .contains("Atomic publication failed"));
    assert_eq!(
        std::fs::read(fixture.base.join(&fixture.job.filename))?,
        b"existing"
    );
    fixture.assert_private_cleanup()?;
    Ok(())
}

#[test]
fn private_material_is_removed_before_a_terminal_state_becomes_visible() -> Result<()> {
    // Intercept the actual atomic status publication in the running shell. This
    // observes cleanup at the publication boundary without a polling race or sleep.
    let hook = r#"
mv() {
  if [ "$#" -eq 3 ] && [ "$2" = "$job/status.next" ] && [ "$3" = "$job/status" ]; then
    next=$(cat "$2")
    case "$next" in
      Completed|Failed|Cancelled)
        printf '%s\n' "$next" > "$job/cleanup-observed"
        for name in request.conf request.body request.urls fetch.sh backend.raw publication.raw; do
          if [ -e "$job/$name" ] || [ -L "$job/$name" ]; then
            printf '%s\n' "$name" >> "$job/cleanup-violation"
          fi
        done
        ;;
    esac
  fi
  command mv "$@"
}
"#;
    for terminal in ["Completed", "Failed", "Cancelled"] {
        let fixture = WorkerFixture::new()?;
        // Seed both raw files, including the cancellation path that never invokes
        // a downloader, so every terminal transition must remove private material.
        for name in ["backend.raw", "publication.raw"] {
            std::fs::write(fixture.dir.join(name), "SYNTHETIC-RAW-SECRET")?;
        }
        let backend = match terminal {
            "Completed" => format!(
                "#!/bin/sh\nprintf '%s' payload > {}\n",
                quote(&posix(&fixture.partial()?))?
            ),
            "Failed" => "#!/bin/sh\nexit 9\n".into(),
            "Cancelled" => {
                std::fs::write(fixture.dir.join("cancel"), "cancel")?;
                "#!/bin/sh\nexit 0\n".into()
            }
            _ => unreachable!(),
        };
        let exit = fixture.run_with_hook(&backend, hook)?;
        assert_eq!(exit.success(), terminal != "Failed");
        assert_eq!(fixture.metadata("cleanup-observed")?.trim(), terminal);
        assert_eq!(fixture.metadata("status")?.trim(), terminal);
        assert!(
            !fixture.dir.join("cleanup-violation").exists(),
            "Private material was visible when {terminal} was published: {}",
            std::fs::read_to_string(fixture.dir.join("cleanup-violation")).unwrap_or_default()
        );
        fixture.assert_private_cleanup()?;
    }
    Ok(())
}
