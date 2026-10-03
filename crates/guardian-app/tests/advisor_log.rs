use guardian_advisor::{Failure, Outcome};
use guardian_app::advisor::{log::record, service::Evaluation, wire::Reply};
use guardian_app::state::AdvisorState;
use guardian_core::Verdict;
use guardian_policy::config::AdvisorConfig;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::time::{Duration, Instant};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(1)
}
fn evaluation() -> Evaluation {
    Evaluation {
        outcome: Outcome {
            candidate: Verdict::Ask,
            final_verdict: Verdict::Allow,
        },
        reply: Some(Reply::Failure {
            failure: Failure::Authentication,
        }),
        failure: Some(Failure::Authentication),
        unreaped: false,
        elapsed_ms: 17,
    }
}

// @kotowari[REQ-advisor-011, EX-advisor-021]
#[test]
fn default_record_contains_only_classified_metadata_and_redacts_detected_model_secret() {
    let root = tempfile::tempdir().unwrap();
    let state = AdvisorState::open(root.path(), deadline()).unwrap();
    let config = AdvisorConfig {
        model: "sk-fixtureabcdefghijklmnopqrstuvwxyz".into(),
        ..Default::default()
    };
    let candidate = r#"{"command":"fixture-command-private","cwd":"/fixture/private","context":"fixture-message-private","session":"fixture-session-private","api_key":"fixture-secret-private"}"#;
    let value = record(100, &config, Verdict::Allow, &evaluation(), Some(candidate));
    state.append_advisor_log(&value, deadline()).unwrap();
    state.append_advisor_log(&value, deadline()).unwrap();
    let path = root.path().join("command-guardian/advisor.jsonl");
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(text.lines().count(), 2);
    for marker in [
        "fixture-command-private",
        "/fixture/private",
        "fixture-message-private",
        "fixture-session-private",
        "fixture-secret-private",
        &config.model,
    ] {
        assert!(!text.contains(marker));
    }
    assert_eq!(value["failure"], "authentication");
    assert_eq!(value["elapsed_ms"], 17);
    assert!(value.get("text").is_none());
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert!(!root.path().join("command-guardian/shadow.log").exists());
}

// @kotowari[REQ-advisor-011, EX-advisor-022]
#[test]
fn explicit_debug_records_only_redacted_candidate_and_log_refuses_links_unsafe_mode_or_expired_deadline()
 {
    let root = tempfile::tempdir().unwrap();
    let state = AdvisorState::open(root.path(), deadline()).unwrap();
    let config = AdvisorConfig {
        debug_text: true,
        ..Default::default()
    };
    let candidate = r#"{"command":"fixture-command","api_key":"fixture-secret-private"}"#;
    let value = record(100, &config, Verdict::Allow, &evaluation(), Some(candidate));
    assert!(value["text"].as_str().unwrap().contains("fixture-command"));
    assert!(!value.to_string().contains("fixture-secret-private"));
    assert_eq!(value["failure"], "authentication");
    state.append_advisor_log(&value, deadline()).unwrap();
    let path = root.path().join("command-guardian/advisor.jsonl");
    let before = fs::read(&path).unwrap();
    assert!(state.append_advisor_log(&value, Instant::now()).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(state.append_advisor_log(&value, deadline()).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let other = root.path().join("other");
    fs::hard_link(&path, &other).unwrap();
    assert!(state.append_advisor_log(&value, deadline()).is_err());
    fs::remove_file(&path).unwrap();
    symlink(&other, &path).unwrap();
    assert!(state.append_advisor_log(&value, deadline()).is_err());
    assert_eq!(fs::read(other).unwrap(), before);
}
