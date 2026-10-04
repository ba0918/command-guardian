use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn fixture() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap()
}

fn command(home: &std::path::Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_command-guardian"));
    cmd.env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("TMPDIR", "/tmp")
        .current_dir(home);
    cmd
}

fn config(home: &std::path::Path, text: &str) {
    let path = home.join(".config/command-guardian/config.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn json_check(home: &std::path::Path, text: &str) -> (i32, serde_json::Value) {
    let out = command(home)
        .args(["check", text, "--cwd", "/tmp", "--format", "json"])
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        serde_json::from_slice(&out.stdout).unwrap(),
    )
}

/// stdin を開いたままでも終わることを確かめてから、出力を集める。
fn output_without_reading_stdin(
    mut child: std::process::Child,
    label: &str,
) -> std::process::Output {
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(3) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("{label} waited for stdin");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().unwrap()
}

// @kotowari[REQ-011, REQ-017, EX-058]
#[test]
fn non_allow_text_output_including_verdict_stays_within_four_lines() {
    let home = fixture();
    let output = command(home.path())
        .args(["check", "rm /etc/a /etc/b", "--cwd", "/tmp"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!((2..=4).contains(&text.lines().count()), "{text}");
    let (code, json) = json_check(home.path(), "rm /etc/a /etc/b");
    assert_eq!(code, 2);
    let message = json["message"].as_str().unwrap();
    for line in message.lines() {
        assert!(text.contains(line), "Missing explanation: {line}");
    }
    assert!(text.lines().next().unwrap().contains("Verdict: block"));
}

// @kotowari[REQ-044, EX-066, EX-067]
#[test]
fn help_at_each_entry_exits_without_reading_open_stdin_or_broken_config() {
    let home = fixture();
    config(home.path(), "broken = [");
    for entry in [None, Some("check"), Some("hook")] {
        for flag in ["--help", "-h"] {
            let mut cmd = command(home.path());
            if let Some(entry) = entry {
                cmd.arg(entry);
            }
            let child = cmd
                .arg(flag)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let out = output_without_reading_stdin(child, &format!("help {entry:?} {flag}"));
            assert!(out.status.success());
            assert!(out.stderr.is_empty(), "{:?}", out.stderr);
            let help = String::from_utf8(out.stdout).unwrap();
            for label in [
                "Usage:",
                "Options:",
                "Examples:",
                "Exit codes:",
                "--help",
                "-h",
            ] {
                assert!(help.contains(label), "{help}");
            }
            match entry {
                Some("check") => {
                    assert!(help.contains("--cwd"));
                    assert!(help.contains("current working directory"));
                    assert!(help.contains("--format text|json"));
                    assert!(help.contains("default: text"));
                    for code in ["0 allow", "1 ask", "2 block", "3"] {
                        assert!(help.contains(code));
                    }
                }
                Some("hook") => {
                    assert!(help.contains("stdin"));
                    assert!(help.contains("--agent claude|codex"));
                    assert!(help.contains("always 0"));
                }
                _ => {
                    assert!(help.contains("check"));
                    assert!(help.contains("hook"));
                    assert!(help.contains("--version, -V"), "{help}");
                }
            }
        }
    }
}

fn manifest_version() -> String {
    let manifest =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    let value: toml::Table = manifest.parse().unwrap();
    value["package"]["version"].as_str().unwrap().to_string()
}

// @kotowari[REQ-071, EX-154]
#[test]
fn ex_154_version_prints_the_manifest_version_without_reading_open_stdin_or_broken_config() {
    let home = fixture();
    config(home.path(), "broken = [");
    for flag in ["--version", "-V"] {
        let child = command(home.path())
            .arg(flag)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let out = output_without_reading_stdin(child, &format!("version {flag}"));
        assert_eq!(out.status.code(), Some(0), "{flag}");
        assert!(out.stderr.is_empty(), "{:?}", out.stderr);
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            format!("command-guardian {}\n", manifest_version())
        );
    }
}

// @kotowari[REQ-071, EX-155]
#[test]
fn ex_155_version_after_hook_is_handled_by_the_hook_entry() {
    let home = fixture();
    let version_line = format!("command-guardian {}", manifest_version());
    let out = command(home.path())
        .args(["hook", "--version"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty(), "{:?}", out.stdout);
    let mut child = command(home.path())
        .args(["hook", "--version", "--agent", "claude"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = serde_json::json!({"hook_event_name":"PreToolUse", "tool_name":"Bash", "tool_input":{"command":"rm -rf /etc/nginx"}, "cwd":"/tmp"});
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(!text.contains(&version_line), "{text}");
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["hookSpecificOutput"]["permissionDecision"], "deny");
}

// @kotowari[REQ-071]
#[test]
fn req_071_version_after_check_is_not_a_version_request() {
    let home = fixture();
    let out = command(home.path())
        .args(["check", "--version"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty(), "{:?}", out.stdout);
}

// @kotowari[REQ-044, EX-068]
#[test]
fn delimiter_and_option_values_do_not_trigger_help() {
    let home = fixture();
    config(
        home.path(),
        "[[rules.custom]]\nname = 'help-like command'\npattern = '^(-h|--help)$'\nverdict = 'ask'\n",
    );
    for literal in ["--help", "-h"] {
        let out = command(home.path())
            .args(["check", "--format", "json", "--", literal])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(json["verdict"], "ask");
        assert_eq!(json["rules"][0]["program"], "help-like command");
        assert!(json["effects"].as_array().unwrap().is_empty());
    }
    let out = command(home.path())
        .args(["check", "true", "--cwd", "--help"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "Verdict: allow\n");
    let out = command(home.path())
        .args(["check", "true", "--format", "--help"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("Unknown --format")
    );
    let out = command(home.path())
        .args(["hook", "--agent", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
}

// @kotowari[REQ-043, EX-063]
#[test]
fn custom_name_and_path_remain_original_json_data() {
    let home = fixture();
    config(
        home.path(),
        "[[rules.custom]]\nname = '削除確認'\npattern = 'rm'\nverdict = 'ask'\n",
    );
    let (code, value) = json_check(home.path(), "rm 資料.txt");
    assert_eq!(code, 1);
    assert_eq!(value["rules"][0]["program"], "削除確認");
    assert!(
        value["rules"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("削除確認")
    );
    assert_eq!(value["effects"][0]["path"], "/tmp/資料.txt");
    let mut keys: Vec<_> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["effects", "message", "reason", "rules", "verdict"]);
}

// @kotowari[REQ-043, EX-072, EX-073]
#[test]
fn guard_reason_retains_raw_newlines_and_escapes_only_message() {
    let home = fixture();
    for reason in [
        "先にバックアップを確認",
        "先に確認\n承認後に実行",
        "先に確認\r\n承認後に実行\\n",
    ] {
        let escaped = reason
            .replace('\\', "\\\\")
            .replace('\r', "\\r")
            .replace('\n', "\\n");
        config(
            home.path(),
            &format!(
                "[[commands.guard]]\nprogram = 'git'\ndeny = [['push']]\nverdict = 'ask'\nreason = \"{escaped}\"\n"
            ),
        );
        let (code, value) = json_check(home.path(), "git push origin main");
        assert_eq!(code, 1);
        assert_eq!(value["rules"][0]["reason"], reason);
        let message = value["message"].as_str().unwrap();
        assert!(message.contains(&escaped), "{message:?}");
        assert!((2..=4).contains(&message.lines().count()));
        assert!(message.contains("Verdict: ask"));
        assert!(message.contains("Alternative:"), "{message}");
    }
}

// @kotowari[REQ-043, REQ-011]
#[test]
fn custom_name_with_newlines_is_preserved_and_has_single_line_display() {
    let home = fixture();
    config(
        home.path(),
        "[[rules.custom]]\nname = \"削除\\r\\n確認\\\\n\"\npattern = 'rm'\nverdict = 'ask'\n",
    );
    let (code, value) = json_check(home.path(), "rm 資料.txt");
    assert_eq!(code, 1);
    assert_eq!(value["rules"][0]["program"], "削除\r\n確認\\n");
    let message = value["message"].as_str().unwrap();
    assert!(message.contains("削除\\r\\n確認\\\\n"));
    assert!((2..=4).contains(&message.lines().count()));
}

// @kotowari[REQ-044, REQ-017]
#[test]
fn check_defaults_to_current_directory_and_text_without_changing_argument_contracts() {
    let home = fixture();
    let cwd = home.path().join("work");
    std::fs::create_dir(&cwd).unwrap();
    let out = command(home.path())
        .current_dir(&cwd)
        .args(["check", "rm ."])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.starts_with("Verdict: block"));
    assert!(text.contains(home.path().to_str().unwrap()));
    assert!(text.contains("working directory"));
    for args in [
        vec!["check", "true", "false"],
        vec!["check", "--", "true", "false"],
        vec!["check", "true", "--cwd"],
        vec!["check", "true", "--format"],
        vec!["check", "true", "--unknown"],
    ] {
        let out = command(home.path()).args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
        assert!(!out.stderr.is_empty());
    }
}

// @kotowari[REQ-043, EX-064]
#[test]
fn warning_retains_the_real_toml_library_error_detail() {
    let home = fixture();
    let text = "日本語 = [";
    config(home.path(), text);
    let detail =
        guardian_policy::layers::parse_layer(text, home.path(), Some(home.path())).unwrap_err();
    let out = command(home.path())
        .args(["check", "true", "--cwd", "/tmp"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let warning = String::from_utf8(out.stderr).unwrap();
    assert!(warning.contains(&detail), "{warning}");
}

// @kotowari[REQ-043, EX-065]
#[test]
fn shadow_preserves_command_while_hook_and_log_use_english_messages() {
    let home = fixture();
    for enforce in [false, true] {
        config(
            home.path(),
            &format!(
                "[mode]\nenforce = {enforce}\n[[rules.custom]]\nname = '削除確認'\npattern = 'rm'\nverdict = 'ask'\n"
            ),
        );
        for agent in ["claude", "codex"] {
            let text = if enforce {
                "rm -rf /etc/nginx"
            } else {
                "rm '資料.txt'"
            };
            let mut child = command(home.path())
                .args(["hook", "--agent", agent])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let input = serde_json::json!({"tool_name":"Bash", "tool_input":{"command":text}, "cwd":"/tmp"});
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.to_string().as_bytes())
                .unwrap();
            let out = child.wait_with_output().unwrap();
            assert!(out.status.success());
            if enforce {
                let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
                let message = json["hookSpecificOutput"]["permissionDecisionReason"]
                    .as_str()
                    .unwrap();
                assert!(message.contains("Reason:"));
                assert!(message.contains("Alternative:"));
            } else {
                assert!(out.stdout.is_empty());
            }
        }
    }
    let path = home.path().join("state/command-guardian");
    let logs: String = std::fs::read_dir(path)
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect();
    for line in logs.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 5);
        assert_eq!(fields[4], "rm '資料.txt'");
        assert!(fields[2].contains("Custom rule"), "{line}");
        assert!(fields[2].contains("Verdict: ask"), "{line}");
    }
}

// @kotowari[REQ-043, REQ-011]
#[test]
fn newline_paths_have_original_data_and_bounded_display() {
    let home = fixture();
    let (code, value) = json_check(home.path(), "rm '/etc/資料\r\n末尾\\n'");
    assert_eq!(code, 2);
    assert_eq!(value["effects"][0]["path"], "/etc/資料\r\n末尾\\n");
    let message = value["message"].as_str().unwrap();
    assert!(message.contains("/etc/資料\\r\\n末尾\\\\n"), "{message:?}");
    assert!((2..=4).contains(&message.lines().count()));
    assert!(message.contains("Reason:"));
    assert!(message.contains("Alternative:"));
}
