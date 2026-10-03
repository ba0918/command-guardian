use std::fs;
use std::io::Write;
use std::process::Command;
use std::process::Stdio;

// @kotowari[REQ-advisor-011, EX-advisor-021, EX-advisor-022]
#[test]
fn real_cli_masks_detected_json_model_keys_in_normal_and_debug_advisor_logs() {
    let fake = "sk-fixtureabcdefghijklmnopqrstuvwxyz";
    for debug in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let config = home.path().join("config/command-guardian");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("config.toml"),
            format!("[advisor]\nmode='enforce'\nmodel='{{\"nested\":{{\"{fake}\":\"fixture-value\"}}}}'\ndebug_text={debug}\n"),
        ).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_command-guardian"))
            .args([
                "check",
                "true",
                "--cwd",
                "/fixture/work",
                "--format",
                "json",
            ])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("XDG_STATE_HOME", home.path().join("state"))
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let text =
            fs::read_to_string(home.path().join("state/command-guardian/advisor.jsonl")).unwrap();
        let log: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(log["skip"], "secret");
        assert!(log["failure"].is_null());
        assert!(!text.contains(fake));
    }
}

// @kotowari[REQ-advisor-011, REQ-advisor-018, REQ-advisor-021, EX-advisor-021, EX-advisor-042]
#[test]
fn cli_rejects_linked_or_locked_advice_log_and_keeps_one_mechanical_json_with_payload_free_warning()
{
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};
    for storage in ["symlink", "hardlink", "locked"] {
        let home = tempfile::tempdir().unwrap();
        let config = home.path().join("config/command-guardian");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("config.toml"),
            "[advisor]\nmode='enforce'\ntimeout_ms=500\n",
        )
        .unwrap();
        let state = home.path().join("state/command-guardian");
        fs::create_dir_all(&state).unwrap();
        fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
        let target = home.path().join("fixture-not-written");
        fs::write(&target, "fixture unchanged").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        let log = state.join("advisor.jsonl");
        let lock = match storage {
            "symlink" => {
                std::os::unix::fs::symlink(&target, &log).unwrap();
                None
            }
            "hardlink" => {
                fs::hard_link(&target, &log).unwrap();
                None
            }
            _ => {
                fs::write(&log, "").unwrap();
                fs::set_permissions(&log, fs::Permissions::from_mode(0o600)).unwrap();
                let file = fs::File::open(&log).unwrap();
                rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive).unwrap();
                Some(file)
            }
        };
        let started = Instant::now();
        let output = Command::new(env!("CARGO_BIN_EXE_command-guardian"))
            .args([
                "check",
                "printf fixture-private-log-failure",
                "--cwd",
                "/fixture/private-log-work",
                "--format",
                "json",
            ])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("XDG_STATE_HOME", home.path().join("state"))
            .output()
            .unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["verdict"],
            "allow"
        );
        assert_eq!(output.stderr, b"Warning: Could not write advisor log.\n");
        assert_eq!(fs::read_to_string(&target).unwrap(), "fixture unchanged");
        if storage == "locked" {
            assert_eq!(fs::read_to_string(&log).unwrap(), "");
        }
        drop(lock);
    }
}

// @kotowari[REQ-advisor-009, REQ-advisor-011, REQ-advisor-021, EX-advisor-018, EX-advisor-042]
#[test]
fn real_cli_secret_oversize_and_non_utf8_cwd_preserve_mechanical_result_without_model_authentication(
) {
    for (command, expected) in [
        ("printf api_key=fixture-secret-private".to_owned(), "secret"),
        (format!("printf {}", "x".repeat(70000)), "size"),
    ] {
        let home = tempfile::tempdir().unwrap();
        let config = home.path().join("config/command-guardian");
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("config.toml"), "[advisor]\nmode='enforce'\n").unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_command-guardian"))
            .args([
                "check",
                &command,
                "--cwd",
                "/fixture/work",
                "--format",
                "json",
            ])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("XDG_STATE_HOME", home.path().join("state"))
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["verdict"],
            "allow"
        );
        let text =
            fs::read_to_string(home.path().join("state/command-guardian/advisor.jsonl")).unwrap();
        let log: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(log["skip"], expected);
        assert!(log["failure"].is_null());
        assert!(!text.contains("fixture-secret-private"));
    }
    use std::os::unix::ffi::OsStringExt;
    for (mode, debug) in [
        ("enforce", false),
        ("enforce", true),
        ("observe", false),
        ("observe", true),
    ] {
        let home = tempfile::tempdir().unwrap();
        let config = home.path().join("config/command-guardian");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("config.toml"),
            format!("[advisor]\nmode='{mode}'\ndebug_text={debug}\n"),
        )
        .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_command-guardian"))
            .args(["check", "printf fixture-safe", "--cwd"])
            .arg(std::ffi::OsString::from_vec(
                b"/fixture/nonutf8-\xff".to_vec(),
            ))
            .args(["--format", "json"])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("XDG_STATE_HOME", home.path().join("state"))
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["verdict"],
            "allow"
        );
        assert!(output.stderr.is_empty());
        let text =
            fs::read_to_string(home.path().join("state/command-guardian/advisor.jsonl")).unwrap();
        let log: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(log["skip"], "encoding");
        assert_eq!(log["mode"], mode);
        assert_eq!(log["machine"], "allow");
        assert_eq!(log["final"], "allow");
        assert!(log["failure"].is_null());
        assert!(!text.contains("fixture-safe"));
        assert!(!text.contains("nonutf8"));
        assert!(log["text"].is_null());
    }
}

