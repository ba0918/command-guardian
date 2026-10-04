use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new(shadow: bool) -> Self {
        Self::with_config(&format!("[mode]\nenforce = {}\n", !shadow))
    }

    fn with_config(text: &str) -> Self {
        let root = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let config = root.path().join("config/command-guardian");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("config.toml"), text).unwrap();
        Self { root }
    }

    fn log(&self) -> Option<String> {
        std::fs::read_to_string(self.root.path().join("state/command-guardian/shadow.log")).ok()
    }

    fn run(&self, input: &str) -> (Value, String) {
        let mut child = Command::new(env!("CARGO_BIN_EXE_command-guardian"))
            .args(["hook", "--agent", "opencode"])
            .env("HOME", self.root.path())
            .env("XDG_CONFIG_HOME", self.root.path().join("config"))
            .env("XDG_STATE_HOME", self.root.path().join("state"))
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
        assert_eq!(out.status.code(), Some(0));
        let response = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
        (response, String::from_utf8(out.stderr).unwrap())
    }

    fn input(&self, command: &str, shell: &str) -> String {
        json!({"command": command, "cwd": self.root.path(), "shell": shell}).to_string()
    }
}

// @kotowari[REQ-016, REQ-048]
#[test]
fn req_048_opencode_returns_each_verdict_without_writing_shadow_logs() {
    let f = Fixture::new(false);
    for (command, verdict) in [
        ("true", "allow"),
        ("rm /etc/x", "block"),
        ("eval \"$UNKNOWN\"", "ask"),
    ] {
        let (response, _) = f.run(&f.input(command, "/bin/bash"));
        assert_eq!(response["status"], "judged", "{command}: {response}");
        assert_eq!(response["mode"]["enforce"], true);
        assert_eq!(response["verdict"], verdict);
        assert!(response["reason"].is_string());
    }
    assert!(
        !f.root
            .path()
            .join("state/command-guardian/shadow.log")
            .exists()
    );
}

// @kotowari[REQ-048, REQ-051, REQ-055]
#[test]
fn req_048_invalid_input_and_unsupported_shell_are_not_shadow_success() {
    let f = Fixture::new(false);
    for input in [
        "{".to_string(),
        "{}".to_string(),
        f.input("true", "/bin/zsh"),
    ] {
        let (response, _) = f.run(&input);
        assert_eq!(response["status"], "unavailable");
        assert!(response["reason"].as_str().is_some_and(|s| !s.is_empty()));
        assert!(response.get("verdict").is_none());
    }
}

// @kotowari[REQ-018, REQ-048, EX-080]
#[test]
fn req_048_shadow_logs_but_never_returns_a_verdict() {
    let f = Fixture::new(true);
    let (response, _) = f.run(&f.input("rm /etc/x", "/bin/bash"));
    assert_eq!(response["status"], "shadow");
    assert_eq!(response["mode"]["enforce"], false);
    assert!(response.get("verdict").is_none());
    let log =
        std::fs::read_to_string(f.root.path().join("state/command-guardian/shadow.log")).unwrap();
    assert!(log.contains("block") && log.contains("rm /etc/x"));
}

// @kotowari[REQ-048, EX-104]
#[test]
fn req_048_log_failure_keeps_confirmed_shadow_mode() {
    let f = Fixture::new(true);
    std::fs::create_dir_all(f.root.path().join("state/command-guardian/shadow.log")).unwrap();
    let (response, stderr) = f.run(&f.input("rm /etc/x", "/bin/bash"));
    assert_eq!(response["status"], "shadow");
    assert_eq!(response["mode"]["enforce"], false);
    assert!(stderr.contains("shadow log"));
}

// @kotowari[REQ-048, REQ-055, EX-105]
#[test]
fn req_048_unsupported_shell_in_shadow_warns_without_bash_judgment() {
    let f = Fixture::new(true);
    let (response, stderr) = f.run(&f.input("rm /etc/x", "/bin/zsh"));
    assert_eq!(response["status"], "shadow");
    assert_eq!(response["mode"]["enforce"], false);
    assert!(response.get("verdict").is_none());
    assert!(stderr.contains("shell"));
    assert!(
        !f.root
            .path()
            .join("state/command-guardian/shadow.log")
            .exists()
    );
}

// @kotowari[REQ-061, REQ-062, EX-123]
#[test]
fn ex_123_opencode_deferred_ask_is_distinct_from_allow_and_recorded() {
    let f = Fixture::with_config("[mode]\nenforce = true\ndefer_ask = true\n");
    let (response, _) = f.run(&f.input("rm /mnt/fixture/x", "/bin/bash"));
    assert_eq!(response["status"], "deferred", "{response}");
    assert_eq!(response["mode"]["enforce"], true);
    assert!(response.get("verdict").is_none(), "{response}");
    assert!(response.get("warning").is_none(), "{response}");
    assert!(response["reason"].as_str().is_some_and(|s| !s.is_empty()));
    let log = f.log().unwrap();
    assert!(log.trim_end().ends_with("\tdeferred"), "{log}");
    for (command, verdict) in [("true", "allow"), ("rm /etc/x", "block")] {
        let (response, _) = f.run(&f.input(command, "/bin/bash"));
        assert_eq!(response["status"], "judged", "{command}: {response}");
        assert_eq!(response["verdict"], verdict);
    }
    assert_eq!(f.log().unwrap().lines().count(), 1);
}

// @kotowari[REQ-061, REQ-062, EX-131]
#[test]
fn ex_131_opencode_record_failure_keeps_the_deferral_with_a_warning() {
    let f = Fixture::with_config("[mode]\nenforce = true\ndefer_ask = true\n");
    let sentinel = f.root.path().join("sentinel");
    std::fs::write(&sentinel, "unchanged").unwrap();
    let dir = f.root.path().join("state/command-guardian");
    std::fs::create_dir_all(&dir).unwrap();
    std::os::unix::fs::symlink(&sentinel, dir.join("shadow.log")).unwrap();
    let (response, stderr) = f.run(&f.input("rm /mnt/fixture/x", "/bin/bash"));
    assert_eq!(response["status"], "deferred", "{response}");
    assert_eq!(response["mode"]["enforce"], true);
    assert!(
        response["warning"]
            .as_str()
            .is_some_and(|w| w.contains("shadow log")),
        "{response}"
    );
    assert!(stderr.contains("shadow log"), "{stderr}");
    assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "unchanged");
}

// @kotowari[REQ-063, REQ-048]
#[test]
fn req_063_opencode_shadow_mode_takes_precedence_over_defer_ask() {
    let f = Fixture::with_config("[mode]\nenforce = false\ndefer_ask = true\n");
    let (response, _) = f.run(&f.input("rm /mnt/fixture/x", "/bin/bash"));
    assert_eq!(response["status"], "shadow", "{response}");
    assert_eq!(response["mode"]["enforce"], false);
    assert!(response.get("verdict").is_none(), "{response}");
    assert!(response.get("warning").is_none(), "{response}");
    let log = f.log().unwrap();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 1, "{log}");
    let fields: Vec<&str> = lines[0].split('\t').collect();
    assert_eq!(fields.len(), 5, "{log}");
    assert_eq!(fields[1], "ask");
}
