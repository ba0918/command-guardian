use guardian_app::state::AdvisorState;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::time::{Duration, Instant};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(2)
}

// @kotowari[REQ-advisor-010, EX-advisor-019]
#[test]
fn bounded_cache_is_private_and_only_reused_for_its_session_before_expiry() {
    let root = tempfile::tempdir().unwrap();
    let state = AdvisorState::open(root.path(), deadline()).unwrap();
    state
        .save_context(
            "fixture-session/../../escape",
            b"fixture bounded window",
            100,
            deadline(),
        )
        .unwrap();
    assert_eq!(
        state
            .load_context(
                "fixture-session/../../escape",
                101,
                86400,
                65536,
                deadline()
            )
            .unwrap(),
        Some(b"fixture bounded window".to_vec())
    );
    assert_eq!(
        state
            .load_context("other-session", 101, 86400, 65536, deadline())
            .unwrap(),
        None
    );
    let dir = root.path().join("command-guardian/advisor-context");
    assert_eq!(
        fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let entries = fs::read_dir(&dir)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert!(!entries[0]
        .file_name()
        .to_string_lossy()
        .contains("fixture-session"));
    assert_eq!(
        entries[0].metadata().unwrap().permissions().mode() & 0o777,
        0o600
    );
}

// @kotowari[REQ-advisor-010, EX-advisor-020]
#[test]
fn exact_ttl_future_clock_and_expired_deadline_never_return_old_context() {
    let root = tempfile::tempdir().unwrap();
    let state = AdvisorState::open(root.path(), deadline()).unwrap();
    state
        .save_context("fixture-session", b"fixture window", 100, deadline())
        .unwrap();
    assert!(state
        .load_context("fixture-session", 99, 86400, 65536, deadline())
        .unwrap()
        .is_none());
    assert!(state
        .load_context("fixture-session", 100 + 86400, 86400, 65536, deadline())
        .unwrap()
        .is_none());
    assert_eq!(
        fs::read_dir(root.path().join("command-guardian/advisor-context"))
            .unwrap()
            .count(),
        0
    );
    assert!(state
        .save_context(
            "fixture-session",
            b"late window",
            100,
            Instant::now() - Duration::from_millis(1)
        )
        .is_err());
}

// @kotowari[REQ-advisor-010, EX-advisor-020]
#[test]
fn symlinks_and_multiple_hardlinks_are_rejected_without_changing_their_target() {
    let root = tempfile::tempdir().unwrap();
    let state = AdvisorState::open(root.path(), deadline()).unwrap();
    state
        .save_context("fixture-session", b"fixture window", 100, deadline())
        .unwrap();
    let dir = root.path().join("command-guardian/advisor-context");
    let file = fs::read_dir(&dir).unwrap().next().unwrap().unwrap().path();
    let link = root.path().join("hardlink");
    fs::hard_link(&file, &link).unwrap();
    let before = fs::read(&link).unwrap();
    assert!(state
        .load_context("fixture-session", 101, 86400, 65536, deadline())
        .is_err());
    assert!(state
        .save_context("fixture-session", b"replacement", 102, deadline())
        .is_err());
    assert_eq!(fs::read(&link).unwrap(), before);
    fs::remove_file(&file).unwrap();
    symlink(&link, &file).unwrap();
    assert!(state
        .load_context("fixture-session", 101, 86400, 65536, deadline())
        .is_err());
    assert!(state
        .save_context("fixture-session", b"replacement", 102, deadline())
        .is_err());
    assert_eq!(fs::read(&link).unwrap(), before);
}

// @kotowari[REQ-advisor-010]
#[test]
fn malformed_oversized_and_insecure_cache_files_are_not_adopted() {
    let root = tempfile::tempdir().unwrap();
    let state = AdvisorState::open(root.path(), deadline()).unwrap();
    state
        .save_context("fixture-session", b"fixture window", 100, deadline())
        .unwrap();
    let file = fs::read_dir(root.path().join("command-guardian/advisor-context"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert!(state
        .load_context("fixture-session", 101, 86400, 2, deadline())
        .is_err());
    fs::write(&file, b"malformed frame").unwrap();
    assert!(state
        .load_context("fixture-session", 101, 86400, 65536, deadline())
        .is_err());
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(state
        .load_context("fixture-session", 101, 86400, 65536, deadline())
        .is_err());
}

// @kotowari[REQ-advisor-010, EX-advisor-020]
#[test]
fn directory_symlink_or_wider_permissions_never_enable_cache_access() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("command-guardian")).unwrap();
    fs::set_permissions(
        root.path().join("command-guardian"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    symlink(
        outside.path(),
        root.path().join("command-guardian/advisor-context"),
    )
    .unwrap();
    let state = AdvisorState::open(root.path(), deadline()).unwrap();
    assert!(state
        .save_context("fixture-session", b"fixture window", 100, deadline())
        .is_err());
    assert!(state
        .load_context("fixture-session", 101, 86400, 65536, deadline())
        .is_err());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    fs::set_permissions(
        root.path().join("command-guardian"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert!(AdvisorState::open(root.path(), deadline()).is_err());
}

// @kotowari[REQ-advisor-010]
#[test]
fn opening_state_alone_does_not_create_a_conversation_cache() {
    let root = tempfile::tempdir().unwrap();
    let state = AdvisorState::open(root.path(), deadline()).unwrap();
    assert_eq!(
        state
            .load_context("fixture-session", 101, 86400, 65536, deadline())
            .unwrap(),
        None
    );
    assert!(!root
        .path()
        .join("command-guardian/advisor-context")
        .exists());
}

// @kotowari[REQ-advisor-010]
#[test]
fn acquisition_bound_includes_the_encoded_cache_frame_not_just_its_payload() {
    let root = tempfile::tempdir().unwrap();
    let state = AdvisorState::open(root.path(), deadline()).unwrap();
    state
        .save_context("fixture-session", b"fixture window", 100, deadline())
        .unwrap();
    let file = fs::read_dir(root.path().join("command-guardian/advisor-context"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let encoded_size = fs::metadata(file).unwrap().len() as usize;
    assert!(state
        .load_context("fixture-session", 101, 86400, encoded_size - 1, deadline())
        .is_err());
    assert!(state
        .load_context("fixture-session", 101, 86400, encoded_size, deadline())
        .unwrap()
        .is_some());
}

// @kotowari[REQ-advisor-010, EX-advisor-020]
#[test]
fn invalidating_a_failed_update_prevents_reusing_the_previous_prompt() {
    let root = tempfile::tempdir().unwrap();
    let state = AdvisorState::open(root.path(), deadline()).unwrap();
    state
        .save_context("fixture-session", b"old prompt", 100, deadline())
        .unwrap();
    state.remove_context("fixture-session", deadline()).unwrap();
    assert!(state
        .load_context("fixture-session", 101, 86400, 65536, deadline())
        .unwrap()
        .is_none());
    state.remove_context("fixture-session", deadline()).unwrap();
}
