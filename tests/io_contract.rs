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