// @kotowari[REQ-advisor-010, REQ-024, EX-advisor-019]
#[test]
fn real_claude_prompt_hook_updates_only_verified_bounded_cache_and_never_judges_or_logs_advice() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("config/command-guardian");
    fs::create_dir_all(&config).unwrap();
    fs::write(config.join("config.toml"), "[advisor]\nmode='enforce'\n").unwrap();
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/advisor/claude-2.1.288-multiple.json"
    ))
    .unwrap();
    let transcript = home.path().join("fixture-transcript.jsonl");
    fs::write(
        &transcript,
        fixture["records"]
            .as_array()
            .unwrap()
            .iter()
            .take(16)
            .map(|r| format!("{r}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let mut hook = fixture["hook"].clone();
    hook["transcript_path"] = serde_json::json!(transcript);
    hook["cwd"] = serde_json::json!(home.path());
    hook["hook_event_name"] = "UserPromptSubmit".into();
    hook["prompt"] = "replacement fixture instruction".into();
    let mut child = Command::new(env!("CARGO_BIN_EXE_command-guardian"))
        .args(["hook", "--agent", "claude"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_STATE_HOME", home.path().join("state"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(hook.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    let cache = home.path().join("state/command-guardian/advisor-context");
    assert_eq!(fs::read_dir(&cache).unwrap().count(), 1);
    assert!(!home
        .path()
        .join("state/command-guardian/advisor.jsonl")
        .exists());
    use guardian_app::advisor::context::{load_claude_cache, CacheLimits};
    use guardian_app::state::AdvisorState;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    let deadline = Instant::now() + Duration::from_secs(1);
    let state = AdvisorState::open(&home.path().join("state"), deadline).unwrap();
    hook["hook_event_name"] = "PreToolUse".into();
    let context = load_claude_cache(
        &state,
        &hook,
        CacheLimits {
            max_bytes: 131072,
            exchanges: 3,
            ttl: 86400,
            now: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            deadline,
        },
    );
    assert_eq!(
        context.messages.last().unwrap().text,
        "replacement fixture instruction"
    );
}

// @kotowari[REQ-advisor-020, REQ-advisor-021, EX-advisor-040, EX-advisor-047, EX-advisor-049]
#[test]
fn real_opencode_fd_ack_starts_advice_and_missing_ack_or_off_or_block_delivers_mechanical_json_without_advice(
) {
    use std::io::Read;
    use std::os::fd::AsRawFd;
    use std::os::unix::{net::UnixStream, process::CommandExt};
    use std::time::{Duration, Instant};
    unsafe extern "C" {
        fn dup2(old: i32, new: i32) -> i32;
    }
    for behavior in ["ack", "no_ack", "reject", "off", "block"] {
        let home = tempfile::tempdir().unwrap();
        let config = home.path().join("config/command-guardian");
        fs::create_dir_all(&config).unwrap();
        let mode = if behavior == "off" { "off" } else { "enforce" };
        fs::write(
            config.join("config.toml"),
            format!("[advisor]\nmode='{mode}'\ntimeout_ms=10000\n"),
        )
        .unwrap();
        let (mut peer, socket) = UnixStream::pair().unwrap();
        let fd = socket.as_raw_fd();
        let mut command = Command::new(env!("CARGO_BIN_EXE_command-guardian"));
        command
            .args(["hook", "--agent", "opencode"])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("XDG_STATE_HOME", home.path().join("state"))
            .env("COMMAND_GUARDIAN_ADVISOR_CONTROL", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        unsafe {
            command.pre_exec(move || {
                if dup2(fd, 3) < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        let mut child = command.spawn().unwrap();
        drop(socket);
        let input = serde_json::json!({"command":if behavior=="block" {"rm -rf /etc/fixture-protected"}else{"printf fixture-safe"},"cwd":"/fixture/work","shell":"/bin/bash"});
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.to_string().as_bytes())
            .unwrap();
        let thread = std::thread::spawn(move || {
            peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
            let receive = |peer: &mut UnixStream| -> serde_json::Value {
                let mut bytes = Vec::new();
                loop {
                    let mut byte = [0];
                    peer.read_exact(&mut byte).unwrap();
                    if byte[0] == b'\n' {
                        break;
                    }
                    bytes.push(byte[0]);
                }
                serde_json::from_slice(&bytes).unwrap()
            };
            if behavior == "off" {
                assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);
                return;
            }
            let probe = receive(&mut peer);
            assert_eq!(probe["kind"], "budget_probe");
            writeln!(peer,"{}",serde_json::json!({"version":1,"nonce":probe["nonce"],"kind":"budget_reply","original_remaining_ms":5900})).unwrap();
            if behavior == "block" {
                assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);
                return;
            }
            let start = receive(&mut peer);
            assert_eq!(start["kind"], "advisory_start");
            assert_eq!(start["timeout_ms"], 10000);
            if behavior == "no_ack" {
                std::thread::sleep(Duration::from_millis(150));
                return;
            }
            writeln!(peer,"{}",serde_json::json!({"version":1,"nonce":start["nonce"],"kind":"advisory_ack","accepted":behavior=="ack"})).unwrap();
        });
        let started = Instant::now();
        let output = child.wait_with_output().unwrap();
        let elapsed = started.elapsed();
        eprintln!(
            "controlled guardian FD {behavior}: delivery and exit {:.3}ms",
            elapsed.as_secs_f64() * 1000.0
        );
        assert!(elapsed < Duration::from_millis(500));
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["status"], "judged");
        assert_eq!(
            value["verdict"],
            if behavior == "block" {
                "block"
            } else {
                "allow"
            }
        );
        let path = home.path().join("state/command-guardian/advisor.jsonl");
        if behavior == "ack" {
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(fs::read_to_string(path).unwrap().trim())
                    .unwrap()["failure"],
                "authentication"
            );
        } else {
            assert!(!path.exists());
        }
        thread.join().unwrap();
    }
}

// @kotowari[REQ-advisor-021, REQ-advisor-011, EX-advisor-024, EX-advisor-042]
#[test]
fn claude_and_codex_authentication_fallback_keep_allow_silent_and_shadow_keeps_its_original_body_log(
) {
    for agent in ["claude", "codex"] {
        for enforce in [true, false] {
            let home = tempfile::tempdir().unwrap();
            let config = home.path().join("config/command-guardian");
            fs::create_dir_all(&config).unwrap();
            fs::write(
                config.join("config.toml"),
                format!("[mode]\nenforce={enforce}\n[advisor]\nmode='enforce'\n"),
            )
            .unwrap();
            let mut child = Command::new(env!("CARGO_BIN_EXE_command-guardian"))
                .args(["hook", "--agent", agent])
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .env("HOME", home.path())
                .env("XDG_CONFIG_HOME", home.path().join("config"))
                .env("XDG_STATE_HOME", home.path().join("state"))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let input = serde_json::json!({"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"printf fixture-private-hook"},"cwd":"/fixture/private-hook-work","session_id":"fixture-session","turn_id":"fixture-turn","tool_use_id":"fixture-call"});
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.to_string().as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success());
            assert!(output.stdout.is_empty());
            assert!(output.stderr.is_empty());
            let text = fs::read_to_string(home.path().join("state/command-guardian/advisor.jsonl"))
                .unwrap();
            let log: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
            assert_eq!(log["failure"], "authentication");
            assert!(!text.contains("fixture-private-hook"));
            assert!(!text.contains("fixture-session"));
            if !enforce {
                assert!(
                    fs::read_to_string(home.path().join("state/command-guardian/shadow.log"))
                        .unwrap()
                        .contains("fixture-private-hook")
                );
            }
        }
    }
}

// @kotowari[REQ-advisor-021, REQ-advisor-011, EX-advisor-024, EX-advisor-042]
#[test]
fn cli_invokes_adviser_without_authentication_then_emits_original_allow_once_and_private_failure_metadata(
) {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("config/command-guardian");
    fs::create_dir_all(&config).unwrap();
    fs::write(config.join("config.toml"), "[advisor]\nmode='enforce'\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_command-guardian"))
        .args([
            "check",
            "printf fixture-private-command",
            "--cwd",
            "/fixture/private-work",
            "--format",
            "json",
        ])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_STATE_HOME", home.path().join("state"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["verdict"], "allow");
    assert!(output.stderr.is_empty());
    let text =
        fs::read_to_string(home.path().join("state/command-guardian/advisor.jsonl")).unwrap();
    assert_eq!(text.lines().count(), 1);
    let log: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
    assert_eq!(log["failure"], "authentication");
    assert_eq!(log["final"], "allow");
    assert!(!text.contains("fixture-private-command"));
    assert!(!text.contains("/fixture/private-work"));
}

// @kotowari[REQ-advisor-001, EX-advisor-002]
#[test]
fn cli_off_and_mechanical_block_do_not_create_adviser_state_or_send_anything() {
    for (mode, command, code) in [
        ("off", "printf fixture-safe", 0),
        ("enforce", "rm -rf /etc/fixture-protected", 2),
    ] {
        let home = tempfile::tempdir().unwrap();
        let config = home.path().join("config/command-guardian");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("config.toml"),
            format!("[advisor]\nmode='{mode}'\n"),
        )
        .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_command-guardian"))
            .args([
                "check",
                command,
                "--cwd",
                "/fixture/work",
                "--format",
                "json",
            ])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("XDG_STATE_HOME", home.path().join("state"))
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(code));
        assert!(!home.path().join("state").exists());
    }
}
