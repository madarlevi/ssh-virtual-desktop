use crate::security::{path, quote};
use anyhow::Result;
pub fn command(cwd: Option<&str>) -> Result<String> {
    // sshd invokes the account's shell with -c. Explicitly select sh for the
    // launcher so fish/tcsh accounts can still exec their own default shell.
    Ok(format!("/bin/sh -c {}", quote(&launcher(cwd)?)?))
}
fn launcher(cwd: Option<&str>) -> Result<String> {
    let cd = if let Some(cwd) = cwd {
        path(cwd)?;
        format!("cd {} || exit; ", quote(cwd)?)
    } else {
        String::new()
    };
    let script = quote(include_str!("terminal.bash"))?;
    Ok(format!(
        r#"{cd}case "${{SHELL##*/}}" in
bash)
  if "$SHELL" -c '(( BASH_VERSINFO[0] > 4 || (BASH_VERSINFO[0] == 4 && BASH_VERSINFO[1] >= 4) ))' 2>/dev/null; then
    svd_rc=$(umask 077; mktemp "${{TMPDIR:-/tmp}}/svd-terminal.XXXXXXXX") || exec "$SHELL" -l
    if ! printf '%s\n' {script} > "$svd_rc"; then rm -f "$svd_rc"; exec "$SHELL" -l; fi
    export SVD_TERMINAL_RC="$svd_rc"
    exec "$SHELL" --noprofile --rcfile "$svd_rc" -i
  fi
  ;;
esac
exec "${{SHELL:-/bin/sh}}" -l"#
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cwd_is_serialized_as_one_word() {
        let script = launcher(Some("/tmp/a'b;$(touch nope)")).unwrap();
        assert!(script.starts_with("cd '/tmp/a'\"'\"'b;$(touch nope)' || exit;"));
        assert!(command(Some("/tmp/../etc")).is_err());
    }
    #[test]
    fn bash_emits_boundaries_around_real_commands() {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let bash = if cfg!(windows) {
            "C:/Program Files/Git/bin/bash.exe"
        } else {
            "/bin/bash"
        };
        if !std::path::Path::new(bash).exists() {
            return;
        }
        let root = std::env::current_dir()
            .unwrap()
            .join("target/terminal-tests")
            .join(format!("{} a'b;$(printf nope)", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root_path = root.to_string_lossy().replace('\\', "/");
        let posix = if cfg!(windows) {
            format!("/{}{}", root_path[..1].to_lowercase(), &root_path[2..])
        } else {
            root_path
        };
        let launch = command(Some(&posix)).unwrap();
        let mut child = Command::new(bash)
            .args(["-c", &launch])
            .env("SHELL", "/bin/bash")
            .env("TMPDIR", &posix)
            .env("SVD_TERMINAL_TEST", "1")
            .env("PROMPT_COMMAND", "printf 'USER-PROMPT-HOOK\\n'")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"printf 'REAL-SHELL-RESULT\\n'\ncd /\nprintf 'SECOND-COMMAND\\n'\nexit\n")
            .unwrap();
        let result = child.wait_with_output().unwrap();
        let mut bytes = result.stdout;
        bytes.extend(result.stderr);
        let output = String::from_utf8_lossy(&bytes);
        assert!(result.status.success(), "{output}");
        assert!(output.contains("REAL-SHELL-RESULT\n"));
        assert!(output.contains("USER-PROMPT-HOOK\n"));
        assert!(output.contains("\u{1b}]133;B\u{7}"));
        assert!(output.contains("\u{1b}]133;C\u{7}"));
        assert!(output.contains("\u{1b}]777;svd-cwd;/\u{7}"));
        assert!(output.contains(&format!("\u{1b}]777;svd-cwd;{posix}\u{7}")));
        assert!(output.contains("38;2;113;221;162m"));
        // The session startup file self-deletes, and metacharacters in cwd did not execute.
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        std::fs::remove_dir(root).unwrap();
    }
}
