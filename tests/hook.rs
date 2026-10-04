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
    env!("CARGO_BIN_EXE_command-guardian")
}

/// HOME と XDG_CONFIG_HOME を分けて渡してフックを起動する。
fn run_hook(args: &[&str], input: &str, home: &Path, xdg: &Path) -> Run {
    run_hook_env(
        args,
        input,
        home.to_str().unwrap(),
        xdg.to_str().unwrap(),
        "/tmp",
    )
}

/// 環境変数をそのまま渡してフックを起動する（空文字列の検証に使う）。
fn run_hook_env(args: &[&str], input: &str, home: &str, xdg: &str, tmpdir: &str) -> Run {
    let mut cmd = Command::new(bin());
    cmd.args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", xdg)
        .env("TMPDIR", tmpdir);
    spawn_with_input(cmd, input)
}

fn spawn_with_input(mut cmd: Command, input: &str) -> Run {
    let mut child = cmd
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

/// フックの環境（GIT_DIR など）を引き継がずに git を起動する。
fn git(root: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(root).env("GIT_CONFIG_NOSYSTEM", "1");
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(&name);
        }
    }
    cmd
}

/// 未追跡のファイルを 1 つ持つ git の作業ツリー。
fn git_repo_with_untracked() -> tempfile::TempDir {
    let dir = temp_dir("command-guardian-hook-git-");
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
        let out = git(root).args(&args).output().unwrap();
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
    let home = temp_dir("command-guardian-hook-home-");

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
    let xdg = temp_dir("command-guardian-hook-xdg-");
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
    assert!(
        specific["permissionDecisionReason"]
            .as_str()
            .unwrap()
            .contains("untracked")
    );
}

// @kotowari[REQ-022, EX-020]
#[test]
fn req_022_claude_block_returns_deny() {
    let home = temp_dir("command-guardian-hook-home-");
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
    assert!(
        specific["permissionDecisionReason"]
            .as_str()
            .unwrap()
            .contains("system area")
    );
}

// @kotowari[REQ-022]
#[test]
fn req_022_claude_allow_returns_nothing() {
    let home = temp_dir("command-guardian-hook-home-");
    let r = run_hook(
        &["hook", "--agent", "claude"],
        &bash_input("rm -rf /tmp/scratch/x", "/tmp/scratch"),
        home.path(),
        &home.path().join(".config"),
    );
    assert_eq!(r.code, 0);
    assert!(r.stdout.trim().is_empty(), "{}", r.stdout);
}

// @kotowari[REQ-001, REQ-022]
#[test]
fn req_022_claude_allows_dev_null_redirects() {
    let home = temp_dir("command-guardian-hook-home-");
    for command in [
        "echo x >/dev/null",
        "echo x 2>/dev/null",
        "echo x &>\"/dev/null\"",
    ] {
        let r = run_hook(
            &["hook", "--agent", "claude"],
            &bash_input(command, "/tmp/scratch"),
            home.path(),
            &home.path().join(".config"),
        );
        assert_eq!(r.code, 0, "{command}: {}", r.stderr);
        assert!(r.stdout.trim().is_empty(), "{command}: {}", r.stdout);
    }
}

// @kotowari[REQ-001, REQ-022]
#[test]
fn req_022_claude_dev_null_redirect_does_not_hide_danger() {
    let home = temp_dir("command-guardian-hook-home-");
    for command in [
        "rm -rf /etc/nginx 2>/dev/null",
        "echo x >/dev/null 2>/etc/log",
    ] {
        let r = run_hook(
            &["hook", "--agent", "claude"],
            &bash_input(command, "/tmp/scratch"),
            home.path(),
            &home.path().join(".config"),
        );
        assert_eq!(r.code, 0, "{command}: {}", r.stderr);
        assert_eq!(
            envelope(&r.stdout)["hookSpecificOutput"]["permissionDecision"],
            "deny",
            "{command}"
        );
    }
}

// @kotowari[REQ-022]
#[test]
fn req_022_claude_dont_ask_modes_drop_the_ask() {
    let xdg = temp_dir("command-guardian-hook-xdg-");
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
    let xdg = temp_dir("command-guardian-hook-xdg-");
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
    let home = temp_dir("command-guardian-hook-home-");
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
    assert!(
        !specific["permissionDecisionReason"]
            .as_str()
            .unwrap()
            .is_empty()
    );
}

