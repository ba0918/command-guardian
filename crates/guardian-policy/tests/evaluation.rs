use guardian_core::{Ask, Class, Failure, Invocation, ObservedPath, Verdict, Why};
use guardian_policy::{Config, Policy, RuleReport};

// @kotowari[REQ-005, REQ-006, REQ-008, REQ-010]
#[test]
fn req_006_classification_and_roots_are_decided_from_values() {
    let mut config = Config::builtin(None);
    config.unknown_verdict = Verdict::Block;
    config.allowed_roots.push("/work".into());
    config.protected_roots.push("/work/protected".into());
    let policy = Policy::new(config, vec![]);
    assert_eq!(
        policy.classification_verdict(Class::Unknown, &Why::GitFailed),
        Verdict::Ask
    );
    assert_eq!(
        policy.classification_verdict(Class::Unknown, &Why::Untracked),
        Verdict::Block
    );
    assert_eq!(
        policy.classification_verdict(Class::Vcs, &Why::Vcs),
        Verdict::Allow
    );
    assert!(policy
        .apply_roots(&ObservedPath {
            path: "/work".into(),
            children: false
        })
        .is_none());
    assert_eq!(
        policy
            .apply_roots(&ObservedPath {
                path: "/work".into(),
                children: true
            })
            .unwrap()
            .2,
        Verdict::Allow
    );
    assert_eq!(
        policy
            .apply_roots(&ObservedPath {
                path: "/work/protected/x".into(),
                children: false
            })
            .unwrap()
            .2,
        Verdict::Block
    );
}

// @kotowari[REQ-009, REQ-026, REQ-033, REQ-038, REQ-039]
#[test]
fn req_009_allow_rules_never_overwrite_heavier_diagnostics_or_rules() {
    let policy = Policy::new(Config::builtin(None), vec![]);
    let rule = |verdict| RuleReport {
        name: "rule".into(),
        reason: "reason".into(),
        verdict,
    };
    assert_eq!(
        policy
            .report(
                vec![],
                vec![rule(Verdict::Allow)],
                vec![Ask::Parse(Failure::Syntax)]
            )
            .verdict,
        Verdict::Ask
    );
    assert_eq!(
        policy
            .report(
                vec![],
                vec![rule(Verdict::Allow)],
                vec![Ask::Parse(Failure::Limit)]
            )
            .verdict,
        Verdict::Block
    );
    assert_eq!(
        policy
            .report(
                vec![],
                vec![rule(Verdict::Block)],
                vec![Ask::Parse(Failure::Syntax)]
            )
            .verdict,
        Verdict::Block
    );
}

// @kotowari[REQ-027, REQ-029, REQ-031]
#[test]
fn req_031_guards_match_the_supplied_apparent_words_and_assignment_names() {
    let value: toml::Value = toml::from_str("[[commands.guard]]\nprogram='git'\nreason='push'\ndeny=[['push']]\ndeny-env=['OVERRIDE']\n").unwrap();
    let mut config = Config::builtin(None);
    config.guard = guardian_policy::guard::parse_guards(&value, &mut vec![]);
    let policy = Policy::new(config, vec![]);
    assert_eq!(
        policy
            .rules(
                &[Invocation {
                    program: "git".into(),
                    words: vec!["push".into()],
                    env_names: vec![]
                }],
                None
            )
            .len(),
        1
    );
    assert!(policy
        .rules(
            &[Invocation {
                program: "git".into(),
                words: vec!["$CMD".into()],
                env_names: vec![]
            }],
            None
        )
        .is_empty());
    assert_eq!(
        policy
            .rules(
                &[Invocation {
                    program: "git".into(),
                    words: vec!["status".into()],
                    env_names: vec!["OVERRIDE".into()]
                }],
                None
            )
            .len(),
        1
    );
}

// @kotowari[REQ-009, REQ-011, REQ-039]
#[test]
fn req_011_the_message_explains_the_source_that_decides_block() {
    let policy = Policy::new(Config::builtin(None), vec![]);
    let effect = |verdict| guardian_policy::EffectReport {
        op: guardian_core::Op::Delete,
        target: guardian_core::Target::Path {
            path: "/work/file".into(),
            dereference: false,
        },
        class: Class::Unknown,
        why: Why::Untracked,
        verdict,
    };
    let rule = |verdict, reason: &str| RuleReport {
        name: "rule".into(),
        reason: reason.into(),
        verdict,
    };
    for (effects, rules, expected) in [
        (vec![effect(Verdict::Ask)], vec![], "判定の上限"),
        (
            vec![],
            vec![rule(Verdict::Ask, "確認する規則")],
            "判定の上限",
        ),
        (
            vec![effect(Verdict::Ask)],
            vec![rule(Verdict::Ask, "確認する規則")],
            "判定の上限",
        ),
        (vec![effect(Verdict::Block)], vec![], "未追跡"),
        (
            vec![effect(Verdict::Ask)],
            vec![rule(Verdict::Block, "拒否する規則")],
            "拒否する規則",
        ),
        (
            vec![],
            vec![
                rule(Verdict::Ask, "確認する規則"),
                rule(Verdict::Block, "拒否する規則"),
            ],
            "拒否する規則",
        ),
    ] {
        let report = policy.report(
            effects,
            rules,
            vec![Ask::Parse(Failure::Syntax), Ask::Parse(Failure::Limit)],
        );
        assert_eq!(report.verdict, Verdict::Block);
        assert!(report.reason.contains(expected), "{}", report.reason);
        assert!(report.message.contains(expected), "{}", report.message);
        assert!(
            (2..=4).contains(&report.message.lines().count()),
            "{}",
            report.message
        );
    }
}
