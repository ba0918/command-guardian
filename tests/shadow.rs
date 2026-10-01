//! S16: 影実行とログ（REQ-018, REQ-019）。

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_command-guardian")
}

fn temp_dir(prefix: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap()
}

/// `[mode] enforce = false` の利用者設定を置く。
fn write_shadow_config(xdg: &Path) {
    let path = xdg.join("command-guardian/config.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "[mode]\nenforce = false\n").unwrap();
}

fn bash_input(command: &str, cwd: &str) -> String {
    format!(
        r#"{{"tool_name":"Bash","tool_input":{{"command":{}}},"cwd":{}}}"#,
        serde_json::to_string(command).unwrap(),
        serde_json::to_string(cwd).unwrap()
    )
}

/// XDG_STATE_HOME を渡す（`None` のときは環境から外す）。
fn run_hook(input: &str, home: &Path, xdg: &Path, state: Option<&Path>) -> Run {
    run_bin(
        &["hook", "--agent", "claude"],
        Some(input),
        home,
        xdg,
        state,
    )
}

fn run_check(command: &str, home: &Path, xdg: &Path, state: Option<&Path>) -> Run {
    run_bin(
        &["check", command, "--cwd", "/tmp/scratch"],
        None,
        home,
        xdg,
        state,
    )
}

// @kotowari[REQ-019]
#[test]
fn req_019_shadow_file_symlink_does_not_modify_its_target() {
    let home = temp_dir("shadow-safe-home-");
    let state = temp_dir("shadow-safe-state-");
    let xdg = home.path().join("config");
    write_shadow_config(&xdg);
    let sentinel = state.path().join("sentinel");
    std::fs::write(&sentinel, "unchanged").unwrap();
    std::fs::set_permissions(&sentinel, std::fs::Permissions::from_mode(0o640)).unwrap();
    let dir = state.path().join("command-guardian");
    std::fs::create_dir(&dir).unwrap();
    std::os::unix::fs::symlink(&sentinel, dir.join("shadow.log")).unwrap();
    let result = run_hook(
        &bash_input("rm /etc/x", "/tmp"),
        home.path(),
        &xdg,
        Some(state.path()),
    );
    assert_eq!(result.code, 0);
    assert!(result.stdout.is_empty());
    assert!(
        result.stderr.to_lowercase().contains("shadow log"),
        "{}",
        result.stderr
    );
    assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "unchanged");
    assert_eq!(
        std::fs::metadata(sentinel).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

// @kotowari[REQ-019]
#[test]
fn req_019_shadow_directory_symlink_does_not_modify_its_target() {
    let home = temp_dir("shadow-safe-home-");
    let state = temp_dir("shadow-safe-state-");
    let xdg = home.path().join("config");
    write_shadow_config(&xdg);
    let sentinel = state.path().join("sentinel-dir");
    std::fs::create_dir(&sentinel).unwrap();
    std::fs::set_permissions(&sentinel, std::fs::Permissions::from_mode(0o750)).unwrap();
    std::os::unix::fs::symlink(&sentinel, state.path().join("command-guardian")).unwrap();
    let result = run_hook(
        &bash_input("rm /etc/x", "/tmp"),
        home.path(),
        &xdg,
        Some(state.path()),
    );
    assert_eq!(result.code, 0);
    assert!(result.stdout.is_empty());
    assert!(!sentinel.join("shadow.log").exists());
    assert!(
        result.stderr.to_lowercase().contains("shadow log"),
        "{}",
        result.stderr
    );
    assert_eq!(
        std::fs::metadata(sentinel).unwrap().permissions().mode() & 0o777,
        0o750
    );
}

// @kotowari[REQ-018, REQ-019]
#[test]
fn req_019_unusable_shadow_log_warns_without_returning_a_hook_decision() {
    let home = temp_dir("shadow-failure-home-");
    let state = temp_dir("shadow-failure-state-");
    let xdg = home.path().join("config");
    write_shadow_config(&xdg);
    let dir = state.path().join("command-guardian");
    std::fs::create_dir(&dir).unwrap();
    std::fs::create_dir(dir.join("shadow.log")).unwrap();
    let result = run_hook(
        &bash_input("rm /etc/x", "/tmp"),
        home.path(),
        &xdg,
        Some(state.path()),
    );
    assert_eq!(result.code, 0);
    assert!(result.stdout.is_empty());
    assert!(
        result.stderr.to_lowercase().contains("shadow log"),
        "{}",
        result.stderr
    );
}

// @kotowari[REQ-018, REQ-019]
#[test]
fn req_019_missing_shadow_log_location_warns_instead_of_silently_omitting_the_record() {
    let home = temp_dir("shadow-failure-home-");
    let xdg = home.path().join("config");
    write_shadow_config(&xdg);
    let result = run_hook(&bash_input("rm /etc/x", "/tmp"), Path::new(""), &xdg, None);
    assert_eq!(result.code, 0);
    assert!(result.stdout.is_empty());
    assert!(
        result.stderr.to_lowercase().contains("shadow log"),
        "{}",
        result.stderr
    );
}

// @kotowari[REQ-019]
#[test]
fn req_019_concurrent_shadow_records_keep_fields_from_the_same_judgment() {
    let home = temp_dir("shadow-concurrent-home-");
    let state = temp_dir("shadow-concurrent-state-");
    let xdg = home.path().join("config");
    write_shadow_config(&xdg);
    let barrier = std::sync::Barrier::new(24);
    std::thread::scope(|scope| {
        for index in 0..24 {
            let home = home.path();
            let state = state.path();
            let xdg = &xdg;
            let barrier = &barrier;
            scope.spawn(move || {
                barrier.wait();
                let command = format!("echo '{}'; rm /etc/record-{index}", "x".repeat(4096));
                let result = run_hook(&bash_input(&command, "/tmp"), home, xdg, Some(state));
                assert_eq!(result.code, 0);
                assert!(result.stdout.is_empty());
            });
        }
    });
    let log = std::fs::read_to_string(state.path().join("command-guardian/shadow.log")).unwrap();
    assert_eq!(log.lines().count(), 24);
    let mut records = std::collections::HashSet::new();
    for line in log.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 5, "record field count: {}", fields.len());
        assert!(fields[0].parse::<u64>().is_ok());
        assert_eq!(fields[1], "block");
        assert!(fields[2].contains(fields[3]));
        assert!(fields[4].ends_with(&format!("rm {}", fields[3])));
        assert!(records.insert(fields[3]));
    }
}

