//! Turning a `.desktop` `Exec` line into a detached child process.

use std::path::Path;
use std::process::{Command, Stdio};

/// The field codes of the Desktop Entry spec (`%f %F %u %U` file/URL lists
/// plus the deprecated `%d %D %n %N %v %m` and `%i %c %k`). The bar launches
/// apps without a file argument, so all of them are stripped.
const FIELD_CODES: [char; 13] = [
    'f', 'F', 'u', 'U', 'd', 'D', 'n', 'N', 'i', 'c', 'k', 'v', 'm',
];

/// Splits an `Exec` value into argv: minimal spec unquoting (double quotes
/// with `\"` `` \` `` `\$` `\\` escapes), `%%` → `%`, field codes stripped.
/// Tokens that were only a field code disappear entirely.
pub(crate) fn exec_to_argv(exec: &str) -> Vec<String> {
    let mut argv = Vec::new();
    let mut chars = exec.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.peek().is_none() {
            break;
        }
        let mut token = String::new();
        let mut in_quotes = false;
        while let Some(c) = chars.next() {
            match c {
                '"' => in_quotes = !in_quotes,
                '\\' if in_quotes => {
                    // Inside quotes the spec escapes `"` `` ` `` `$` `\`.
                    if let Some(escaped) = chars.next() {
                        token.push(escaped);
                    }
                }
                '%' => match chars.peek() {
                    Some('%') => {
                        chars.next();
                        token.push('%');
                    }
                    Some(code) if FIELD_CODES.contains(code) => {
                        chars.next(); // strip the field code
                    }
                    _ => token.push('%'),
                },
                c if c.is_whitespace() && !in_quotes => break,
                c => token.push(c),
            }
        }
        if !token.is_empty() {
            argv.push(token);
        }
    }
    argv
}

/// Spawns `argv` fully detached: own process group (survives smabar
/// exiting — deliberately no kill-on-drop), null stdio, `cwd` as working
/// directory. A reaper thread waits on the child so it never lingers as a
/// zombie while the bar keeps running.
pub(crate) fn spawn_detached(argv: &[String], cwd: &Path) -> std::io::Result<u32> {
    let (program, args) = argv.split_first().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "empty command line")
    })?;
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    crate::platform::detach_into_own_process_group(&mut command);
    crate::platform::render::scrub_child_env(&mut command);
    let mut child = command.spawn()?;
    let pid = child.id();
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_field_codes_and_keeps_plain_arguments() {
        assert_eq!(exec_to_argv("firefox %u"), vec!["firefox"]);
        assert_eq!(
            exec_to_argv("code --new-window %F"),
            vec!["code", "--new-window"]
        );
        assert_eq!(
            exec_to_argv("app %f %F %u %U %d %D %n %N %i %c %k %v %m"),
            vec!["app"]
        );
        // Embedded field codes vanish; literal %% survives as %.
        assert_eq!(exec_to_argv("app --url=%U"), vec!["app", "--url="]);
        assert_eq!(exec_to_argv("app 100%%"), vec!["app", "100%"]);
        // Unknown percent sequences pass through untouched.
        assert_eq!(exec_to_argv("app 50%x"), vec!["app", "50%x"]);
    }

    #[test]
    fn unquotes_double_quoted_arguments() {
        assert_eq!(
            exec_to_argv(r#"/opt/my app/run "with space" plain"#),
            vec!["/opt/my", "app/run", "with space", "plain"]
        );
        assert_eq!(
            exec_to_argv(r#""quoted \"inner\" \\ arg""#),
            vec![r#"quoted "inner" \ arg"#]
        );
        assert_eq!(exec_to_argv("   "), Vec::<String>::new());
    }

    #[test]
    fn spawn_detached_launches_and_reports_a_missing_program() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pid = spawn_detached(&["cargo".to_string(), "--version".to_string()], dir.path())
            .expect("spawn cargo");
        assert!(pid > 0);

        let err = spawn_detached(
            &["definitely-not-a-real-binary-xyz".to_string()],
            dir.path(),
        )
        .expect_err("missing binary");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }
}
