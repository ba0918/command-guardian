use guardian_advisor::{Role, ScopeEvidence};
use guardian_app::advisor::context::{acquire_codex, codex_context};
use serde_json::json;

fn transcript() -> Vec<u8> {
    let records = [
        json!({"type":"session_meta","ordinal":0,"payload":{"session_id":"fixture-session","cli_version":"0.160.0","source":"exec","thread_source":"user"}}),
        json!({"type":"response_item","ordinal":1,"payload":{"type":"message","id":"fixture-user","role":"user","content":[{"type":"input_text","text":"Fixture user request: print fixture-safe only."}],"internal_chat_message_metadata_passthrough":{"turn_id":"fixture-turn","content_item_kinds":["user.text"]}}}),
        json!({"type":"response_item","ordinal":2,"payload":{"type":"message","id":"fixture-synthetic","role":"user","content":[{"type":"input_text","text":"fixture synthetic continuation, not user approval"}],"internal_chat_message_metadata_passthrough":{"turn_id":"fixture-turn","content_item_kinds":["unknown"]}}}),
        json!({"type":"response_item","ordinal":3,"payload":{"type":"function_call","call_id":"fixture-call","internal_chat_message_metadata_passthrough":{"turn_id":"fixture-turn"}}}),
        json!({"type":"response_item","ordinal":4,"payload":{"type":"message","id":"later-user","role":"user","content":[{"type":"input_text","text":"later user input"}],"internal_chat_message_metadata_passthrough":{"turn_id":"fixture-turn","content_item_kinds":["user.text"]}}}),
    ];
    records
        .iter()
        .map(|r| format!("{r}\n"))
        .collect::<String>()
        .into_bytes()
}

fn hook() -> serde_json::Value {
    json!({"hook_event_name":"PreToolUse","session_id":"fixture-session","turn_id":"fixture-turn","tool_use_id":"fixture-call"})
}

// @kotowari[REQ-advisor-006, EX-advisor-011, EX-advisor-012]
#[test]
fn verified_codex_execution_boundary_excludes_synthetic_and_later_user_records() {
    let context = codex_context(&hook(), &transcript(), 65536, 3);
    assert_eq!(context.messages.len(), 1);
    assert_eq!(context.messages[0].role, Role::User);
    assert_eq!(
        context.messages[0].text,
        "Fixture user request: print fixture-safe only."
    );
    assert_eq!(context.evidence, ScopeEvidence::Confirmed);
}

// @kotowari[REQ-advisor-006, EX-advisor-012]
#[test]
fn unknown_version_session_turn_or_call_never_establishes_correspondence() {
    for field in ["session_id", "turn_id", "tool_use_id"] {
        let mut value = hook();
        value[field] = json!("unmatched");
        assert!(codex_context(&value, &transcript(), 65536, 3)
            .messages
            .is_empty());
    }
    let changed = String::from_utf8(transcript())
        .unwrap()
        .replace("0.160.0", "0.159.0");
    assert!(codex_context(&hook(), changed.as_bytes(), 65536, 3)
        .messages
        .is_empty());
    assert!(codex_context(&hook(), &transcript(), 2, 3)
        .messages
        .is_empty());
    assert!(codex_context(&hook(), b"malformed", 65536, 3)
        .messages
        .is_empty());
}

// @kotowari[REQ-advisor-006, EX-advisor-011, EX-advisor-012]
#[test]
fn released_host_snapshot_does_not_promote_environment_or_stop_continuation() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/advisor/codex-0.160.0.json"
    ))
    .unwrap();
    let records = fixture["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| format!("{record}\n"))
        .collect::<String>();
    let context = codex_context(&fixture["hook"], records.as_bytes(), 65536, 3);
    assert_eq!(context.messages.len(), 2);
    assert_eq!(
        context.messages[0].text,
        "Fixture user request: print fixture-safe only."
    );
    assert_eq!(context.messages[1].role, Role::Assistant);
    assert_eq!(context.messages[1].text, "fixture assistant response");
    assert_eq!(context.evidence, ScopeEvidence::Confirmed);
}

// @kotowari[REQ-advisor-006, EX-advisor-012]
#[test]
fn ambiguous_duplicate_session_fields_are_not_silently_replaced() {
    let bytes = String::from_utf8(transcript()).unwrap().replacen(
        "\"session_id\":\"fixture-session\"",
        "\"session_id\":\"other-session\",\"session_id\":\"fixture-session\"",
        1,
    );
    assert!(codex_context(&hook(), bytes.as_bytes(), 65536, 3)
        .messages
        .is_empty());
}

// @kotowari[REQ-advisor-006, EX-advisor-012]
#[test]
fn duplicate_source_order_or_execution_boundary_cannot_verify_provenance() {
    let original = String::from_utf8(transcript()).unwrap();
    let invalid_order = original.replace("\"ordinal\":2", "\"ordinal\":1");
    assert!(codex_context(&hook(), invalid_order.as_bytes(), 65536, 3)
        .messages
        .is_empty());
    let boundary = original
        .lines()
        .find(|line| line.contains("\"ordinal\":3"))
        .unwrap();
    let duplicated = format!(
        "{original}{}\n",
        boundary.replace("\"ordinal\":3", "\"ordinal\":5")
    );
    assert!(codex_context(&hook(), duplicated.as_bytes(), 65536, 3)
        .messages
        .is_empty());
}

// @kotowari[REQ-advisor-006, EX-advisor-011, EX-advisor-012]
#[test]
fn direct_acquisition_is_bounded_and_does_not_create_a_session_cache() {
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("fixture-rollout.jsonl");
    std::fs::write(&path, transcript()).unwrap();
    let mut value = hook();
    value["transcript_path"] = json!(path);
    let deadline = Instant::now() + Duration::from_secs(2);
    let context = acquire_codex(&value, 65536, 3, deadline);
    assert_eq!(context.messages.len(), 1);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    assert!(acquire_codex(&value, 2, 3, deadline).messages.is_empty());
    assert!(acquire_codex(&value, 65536, 0, deadline)
        .messages
        .is_empty());
    assert!(
        acquire_codex(&value, 65536, 3, Instant::now() - Duration::from_millis(1))
            .messages
            .is_empty()
    );
    std::fs::remove_file(path).unwrap();
    assert!(acquire_codex(&value, 65536, 3, deadline)
        .messages
        .is_empty());
}