fn run_bin(
    args: &[&str],
    input: Option<&str>,
    home: &Path,
    xdg: &Path,
    state: Option<&Path>,
) -> Run {
    let mut cmd = Command::new(bin());
    cmd.args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", xdg)
        .env("TMPDIR", "/tmp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match state {
        Some(s) => cmd.env("XDG_STATE_HOME", s),
        None => cmd.env_remove("XDG_STATE_HOME"),
    };
    let mut child = cmd.spawn().unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

// @kotowari[REQ-018, EX-018]
#[test]
fn req_018_shadow_returns_nothing_and_logs() {
    let home = temp_dir("command-guardian-shadow-home-");
    let xdg = temp_dir("command-guardian-shadow-xdg-");
    let state = temp_dir("command-guardian-shadow-state-");
    write_shadow_config(xdg.path());
    let r = run_hook(
        &bash_input("rm -rf /etc/nginx", "/tmp/scratch"),
        home.path(),
        xdg.path(),
        Some(state.path()),
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert!(r.stdout.trim().is_empty(), "{}", r.stdout);
    let log = std::fs::read_to_string(state.path().join("command-guardian/shadow.log")).unwrap();
    assert!(log.contains("block"), "{log}");
    assert!(log.contains("/etc/nginx"), "{log}");
    assert!(log.contains("rm -rf /etc/nginx"), "{log}");
    assert!(log.contains("system area"), "{log}");
}

// @kotowari[REQ-019, EX-026]
#[test]
fn req_019_shadow_log_is_one_line_with_owner_only_permissions() {
    let home = temp_dir("command-guardian-shadow-home-");
    let xdg = temp_dir("command-guardian-shadow-xdg-");
    let state = temp_dir("command-guardian-shadow-state-");
    write_shadow_config(xdg.path());
    run_hook(
        &bash_input("rm -rf /etc/nginx", "/tmp/scratch"),
        home.path(),
        xdg.path(),
        Some(state.path()),
    );
    let path = state.path().join("command-guardian/shadow.log");
    let log = std::fs::read_to_string(&path).unwrap();
    let line = log.trim_end();
    assert!(!line.contains('\n'), "{log}");
    let fields: Vec<&str> = line.split('\t').collect();
    assert_eq!(fields.len(), 5, "{line}");
    assert!(fields[0].parse::<u64>().unwrap() > 0, "{line}");
    assert_eq!(fields[1], "block", "{line}");
    assert!(fields[2].contains("system area"), "{line}");
    assert_eq!(fields[3], "/etc/nginx", "{line}");
    assert_eq!(fields[4], "rm -rf /etc/nginx", "{line}");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600, "{mode:o}");
}

// @kotowari[REQ-019]
#[test]
fn req_019_state_home_falls_back_to_home_local_state() {
    let home = temp_dir("command-guardian-shadow-home-");
    let xdg = home.path().join(".config");
    write_shadow_config(&xdg);
    run_hook(
        &bash_input("rm -rf /etc/nginx", "/tmp/scratch"),
        home.path(),
        &xdg,
        None,
    );
    assert!(home
        .path()
        .join(".local/state/command-guardian/shadow.log")
        .is_file());
}

// @kotowari[REQ-018]
#[test]
fn req_018_enforce_true_does_not_write_a_log() {
    let home = temp_dir("command-guardian-shadow-home-");
    let xdg = temp_dir("command-guardian-shadow-xdg-");
    let state = temp_dir("command-guardian-shadow-state-");
    let r = run_hook(
        &bash_input("rm -rf /etc/nginx", "/tmp/scratch"),
        home.path(),
        xdg.path(),
        Some(state.path()),
    );
    assert_eq!(r.code, 0);
    assert!(r.stdout.contains("deny"), "{}", r.stdout);
    assert!(!state.path().join("command-guardian").exists());
}

// @kotowari[REQ-018, EX-034]
#[test]
fn req_018_check_still_prints_the_verdict_in_shadow() {
    let home = temp_dir("command-guardian-shadow-home-");
    let xdg = temp_dir("command-guardian-shadow-xdg-");
    let state = temp_dir("command-guardian-shadow-state-");
    write_shadow_config(xdg.path());
    let r = run_check(
        "rm -rf /etc/nginx",
        home.path(),
        xdg.path(),
        Some(state.path()),
    );
    assert_eq!(r.code, 2, "stdout: {} stderr: {}", r.stdout, r.stderr);
    assert!(r.stdout.contains("block"), "{}", r.stdout);
    assert!(!state.path().join("command-guardian").exists());
}

// @kotowari[REQ-019]
#[test]
fn req_019_empty_state_home_falls_back_to_home_local_state() {
    // XDG_STATE_HOME が空文字列のときは ~/.local/state に書き、cwd 相対には書かない。
    let home = temp_dir("command-guardian-shadow-home-");
    let xdg = temp_dir("command-guardian-shadow-xdg-");
    let scratch = temp_dir("command-guardian-shadow-cwd-");
    write_shadow_config(xdg.path());
    let input = bash_input("rm -rf /etc/nginx", "/tmp/scratch");
    let mut child = Command::new(bin())
        .args(["hook", "--agent", "claude"])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", xdg.path())
        .env("XDG_STATE_HOME", "")
        .env("TMPDIR", "/tmp")
        .current_dir(scratch.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(home
        .path()
        .join(".local/state/command-guardian/shadow.log")
        .is_file());
    assert!(!scratch.path().join("command-guardian/shadow.log").exists());
}

// @kotowari[REQ-019]
#[test]
fn req_019_multiline_command_stays_one_record() {
    // コマンド本文の改行とタブをエスケープし、1 判定 1 行 5 フィールドを守る。
    let home = temp_dir("command-guardian-shadow-home-");
    let xdg = temp_dir("command-guardian-shadow-xdg-");
    let state = temp_dir("command-guardian-shadow-state-");
    write_shadow_config(xdg.path());
    run_hook(
        &bash_input("echo a\tb\nrm -rf /tmp/scratch/x", "/tmp/scratch"),
        home.path(),
        xdg.path(),
        Some(state.path()),
    );
    let path = state.path().join("command-guardian/shadow.log");
    let log = std::fs::read_to_string(&path).unwrap();
    let line = log.trim_end();
    assert!(!line.contains('\n'), "{log}");
    assert_eq!(line.split('\t').count(), 5, "{line}");
    assert!(line.contains("\\n"), "{line}");
    assert!(line.contains("\\t"), "{line}");
}
