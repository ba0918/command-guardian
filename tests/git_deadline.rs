use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// @kotowari[REQ-010, REQ-039]
#[test]
fn req_039_system_git_wait_and_output_are_bounded_and_reaped() {
    for output in [false, true] {
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
        let started = Instant::now();
        let mut child = Command::new(env!("CARGO_BIN_EXE_command-guardian"))
            .args(["check", "rm file", "--format", "json", "--cwd"])
            .arg(&repo)
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
        assert_eq!(status.unwrap().code(), Some(2));
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(stdout).unwrap()).unwrap();
        assert_eq!(json["verdict"], "block");
        assert!(json["reason"].as_str().unwrap().contains("上限"), "{json}");
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