// @kotowari[REQ-023]
#[test]
fn req_023_codex_allow_returns_nothing() {
    let home = temp_dir("command-guardian-hook-home-");
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
    let home = temp_dir("command-guardian-hook-home-");
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

// @kotowari[REQ-010]
#[test]
fn req_010_hook_does_not_fail_on_a_bare_wrapper_option() {
    // 値付きオプションが末尾のラッパーでも、フックは判定を出さずに 0 で終わる。
    let home = temp_dir("command-guardian-hook-home-");
    let r = run_hook(
        &["hook", "--agent", "claude"],
        &bash_input("doas -a", "/tmp/scratch"),
        home.path(),
        &home.path().join(".config"),
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert!(r.stdout.trim().is_empty(), "{}", r.stdout);
}

// @kotowari[REQ-006]
#[test]
fn req_006_empty_tmpdir_still_denies_protected_paths() {
    // TMPDIR が空文字列でも保護領域の削除は deny のまま。
    let r = run_hook_env(
        &["hook", "--agent", "claude"],
        &bash_input("rm -rf /etc/nginx", "/tmp/scratch"),
        "",
        "",
        "",
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let value = envelope(&r.stdout);
    assert_eq!(value["hookSpecificOutput"]["permissionDecision"], "deny");
}

// @kotowari[REQ-015]
#[test]
fn req_015_hook_warns_when_the_user_config_is_broken() {
    // 壊れた利用者設定では check と同じくフックも警告を stderr に出す。
    let home = temp_dir("command-guardian-hook-home-");
    let xdg = temp_dir("command-guardian-hook-xdg-");
    let config = xdg.path().join("command-guardian/config.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(&config, "これは TOML ではない [ paths\n").unwrap();
    let r = run_hook(
        &["hook", "--agent", "claude"],
        &bash_input("rm -rf /tmp/scratch/x", "/tmp/scratch"),
        home.path(),
        xdg.path(),
    );
    assert_eq!(r.code, 0);
    assert!(r.stderr.contains("Warning:"), "{}", r.stderr);
    assert!(r.stderr.contains("user configuration"), "{}", r.stderr);
}

/// XDG_STATE_HOME もフィクスチャへ向けてフックを起動する。委任の記録が実利用者の
/// 影ログへ書かれないよう、`defer_ask` を有効にする試験はこれを使う。
fn run_hook_with_state(args: &[&str], input: &str, home: &Path, xdg: &Path, state: &Path) -> Run {
    let mut cmd = Command::new(bin());
    cmd.args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", xdg)
        .env("XDG_STATE_HOME", state)
        .env("TMPDIR", "/tmp");
    spawn_with_input(cmd, input)
}

fn write_user_config(xdg: &Path, text: &str) {
    let path = xdg.join("command-guardian/config.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn real_home() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap()
}

// @kotowari[REQ-059, EX-114]
#[test]
fn ex_114_without_defer_ask_claude_still_asks() {
    let home = temp_dir("command-guardian-hook-home-");
    let state = temp_dir("command-guardian-hook-state-");
    let xdg = home.path().join(".config");
    let r = run_hook_with_state(
        &["hook", "--agent", "claude"],
        &bash_input("rm /mnt/fixture/x", "/tmp/scratch"),
        home.path(),
        &xdg,
        state.path(),
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert_eq!(
        envelope(&r.stdout)["hookSpecificOutput"]["permissionDecision"],
        "ask"
    );
}

// @kotowari[REQ-059, EX-115]
#[test]
fn ex_115_trusted_project_cannot_enable_defer_ask() {
    let xdg = temp_dir("command-guardian-hook-xdg-");
    let state = temp_dir("command-guardian-hook-state-");
    let repo = git_repo_with_untracked();
    let root = repo.path().canonicalize().unwrap();
    write_user_config(
        xdg.path(),
        &format!("trusted_projects = [{:?}]\n", root.to_str().unwrap()),
    );
    std::fs::write(
        root.join(".command-guardian.toml"),
        "[mode]\ndefer_ask = true\n",
    )
    .unwrap();
    let r = run_hook_with_state(
        &["hook", "--agent", "claude"],
        &bash_input("rm notes.txt", root.to_str().unwrap()),
        &real_home(),
        xdg.path(),
        state.path(),
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert_eq!(
        envelope(&r.stdout)["hookSpecificOutput"]["permissionDecision"],
        "ask"
    );
    assert!(r.stderr.contains("mode.defer_ask"), "{}", r.stderr);
}

// @kotowari[REQ-059, EX-117]
#[test]
fn ex_117_invalid_user_defer_ask_keeps_asking_and_warns() {
    let home = temp_dir("command-guardian-hook-home-");
    let state = temp_dir("command-guardian-hook-state-");
    let xdg = home.path().join(".config");
    write_user_config(&xdg, "[mode]\ndefer_ask = \"yes\"\n");
    let r = run_hook_with_state(
        &["hook", "--agent", "claude"],
        &bash_input("rm /mnt/fixture/x", "/tmp/scratch"),
        home.path(),
        &xdg,
        state.path(),
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert_eq!(
        envelope(&r.stdout)["hookSpecificOutput"]["permissionDecision"],
        "ask"
    );
    assert!(r.stderr.contains("user configuration"), "{}", r.stderr);
}

// @kotowari[REQ-060, EX-118]
#[test]
fn ex_118_claude_defers_ask_with_empty_output() {
    let xdg = temp_dir("command-guardian-hook-xdg-");
    let state = temp_dir("command-guardian-hook-state-");
    let repo = git_repo_with_untracked();
    write_user_config(xdg.path(), "[mode]\ndefer_ask = true\n");
    let r = run_hook_with_state(
        &["hook", "--agent", "claude"],
        &bash_input("rm notes.txt", repo.path().to_str().unwrap()),
        &real_home(),
        xdg.path(),
        state.path(),
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert!(r.stdout.is_empty(), "{}", r.stdout);
}

// @kotowari[REQ-060, EX-119]
#[test]
fn ex_119_unreadable_syntax_ask_is_also_deferred() {
    let home = temp_dir("command-guardian-hook-home-");
    let state = temp_dir("command-guardian-hook-state-");
    let xdg = home.path().join(".config");
    write_user_config(&xdg, "[mode]\ndefer_ask = true\n");
    let r = run_hook_with_state(
        &["hook", "--agent", "claude"],
        &bash_input("if then fi (", "/tmp/scratch"),
        home.path(),
        &xdg,
        state.path(),
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert!(r.stdout.is_empty(), "{}", r.stdout);
}

// @kotowari[REQ-060, EX-120]
#[test]
fn ex_120_block_is_not_deferred() {
    let home = temp_dir("command-guardian-hook-home-");
    let state = temp_dir("command-guardian-hook-state-");
    let xdg = home.path().join(".config");
    write_user_config(&xdg, "[mode]\ndefer_ask = true\n");
    let r = run_hook_with_state(
        &["hook", "--agent", "claude"],
        &bash_input("rm -rf /etc/nginx", "/tmp/scratch"),
        home.path(),
        &xdg,
        state.path(),
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let value = envelope(&r.stdout);
    assert_eq!(value["hookSpecificOutput"]["permissionDecision"], "deny");
    assert!(
        !value["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap()
            .is_empty()
    );
}

// @kotowari[REQ-060, REQ-018, EX-121]
#[test]
fn ex_121_check_ignores_defer_ask() {
    let home = temp_dir("command-guardian-hook-home-");
    let state = temp_dir("command-guardian-hook-state-");
    let xdg = home.path().join(".config");
    write_user_config(&xdg, "[mode]\ndefer_ask = true\n");
    let out = Command::new(bin())
        .args([
            "check",
            "rm /mnt/fixture/x",
            "--cwd",
            "/tmp/scratch",
            "--format",
            "json",
        ])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .env("XDG_STATE_HOME", state.path())
        .env("TMPDIR", "/tmp")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["verdict"], "ask");
    assert!(!state.path().join("command-guardian").exists());
}

// @kotowari[REQ-060, REQ-062, REQ-022, EX-132]
#[test]
fn ex_132_unreadable_hook_input_is_not_deferred_or_recorded() {
    let home = temp_dir("command-guardian-hook-home-");
    let state = temp_dir("command-guardian-hook-state-");
    let xdg = home.path().join(".config");
    write_user_config(&xdg, "[mode]\ndefer_ask = true\n");
    let r = run_hook_with_state(
        &["hook", "--agent", "codex"],
        "{not json",
        home.path(),
        &xdg,
        state.path(),
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert!(r.stdout.is_empty(), "{}", r.stdout);
    assert!(!state.path().join("command-guardian/shadow.log").exists());
}
