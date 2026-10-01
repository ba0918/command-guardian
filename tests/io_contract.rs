use std::process::{Command, Stdio};

fn command(home: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_command-guardian"));
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"));
    command
}

// @kotowari[REQ-017, REQ-021]
#[test]
fn req_021_non_utf8_cwd_and_ignored_hook_arguments_do_not_panic() {
    use std::os::unix::ffi::OsStringExt;
    let home = tempfile::tempdir().unwrap();
    let cwd = home
        .path()
        .join(std::ffi::OsString::from_vec(vec![b'x', 0xff]));
    std::fs::create_dir(&cwd).unwrap();
    let output = command(home.path())
        .args(["check", "true", "--cwd"])
        .arg(&cwd)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{:?}", output);
    let output = command(home.path())
        .arg("hook")
        .arg(std::ffi::OsString::from_vec(vec![0xff]))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{:?}", output);
    let output = command(home.path())
        .arg("check")
        .arg(std::ffi::OsString::from_vec(vec![0xff]))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
}

// @kotowari[REQ-021]
#[test]
fn req_021_check_output_failure_is_an_explicit_failure() {
    let home = tempfile::tempdir().unwrap();
    for format in ["text", "json"] {
        let output = command(home.path())
            .args(["check", "true", "--format", format])
            .stdout(
                std::fs::OpenOptions::new()
                    .write(true)
                    .open("/dev/full")
                    .unwrap(),
            )
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(3), "{:?}", output);
    }
}

// @kotowari[REQ-017]
#[test]
fn req_017_hook_and_help_keep_zero_on_output_failure() {
    let home = tempfile::tempdir().unwrap();
    for args in [
        vec!["--help"],
        vec!["check", "--help"],
        vec!["hook", "--help"],
    ] {
        let output = command(home.path())
            .args(args)
            .stdout(
                std::fs::OpenOptions::new()
                    .write(true)
                    .open("/dev/full")
                    .unwrap(),
            )
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0));
    }
    use std::io::Write;
    let mut child = command(home.path())
        .args(["hook", "--agent", "claude"])
        .stdin(Stdio::piped())
        .stdout(
            std::fs::OpenOptions::new()
                .write(true)
                .open("/dev/full")
                .unwrap(),
        )
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"tool_name":"Bash","tool_input":{"command":"rm /etc/x"},"cwd":"/tmp"}"#)
        .unwrap();
    assert_eq!(child.wait().unwrap().code(), Some(0));
}

// @kotowari[REQ-015, REQ-021]
#[test]
fn req_015_warning_output_failure_does_not_abort_judgment() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("config/command-guardian/config.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(config, "invalid TOML !").unwrap();
    let output = command(home.path())
        .args(["check", "true"])
        .stderr(
            std::fs::OpenOptions::new()
                .write(true)
                .open("/dev/full")
                .unwrap(),
        )
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
}
