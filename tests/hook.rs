//! S15: フックの口（REQ-016, REQ-022, REQ-023, REQ-024）。
//!
//! バイナリを起動し、stdin にフックの入力を与えて stdout を確かめる。

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_hook-guardian")
}

/// HOME と XDG_CONFIG_HOME を分けて渡してフックを起動する。
fn run_hook(args: &[&str], input: &str, home: &Path, xdg: &Path) -> Run {
    let mut child = Command::new(bin())
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", xdg)
        .env("TMPDIR", "/tmp")
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
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

fn temp_dir(prefix: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap()
}

/// 未追跡のファイルを 1 つ持つ git の作業ツリー。
fn git_repo_with_untracked() -> tempfile::TempDir {
    let dir = temp_dir("hook-guardian-hook-git-");
    let root = dir.path();
    std::fs::write(root.join("tracked.txt"), "x").unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "."],
        vec![
            "-c",
            "user.email=t@example.com",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "init",
        ],
    ] {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(&args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
    }
    std::fs::write(root.join("notes.txt"), "notes\n").unwrap();
    dir
}

fn bash_input(command: &str, cwd: &str) -> String {
    format!(
        r#"{{"tool_name":"Bash","tool_input":{{"command":{}}},"cwd":{}}}"#,
        serde_json::to_string(command).unwrap(),
        serde_json::to_string(cwd).unwrap()
    )
}

fn bash_input_with_mode(command: &str, cwd: &str, mode: &str) -> String {
    format!(
        r#"{{"tool_name":"Bash","tool_input":{{"command":{}}},"cwd":{},"permission_mode":{}}}"#,
        serde_json::to_string(command).unwrap(),
        serde_json::to_string(cwd).unwrap(),
        serde_json::to_string(mode).unwrap()
    )
}

fn envelope(stdout: &str) -> serde_json::Value {
    serde_json::from_str(stdout).unwrap()
}

// @kotowari[REQ-016, EX-025]
#[test]
fn req_016_check_and_hook_accept_two_commands() {
    let home = temp_dir("hook-guardian-hook-home-");

    let check = Command::new(bin())
        .args(["check", "true", "--cwd", "/tmp"])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .output()
        .unwrap();
    assert_eq!(check.status.code(), Some(0));

    let r = run_hook(
        &["hook", "--agent", "claude"],
        r#"{"tool_name":"Write","tool_input":{"file_path":"/tmp/x"},"cwd":"/tmp"}"#,
        home.path(),
        &home.path().join(".config"),
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert!(r.stdout.trim().is_empty(), "{}", r.stdout);
}

// @kotowari[REQ-022]
#[test]
fn req_022_claude_ask_returns_permission_decision() {
    let xdg = temp_dir("hook-guardian-hook-xdg-");
    let repo = git_repo_with_untracked();
    // フィクスチャは実ユーザのホームの下にあるため、HOME は実環境のままにする。
    let real_home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap();
    let r = run_hook(
        &["hook", "--agent", "claude"],
        &bash_input("rm notes.txt", repo.path().to_str().unwrap()),
        &real_home,
        &xdg.path().join(".config"),
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let value = envelope(&r.stdout);
    let specific = &value["hookSpecificOutput"];
    assert_eq!(specific["hookEventName"], "PreToolUse");
    assert_eq!(specific["permissionDecision"], "ask");
    assert!(specific["permissionDecisionReason"]
        .as_str()
        .unwrap()
        .contains("未追跡"));
}

// @kotowari[REQ-022, EX-020]
#[test]
fn req_022_claude_block_returns_deny() {
    let home = temp_dir("hook-guardian-hook-home-");
    let r = run_hook(
        &["hook", "--agent", "claude"],
        &bash_input("rm -rf /etc/nginx", "/tmp/scratch"),
        home.path(),
        &home.path().join(".config"),
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let value = envelope(&r.stdout);
    let specific = &value["hookSpecificOutput"];
    assert_eq!(specific["hookEventName"], "PreToolUse");
    assert_eq!(specific["permissionDecision"], "deny");
    assert!(specific["permissionDecisionReason"]
        .as_str()
        .unwrap()
        .contains("システムの領域"));
}

// @kotowari[REQ-022]
#[test]
fn req_022_claude_allow_returns_nothing() {
    let home = temp_dir("hook-guardian-hook-home-");
    let r = run_hook(
        &["hook", "--agent", "claude"],
        &bash_input("rm -rf /tmp/scratch/x", "/tmp/scratch"),
        home.path(),
        &home.path().join(".config"),
    );
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty(), "{}", r.stdout);
}

// @kotowari[REQ-022]
#[test]
fn req_022_claude_dont_ask_modes_drop_the_ask() {
    let xdg = temp_dir("hook-guardian-hook-xdg-");
    let repo = git_repo_with_untracked();
    let real_home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap();
    for mode in ["dontAsk", "bypassPermissions"] {
        let r = run_hook(
            &["hook", "--agent", "claude"],
            &bash_input_with_mode("rm notes.txt", repo.path().to_str().unwrap(), mode),
            &real_home,
            &xdg.path().join(".config"),
        );
        assert_eq!(r.code, 0);
        assert!(r.stdout.trim().is_empty(), "{mode}: {}", r.stdout);
    }
}

// @kotowari[REQ-023, EX-021]
#[test]
fn req_023_codex_ask_returns_nothing() {
    let xdg = temp_dir("hook-guardian-hook-xdg-");
    let repo = git_repo_with_untracked();
    let real_home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap();
    let r = run_hook(
        &["hook", "--agent", "codex"],
        &bash_input("rm notes.txt", repo.path().to_str().unwrap()),
        &real_home,
        &xdg.path().join(".config"),
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert!(r.stdout.trim().is_empty(), "{}", r.stdout);
}

// @kotowari[REQ-023]
#[test]
fn req_023_codex_block_returns_deny() {
    let home = temp_dir("hook-guardian-hook-home-");
    let r = run_hook(
        &["hook", "--agent", "codex"],
        &bash_input("rm -rf /etc/nginx", "/tmp/scratch"),
        home.path(),
        &home.path().join(".config"),
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let value = envelope(&r.stdout);
    let specific = &value["hookSpecificOutput"];
    assert_eq!(specific["hookEventName"], "PreToolUse");
    assert_eq!(specific["permissionDecision"], "deny");
    assert!(!specific["permissionDecisionReason"]
        .as_str()
        .unwrap()
        .is_empty());
}

// @kotowari[REQ-023]
#[test]
fn req_023_codex_allow_returns_nothing() {
    let home = temp_dir("hook-guardian-hook-home-");
    let r = run_hook(
        &["hook", "--agent", "codex"],
        &bash_input("rm -rf /tmp/scratch/x", "/tmp/scratch"),
        home.path(),
        &home.path().join(".config"),
    );
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty(), "{}", r.stdout);
}

// @kotowari[REQ-024, EX-022]
#[test]
fn req_024_non_bash_input_returns_nothing() {
    let home = temp_dir("hook-guardian-hook-home-");
    for agent in ["claude", "codex"] {
        let r = run_hook(
            &["hook", "--agent", agent],
            r#"{"tool_name":"Write","tool_input":{"file_path":"/etc/nginx/nginx.conf"},"cwd":"/tmp"}"#,
            home.path(),
            &home.path().join(".config"),
        );
        assert_eq!(r.code, 0);
        assert!(r.stdout.trim().is_empty(), "{agent}: {}", r.stdout);
    }
}
