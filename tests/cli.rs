//! S14: check の口（REQ-017）。

use std::path::Path;
use std::process::Command;

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_hook-guardian")
}

fn run(args: &[&str], home: &Path) -> Run {
    run_with_home(args, home, &home.join(".config"))
}

/// HOME と XDG_CONFIG_HOME を分けて渡す。
fn run_with_home(args: &[&str], home: &Path, xdg: &Path) -> Run {
    let out = Command::new(bin())
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", xdg)
        .env("TMPDIR", "/tmp")
        .output()
        .unwrap();
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

/// 環境変数をそのまま渡して起動する（空文字列の検証に使う）。
fn run_env(args: &[&str], envs: &[(&str, &str)]) -> Run {
    let mut cmd = Command::new(bin());
    cmd.args(args);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let out = cmd.output().unwrap();
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

fn temp_home() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("hook-guardian-home-")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap()
}

fn git_repo() -> tempfile::TempDir {
    let dir = tempfile::Builder::new()
        .prefix("hook-guardian-cli-git-")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap();
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
    dir
}

// @kotowari[REQ-017, EX-017]
#[test]
fn req_017_check_allows_with_exit_code_zero() {
    let home = temp_home();
    let r = run(
        &["check", "rm -rf /tmp/scratch/x", "--cwd", "/tmp/scratch"],
        home.path(),
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert!(r.stdout.contains("allow"), "{}", r.stdout);
}

// @kotowari[REQ-017, EX-031]
#[test]
fn req_017_check_asks_with_exit_code_one() {
    let xdg = temp_home();
    let repo = git_repo();
    std::fs::write(repo.path().join("notes.txt"), "notes\n").unwrap();
    // フィクスチャは実ユーザのホームの下にあるため、HOME は実環境のままにする
    // （利用者設定は XDG_CONFIG_HOME の分離で読ませない）。
    let real_home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap();
    let r = run_with_home(
        &[
            "check",
            "rm notes.txt",
            "--cwd",
            repo.path().to_str().unwrap(),
        ],
        &real_home,
        &xdg.path().join(".config"),
    );
    assert_eq!(r.code, 1, "stdout: {} stderr: {}", r.stdout, r.stderr);
    assert!(r.stdout.contains("ask"), "{}", r.stdout);
}

// @kotowari[REQ-017, EX-032]
#[test]
fn req_017_check_blocks_with_exit_code_two() {
    let home = temp_home();
    let r = run(
        &["check", "rm -rf /etc/nginx", "--cwd", "/tmp/scratch"],
        home.path(),
    );
    assert_eq!(r.code, 2, "stdout: {} stderr: {}", r.stdout, r.stderr);
    assert!(r.stdout.contains("block"), "{}", r.stdout);
}

// @kotowari[REQ-017, EX-033]
#[test]
fn req_017_check_without_a_command_fails_with_exit_code_three() {
    let home = temp_home();
    let r = run(&["check"], home.path());
    assert_eq!(r.code, 3, "stdout: {} stderr: {}", r.stdout, r.stderr);
    assert!(r.stderr.contains("コマンド文字列"), "{}", r.stderr);
}

// @kotowari[REQ-017]
#[test]
fn req_017_json_has_verdict_and_effect_fields() {
    let home = temp_home();
    let r = run(
        &[
            "check",
            "rm -rf /etc/nginx",
            "--cwd",
            "/tmp/scratch",
            "--format",
            "json",
        ],
        home.path(),
    );
    assert_eq!(r.code, 2);
    let value: serde_json::Value = serde_json::from_str(&r.stdout).unwrap();
    assert_eq!(value["verdict"], "block");
    let effect = &value["effects"][0];
    assert_eq!(effect["op"], "delete");
    assert_eq!(effect["path"], "/etc/nginx");
    assert_eq!(effect["class"], "protected");
    assert_eq!(effect["verdict"], "block");
    assert!(effect["reason"].as_str().unwrap().contains("protected"));
}

// @kotowari[REQ-017]
#[test]
fn req_017_json_reports_rules_and_text_reports_the_message() {
    let home = temp_home();
    let config = home.path().join(".config/hook-guardian/config.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(
        &config,
        r#"
[[commands.guard]]
program = "git"
reason = "push は確認してください"
verdict = "ask"
deny = [["push"]]
"#,
    )
    .unwrap();
    let r = run(
        &[
            "check",
            "git push origin main",
            "--cwd",
            "/tmp/scratch",
            "--format",
            "json",
        ],
        home.path(),
    );
    assert_eq!(r.code, 1, "{}", r.stdout);
    let value: serde_json::Value = serde_json::from_str(&r.stdout).unwrap();
    assert_eq!(value["rules"][0]["program"], "git");
    assert!(value["rules"][0]["reason"]
        .as_str()
        .unwrap()
        .contains("push は確認"));

    let r = run(
        &["check", "git push origin main", "--cwd", "/tmp/scratch"],
        home.path(),
    );
    assert_eq!(r.code, 1);
    assert!(r.stdout.contains("push は確認してください"), "{}", r.stdout);
}

// @kotowari[REQ-017]
#[test]
fn req_017_bad_format_is_a_failure() {
    let home = temp_home();
    let r = run(&["check", "true", "--format", "yaml"], home.path());
    assert_eq!(r.code, 3);
}

// @kotowari[REQ-006]
#[test]
fn req_006_empty_tmpdir_does_not_allow_every_path() {
    // TMPDIR が空文字列のとき、空の許可ルートを足してはならない。
    let r = run_env(
        &["check", "rm -rf /etc/nginx", "--cwd", "/tmp/scratch"],
        &[("HOME", ""), ("XDG_CONFIG_HOME", ""), ("TMPDIR", "")],
    );
    assert_eq!(r.code, 2, "stdout: {} stderr: {}", r.stdout, r.stderr);
    assert!(r.stdout.contains("block"), "{}", r.stdout);
}

// @kotowari[REQ-005]
#[test]
fn req_005_empty_home_keeps_other_homes_protected() {
    // HOME が空文字列のとき、/home 配下はほかの利用者のホームとして保護する。
    let r = run_env(
        &["check", "rm -rf /home/other/x", "--cwd", "/tmp/scratch"],
        &[("HOME", ""), ("XDG_CONFIG_HOME", ""), ("TMPDIR", "/tmp")],
    );
    assert_eq!(r.code, 2, "stdout: {} stderr: {}", r.stdout, r.stderr);
    assert!(r.stdout.contains("block"), "{}", r.stdout);
}
