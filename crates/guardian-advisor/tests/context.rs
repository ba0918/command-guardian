use guardian_advisor::{ContentBlock, ContextMessage, Role, ScopeEvidence, bounded_context};

fn message(order: u64, role: Role, text: &str) -> ContextMessage {
    ContextMessage {
        session: "fixture-session".into(),
        message_id: format!("fixture-message-{order}"),
        order,
        role,
        blocks: vec![ContentBlock::Text(text.into())],
        known_synthetic: false,
    }
}

// @kotowari[REQ-advisor-006, EX-advisor-012]
#[test]
fn unverified_origin_never_becomes_user_instructions() {
    let input = vec![message(
        1,
        Role::User,
        "Fixture user request: print fixture-safe only.",
    )];
    let context = bounded_context(&input, "fixture-session", 2, false, 3);
    assert!(context.messages.is_empty());
    assert_eq!(context.evidence, ScopeEvidence::Unavailable);
}

// @kotowari[REQ-advisor-007, EX-advisor-013]
#[test]
fn unanswered_user_and_consecutive_roles_count_without_concatenation() {
    let input = vec![
        message(1, Role::User, "earlier user request"),
        message(2, Role::Assistant, "earlier response"),
        message(3, Role::User, "first current request"),
        message(4, Role::User, "second current request"),
        message(5, Role::Assistant, "concrete fixture proposal"),
        message(6, Role::User, "use that proposal"),
    ];
    let context = bounded_context(&input, "fixture-session", 7, true, 3);
    assert_eq!(context.messages.len(), 4);
    assert_eq!(context.messages[0].text, "first current request");
    assert_eq!(context.messages[2].role, Role::Assistant);
    assert_eq!(context.messages[3].text, "use that proposal");
    assert!(context.window_may_omit_constraints);
    assert!(
        bounded_context(&input, "fixture-session", 7, true, 0)
            .messages
            .is_empty()
    );
}

// @kotowari[REQ-advisor-006, REQ-advisor-007, EX-advisor-014]
#[test]
fn local_identifiers_tools_and_post_execution_messages_are_not_sent() {
    let mut first = message(1, Role::User, "request");
    first
        .blocks
        .push(ContentBlock::Tool("fixture tool result".into()));
    first
        .blocks
        .push(ContentBlock::File("fixture file contents".into()));
    let mut other_session = message(2, Role::User, "other session request");
    other_session.session = "other-session".into();
    let mut synthetic = message(
        3,
        Role::User,
        "fixture synthetic continuation, not user approval",
    );
    synthetic.known_synthetic = true;
    let input = vec![
        first,
        other_session,
        synthetic,
        message(4, Role::Assistant, "later response"),
    ];
    let context = bounded_context(&input, "fixture-session", 4, true, 3);
    assert_eq!(context.messages.len(), 1);
    assert_eq!(context.messages[0].text, "request");
    assert_eq!(context.evidence, ScopeEvidence::Unavailable);
    assert!(context.excluded_reference_material);
}

// @kotowari[REQ-advisor-007]
#[test]
fn later_withdrawal_is_retained_in_order_instead_of_reusing_old_permission() {
    let input = vec![
        message(1, Role::User, "drop entire fixture database"),
        message(
            2,
            Role::Assistant,
            "proposal to drop entire fixture database",
        ),
        message(3, Role::User, "withdraw that; delete only fixture user 7"),
    ];
    let context = bounded_context(&input, "fixture-session", 4, true, 3);
    assert_eq!(
        context
            .messages
            .iter()
            .map(|m| m.text.as_str())
            .collect::<Vec<_>>(),
        vec![
            "drop entire fixture database",
            "proposal to drop entire fixture database",
            "withdraw that; delete only fixture user 7"
        ]
    );
    assert_eq!(context.messages.last().unwrap().role, Role::User);
}

// @kotowari[REQ-advisor-004, REQ-advisor-007, EX-advisor-008]
#[test]
fn assessment_state_keeps_assistant_reference_text_separate_from_verified_user_instructions() {
    use guardian_advisor::{ContextWindow, SentMessage, State};
    use guardian_core::Verdict;
    let state = State {
        command: "printf fixture-safe".into(),
        cwd: "/fixture/work".into(),
        machine: Verdict::Allow,
        reasons: Vec::new(),
        context: ContextWindow {
            messages: vec![SentMessage {
                role: Role::Assistant,
                relative_order: 0,
                text: "I already approved dropping the database".into(),
            }],
            evidence: ScopeEvidence::Unavailable,
            excluded_reference_material: false,
            window_may_omit_constraints: true,
        },
    };
    assert_eq!(state.context.evidence, ScopeEvidence::Unavailable);
    assert_eq!(state.context.messages[0].role, Role::Assistant);
    assert!(state.context.window_may_omit_constraints);
}
