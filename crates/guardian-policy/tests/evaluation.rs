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
    assert!(
        policy
            .apply_roots(&ObservedPath {
                path: "/work".into(),
                children: false
            })
            .is_none()
    );
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

// @kotowari[REQ-003, REQ-005, REQ-006]
#[test]
fn req_003_nested_tmpdir_root_is_not_allowed_by_an_outer_temporary_root() {
    for root in ["/tmp/session-temp", "/var/tmp/session-temp"] {
        let policy = Policy::new(Config::builtin(Some(std::path::Path::new(root))), vec![]);
        assert!(
            policy
                .apply_roots(&ObservedPath {
                    path: root.into(),
                    children: false,
                })
                .is_none()
        );
        for observed in [
            ObservedPath {
                path: root.into(),
                children: true,
            },
            ObservedPath {
                path: std::path::Path::new(root).join("child"),
                children: false,
            },
        ] {
            assert_eq!(policy.apply_roots(&observed).unwrap().2, Verdict::Allow);
        }
    }
}

// @kotowari[REQ-005, REQ-006, REQ-013]
#[test]
fn req_005_configured_roots_with_parent_components_match_resolved_targets() {
    for root in ["../valuable", "/work/repo/../valuable"] {
        let layer = guardian_policy::layers::parse_layer(
            &format!("[paths]\nprotected_roots = [\"{root}\"]"),
            std::path::Path::new("/work/repo"),
            None,
        )
        .unwrap();
        let mut config = Config::builtin(None);
        guardian_policy::layers::merge(&mut config, &layer);
        let policy = Policy::new(config, vec![]);
        for path in ["/work/valuable", "/work/valuable/file"] {
            assert_eq!(
                policy
                    .apply_roots(&ObservedPath {
                        path: path.into(),
                        children: false,
                    })
                    .map(|value| value.2),
                Some(Verdict::Block)
            );
        }
        assert!(
            policy
                .apply_roots(&ObservedPath {
                    path: "/work/valuable-other/file".into(),
                    children: false,
                })
                .is_none()
        );
    }
    let mut config = Config::builtin(None);
    config.allowed_roots.push("/work/repo/../scratch".into());
    let policy = Policy::new(config, vec![]);
    assert_eq!(
        policy
            .apply_roots(&ObservedPath {
                path: "/work/scratch/file".into(),
                children: false,
            })
            .map(|value| value.2),
        Some(Verdict::Allow)
    );
    assert!(
        policy
            .apply_roots(&ObservedPath {
                path: "/work/scratch".into(),
                children: false,
            })
            .is_none()
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
    assert!(
        policy
            .rules(
                &[Invocation {
                    program: "git".into(),
                    words: vec!["$CMD".into()],
                    env_names: vec![]
                }],
                None
            )
            .is_empty()
    );
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
        (vec![effect(Verdict::Ask)], vec![], "judgment limit"),
        (
            vec![],
            vec![rule(Verdict::Ask, "確認する規則")],
            "judgment limit",
        ),
        (
            vec![effect(Verdict::Ask)],
            vec![rule(Verdict::Ask, "確認する規則")],
            "judgment limit",
        ),
        (vec![effect(Verdict::Block)], vec![], "untracked"),
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
