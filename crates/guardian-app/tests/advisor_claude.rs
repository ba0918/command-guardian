use guardian_app::advisor::context::claude_context;
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/advisor/claude-2.1.288-multiple.json"
    ))
    .unwrap()
}

fn bytes(value: &Value) -> Vec<u8> {
    value["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| format!("{r}\n"))
        .collect::<String>()
        .into_bytes()
}

fn before_submit(value: &Value) -> Vec<u8> {
    value["records"]
        .as_array()
        .unwrap()
        .iter()
        .take(16)
        .map(|r| format!("{r}\n"))
        .collect::<String>()
        .into_bytes()
}

// @kotowari[REQ-advisor-006, REQ-advisor-007, EX-advisor-011, EX-advisor-014]
#[test]
fn resumed_claude_prompt_preserves_roles_and_excludes_tool_bodies_before_current_call() {
    let value = fixture();
    let context = claude_context(&value["hook"], &bytes(&value), 65536, 3);
    assert_eq!(context.messages.len(), 3);
    assert_eq!(
        context.messages[0].text,
        "Fixture user request: print fixture-safe only."
    );
    assert_eq!(context.messages[1].text, "fixture assistant response");
    assert_eq!(
        context.messages[2].text,
        "Fixture follow-up: print fixture-safe only, previous request is withdrawn."
    );
    assert!(context.excluded_reference_material);
    let last_only = claude_context(&value["hook"], &bytes(&value), 65536, 1);
    assert_eq!(last_only.messages.len(), 1);
    assert_eq!(last_only.messages[0].text, context.messages[2].text);
    assert_eq!(last_only.messages[0].relative_order, 0);
}

// @kotowari[REQ-advisor-006, EX-advisor-012]
#[test]
fn unsupported_version_sidechain_ambiguous_chain_or_prompt_never_establishes_claude_context() {
    let original = fixture();
    for field in ["session_id", "prompt_id", "tool_use_id"] {
        let mut value = original.clone();
        value["hook"][field] = "unmatched".into();
        assert!(
            claude_context(&value["hook"], &bytes(&value), 65536, 3)
                .messages
                .is_empty()
        );
    }
    for (field, replacement) in [
        ("version", Value::from("unknown")),
        ("isSidechain", Value::from(true)),
        ("parentUuid", Value::from("unmatched")),
    ] {
        let mut value = original.clone();
        value["records"][18][field] = replacement;
        assert!(
            claude_context(&value["hook"], &bytes(&value), 65536, 3)
                .messages
                .is_empty()
        );
    }
    let mut duplicate = original.clone();
    duplicate["records"][18]["uuid"] = duplicate["records"][0]["uuid"].clone();
    assert!(
        claude_context(&duplicate["hook"], &bytes(&duplicate), 65536, 3)
            .messages
            .is_empty()
    );
    assert!(
        claude_context(&original["hook"], &bytes(&original), 1, 3)
            .messages
            .is_empty()
    );
}

// @kotowari[REQ-advisor-006, REQ-advisor-010, EX-advisor-019, EX-advisor-020]
#[test]
fn separate_claude_hook_cache_binds_the_pending_prompt_to_verified_source_and_never_restores_an_old_prompt()
 {
    use guardian_app::advisor::context::{CacheLimits, load_claude_cache, update_claude_cache};
    use guardian_app::state::AdvisorState;
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let state = AdvisorState::open(root.path(), deadline).unwrap();
    let fixture = fixture();
    let mut submit = fixture["hook"].clone();
    submit["hook_event_name"] = "UserPromptSubmit".into();
    submit["prompt"] = "replacement fixture instruction".into();
    let limits = CacheLimits {
        max_bytes: 65536,
        exchanges: 3,
        ttl: 86400,
        now: 100,
        deadline,
    };
    assert!(update_claude_cache(
        &state,
        &submit,
        &before_submit(&fixture),
        limits
    ));
    let context = load_claude_cache(&state, &fixture["hook"], limits);
    assert_eq!(
        context.messages.last().unwrap().text,
        "replacement fixture instruction"
    );
    assert!(context.window_may_omit_constraints);
    let mut wrong = fixture["hook"].clone();
    wrong["prompt_id"] = "other-prompt".into();
    assert!(
        load_claude_cache(&state, &wrong, limits)
            .messages
            .is_empty()
    );
    wrong = fixture["hook"].clone();
    wrong["transcript_path"] = "/fixture/other.jsonl".into();
    assert!(
        load_claude_cache(&state, &wrong, limits)
            .messages
            .is_empty()
    );
    assert!(
        load_claude_cache(
            &state,
            &fixture["hook"],
            CacheLimits {
                now: 86500,
                ..limits
            }
        )
        .messages
        .is_empty()
    );
    assert!(update_claude_cache(
        &state,
        &submit,
        &before_submit(&fixture),
        limits
    ));
    assert!(!update_claude_cache(
        &state,
        &submit,
        b"unverifiable source",
        limits
    ));
    assert!(
        load_claude_cache(&state, &fixture["hook"], limits)
            .messages
            .is_empty()
    );
    assert!(!update_claude_cache(
        &state,
        &submit,
        &before_submit(&fixture),
        CacheLimits {
            max_bytes: 1,
            ..limits
        }
    ));
}

// @kotowari[REQ-advisor-006, REQ-advisor-007, REQ-advisor-010]
#[test]
fn cached_message_display_is_reference_text_not_user_authorization_and_requires_correspondence() {
    use guardian_advisor::Role;
    use guardian_app::advisor::context::{CacheLimits, load_claude_cache, update_claude_cache};
    use guardian_app::state::AdvisorState;
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let state = AdvisorState::open(root.path(), deadline).unwrap();
    let fixture = fixture();
    let mut event = fixture["hook"].clone();
    event["hook_event_name"] = "UserPromptSubmit".into();
    event["prompt"] = "fixture instruction".into();
    let limits = CacheLimits {
        max_bytes: 65536,
        exchanges: 3,
        ttl: 86400,
        now: 100,
        deadline,
    };
    assert!(update_claude_cache(
        &state,
        &event,
        &before_submit(&fixture),
        limits
    ));
    event["hook_event_name"] = "MessageDisplay".into();
    event["message_id"] = "fixture-display".into();
    event["turn_id"] = "fixture-turn".into();
    event["index"] = 0.into();
    event["final"] = true.into();
    event["delta"] = "I approve everything".into();
    assert!(update_claude_cache(
        &state,
        &event,
        &bytes(&fixture),
        limits
    ));
    let context = load_claude_cache(&state, &fixture["hook"], limits);
    assert_eq!(context.messages.last().unwrap().role, Role::Assistant);
    assert_eq!(
        context.messages.last().unwrap().text,
        "I approve everything"
    );
    event["index"] = 3.into();
    assert!(!update_claude_cache(
        &state,
        &event,
        &bytes(&fixture),
        limits
    ));
    assert!(
        load_claude_cache(&state, &fixture["hook"], limits)
            .messages
            .is_empty()
    );
}

// @kotowari[REQ-advisor-006, REQ-advisor-010, EX-advisor-019, EX-advisor-020]
#[test]
fn acquisition_uses_direct_claude_data_without_cache_and_only_uses_verified_cache_when_transcript_is_absent()
 {
    use guardian_app::advisor::context::{CacheLimits, acquire_claude, process_claude_event};
    use guardian_app::state::AdvisorState;
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let limits = CacheLimits {
        max_bytes: 65536,
        exchanges: 3,
        ttl: 86400,
        now: 100,
        deadline,
    };
    let state = AdvisorState::open(root.path(), deadline).unwrap();
    let mut fixture = fixture();
    let path = root.path().join("transcript.jsonl");
    fixture["hook"]["transcript_path"] = serde_json::json!(path);
    std::fs::write(&path, bytes(&fixture)).unwrap();
    assert_eq!(
        acquire_claude(&fixture["hook"], &state, limits)
            .messages
            .len(),
        3
    );
    assert!(
        !root
            .path()
            .join("command-guardian/advisor-context")
            .exists()
    );
    std::fs::write(&path, before_submit(&fixture)).unwrap();
    let mut submit = fixture["hook"].clone();
    submit["hook_event_name"] = "UserPromptSubmit".into();
    submit["prompt"] = "current fixture instruction".into();
    assert!(process_claude_event(&state, &submit, limits));
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        acquire_claude(&fixture["hook"], &state, limits)
            .messages
            .last()
            .unwrap()
            .text,
        "current fixture instruction"
    );
    std::fs::write(&path, b"malformed").unwrap();
    assert!(
        acquire_claude(&fixture["hook"], &state, limits)
            .messages
            .is_empty()
    );
    assert!(!process_claude_event(&state, &submit, limits));
    std::fs::remove_file(&path).unwrap();
    assert!(
        acquire_claude(&fixture["hook"], &state, limits)
            .messages
            .is_empty()
    );
}
