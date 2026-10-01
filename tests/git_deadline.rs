use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// @kotowari[REQ-010, REQ-011, REQ-022, REQ-023, REQ-039]
#[test]
fn req_039_system_git_wait_and_output_are_bounded_and_reaped() {
    for (output, agent) in [
        (false, None),
        (true, None),
        (false, Some("claude")),
        (false, Some("codex")),
    ] {
        let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let root = dir.path().canonicalize().unwrap();
        let repo = root.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let pid_file = root.join("pid");
        let git = bin.join("git");
        let body = if output {
            format!("echo '{}'", "x".repeat(4096))
        } else {
            ":".into()
        };
        std::fs::write(
            &git,
            format!(
                "#!/bin/sh\necho $$ > '{}'\nwhile :; do {body}; done\n",
                pid_file.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o700)).unwrap();
        let stdout = root.join("stdout");
        let input = "xargs rm; rm file";
        let mut command = Command::new(env!("CARGO_BIN_EXE_command-guardian"));
        if let Some(agent) = agent {
            let stdin = root.join("stdin");
            std::fs::write(
                &stdin,
                serde_json::json!({
                    "tool_name": "Bash", "tool_input": {"command": input}, "cwd": repo
                })
                .to_string(),
            )
            .unwrap();
            command
                .args(["hook", "--agent", agent])
                .stdin(Stdio::from(std::fs::File::open(stdin).unwrap()));
        } else {
            command
                .args(["check", input, "--format", "json", "--cwd"])
                .arg(&repo);
        }
        let started = Instant::now();
        let mut child = command
            .env("HOME", &root)
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("TMPDIR", "/tmp")
            .env("PATH", &bin)
            .stdout(Stdio::from(std::fs::File::create(&stdout).unwrap()))
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break Some(status);
            }
            if started.elapsed() > Duration::from_secs(8) {
                child.kill().unwrap();
                child.wait().unwrap();
                break None;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let pid = std::fs::read_to_string(&pid_file).unwrap();
        let proc = std::path::PathBuf::from(format!("/proc/{}", pid.trim()));
        let reaped = !proc.exists();
        if !reaped {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", pid.trim()])
                .status();
        }
        assert!(
            status.is_some(),
            "outer watchdog stopped an unbounded git wait (output={output})"
        );
        assert_eq!(
            status.unwrap().code(),
            Some(if agent.is_some() { 0 } else { 2 })
        );
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(stdout).unwrap()).unwrap();
        let message = if agent.is_some() {
            assert_eq!(json["hookSpecificOutput"]["permissionDecision"], "deny");
            json["hookSpecificOutput"]["permissionDecisionReason"]
                .as_str()
                .unwrap()
        } else {
            assert_eq!(json["verdict"], "block");
            assert!(json["reason"].as_str().unwrap().contains("上限"), "{json}");
            assert_eq!(json["effects"][0]["verdict"], "ask");
            json["message"].as_str().unwrap()
        };
        assert!(message.contains("判定の上限"), "{json}");
        assert!((2..=4).contains(&message.lines().count()), "{json}");
        assert!(reaped, "git child was not reaped");
    }
}

// @kotowari[REQ-010]
#[test]
fn req_010_system_git_start_failure_is_ask_not_limit() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let root = dir.path().canonicalize().unwrap();
    let repo = root.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_command-guardian"))
        .args(["check", "rm file", "--format", "json", "--cwd"])
        .arg(&repo)
        .env("HOME", &root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("TMPDIR", "/tmp")
        .env("PATH", &root)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["verdict"], "ask");
    assert!(json["reason"].as_str().unwrap().contains("git"), "{json}");
}
