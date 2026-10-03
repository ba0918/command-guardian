use guardian_app::advisor::wire::{FrameKind, Reply, read_reply, write_frame};
use guardian_app::advisor::worker::{Request, Settings, Source};
use std::io::Write;
use std::net::Shutdown;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// @kotowari[REQ-advisor-011, EX-advisor-021, EX-advisor-022]
#[test]
fn real_advice_child_delivers_authentication_failure_before_private_log_and_reports_unsafe_save_without_payload()
 {
    for debug in [false, true] {
        for unsafe_save in [false, true] {
            let home = tempfile::tempdir().unwrap();
            if unsafe_save {
                std::fs::create_dir(home.path().join("command-guardian")).unwrap();
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    home.path().join("command-guardian"),
                    std::fs::Permissions::from_mode(0o700),
                )
                .unwrap();
                std::os::unix::fs::symlink(
                    home.path().join("not-written"),
                    home.path().join("command-guardian/advisor.jsonl"),
                )
                .unwrap();
            }
            let (mut parent, child_socket) = UnixStream::pair().unwrap();
            let mut child = Command::new(env!("CARGO_BIN_EXE_command-guardian"))
                .env_clear()
                .env("HOME", home.path())
                .env("XDG_CONFIG_HOME", home.path())
                .env("XDG_STATE_HOME", home.path())
                .env(
                    "COMMAND_GUARDIAN_ADVISOR_WORKER",
                    "07070707070707070707070707070707",
                )
                .env("COMMAND_GUARDIAN_ADVISOR_BUDGET_NS", "2000000000")
                .env("COMMAND_GUARDIAN_ADVISOR_FRAME_LIMIT", "131072")
                .stdin(Stdio::from(OwnedFd::from(child_socket)))
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let request = serde_json::json!({"command":"printf fixture-private-command","cwd":"/fixture/private-work","machine":"allow","reasons":[],"source":{"host":"cli"},"state_base":home.path(),
                "settings":{"model":"fixture-model","max_request_bytes":65536,"context_exchanges":0,"context_ttl_seconds":86400},
                "log":{"mode":"enforce","threshold":0.9,"debug_text":debug}});
            let deadline = Instant::now() + Duration::from_secs(2);
            write_frame(
                &mut parent,
                [7; 16],
                FrameKind::Request,
                &request,
                131072,
                deadline,
            )
            .unwrap();
            assert_eq!(
                serde_json::to_value(read_reply(&mut parent, [7; 16], deadline).unwrap()).unwrap()
                    ["failure"],
                "authentication"
            );
            while Instant::now() < deadline && child.try_wait().unwrap().is_none() {
                std::thread::sleep(Duration::from_millis(1));
            }
            if child.try_wait().unwrap().is_none() {
                child.kill().unwrap();
                panic!("log child exceeded deadline");
            }
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success());
            assert!(output.stdout.is_empty());
            if unsafe_save {
                assert_eq!(output.stderr, b"Warning: Could not write advisor log.\n");
                assert!(!home.path().join("not-written").exists());
            } else {
                assert!(output.stderr.is_empty());
                let text =
                    std::fs::read_to_string(home.path().join("command-guardian/advisor.jsonl"))
                        .unwrap();
                let value: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
                assert_eq!(value["failure"], "authentication");
                assert_eq!(value["final"], "allow");
                assert_eq!(text.contains("fixture-private-command"), debug);
                assert_eq!(text.contains("/fixture/private-work"), debug);
            }
        }
    }
}

// @kotowari[REQ-advisor-018, REQ-advisor-019, EX-advisor-038]
#[test]
fn real_binary_rejects_invalid_version_nonce_kind_length_and_request_schema_without_public_output()
{
    let home = tempfile::tempdir().unwrap();
    for variant in 0..7 {
        let (mut parent, child_socket) = UnixStream::pair().unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_command-guardian"))
            .arg("--help")
            .env_clear()
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path())
            .env("XDG_STATE_HOME", home.path())
            .env(
                "COMMAND_GUARDIAN_ADVISOR_WORKER",
                "07070707070707070707070707070707",
            )
            .env("COMMAND_GUARDIAN_ADVISOR_BUDGET_NS", "200000000")
            .env("COMMAND_GUARDIAN_ADVISOR_FRAME_LIMIT", "131072")
            .stdin(Stdio::from(OwnedFd::from(child_socket)))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        parent
            .set_write_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let mut bytes = vec![1];
        bytes.extend([7; 16]);
        bytes.push(1);
        let body = match variant {
            4 => br#"{"command":"x","command":"y"}"#.as_slice(),
            5 => b"\xff".as_slice(),
            _ => b"{}".as_slice(),
        };
        bytes.extend((body.len() as u32).to_be_bytes());
        bytes.extend(body);
        match variant {
            0 => bytes[0] = 2,
            1 => bytes[1] = 8,
            2 => bytes[17] = 2,
            3 => bytes[18..22].copy_from_slice(&u32::MAX.to_be_bytes()),
            6 => bytes.extend(b"trailing"),
            _ => (),
        }
        parent.write_all(&bytes).unwrap();
        let _ = parent.shutdown(Shutdown::Write);
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline && child.try_wait().unwrap().is_none() {
            std::thread::sleep(Duration::from_millis(1));
        }
        if child.try_wait().unwrap().is_none() {
            child.kill().unwrap();
            panic!("invalid frame child exceeded deadline");
        }
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success(), "variant {variant}");
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
}

// @kotowari[REQ-advisor-018, REQ-advisor-019, REQ-advisor-012]
#[test]
fn real_binary_dispatch_uses_inherited_socket_before_public_arguments_and_missing_auth_has_no_stdout()
 {
    let home = tempfile::tempdir().unwrap();
    let (mut parent, child_socket) = UnixStream::pair().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_command-guardian"))
        .arg("--help")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path())
        .env("XDG_STATE_HOME", home.path())
        .env(
            "COMMAND_GUARDIAN_ADVISOR_WORKER",
            "07070707070707070707070707070707",
        )
        .env("COMMAND_GUARDIAN_ADVISOR_BUDGET_NS", "2000000000")
        .env("COMMAND_GUARDIAN_ADVISOR_FRAME_LIMIT", "131072")
        .stdin(Stdio::from(OwnedFd::from(child_socket)))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let request = Request {
        command: "printf fixture-safe".into(),
        cwd: "/fixture/work".into(),
        machine: "allow".into(),
        reasons: Vec::new(),
        source: Source::Cli,
        state_base: None,
        log: None,
        settings: Settings {
            model: "fixture-model".into(),
            max_request_bytes: 65536,
            context_exchanges: 3,
            context_ttl_seconds: 86400,
        },
    };
    write_frame(
        &mut parent,
        [7; 16],
        FrameKind::Request,
        &request,
        131072,
        deadline,
    )
    .unwrap();
    let reply = read_reply(&mut parent, [7; 16], deadline).unwrap();
    assert!(matches!(reply, Reply::Failure { .. }));
    assert_eq!(
        serde_json::to_value(reply).unwrap()["failure"],
        "authentication"
    );
    while Instant::now() < deadline && child.try_wait().unwrap().is_none() {
        std::thread::sleep(Duration::from_millis(1));
    }
    if child.try_wait().unwrap().is_none() {
        child.kill().unwrap();
        panic!("fixture child did not finish within deadline");
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
