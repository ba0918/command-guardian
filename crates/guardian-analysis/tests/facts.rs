use guardian_core::Env;

fn facts(command: &str) -> guardian_core::CommandFacts {
    guardian_analysis::analyze(
        guardian_parser::parse(command),
        &Env {
            home: None,
            tmpdir: None,
            cwd: None,
        },
        &mut guardian_parser::parse,
    )
}

// @kotowari[REQ-001, REQ-002, REQ-031, REQ-037, REQ-039]
#[test]
fn req_037_loop_effects_keep_bindings_and_invocations_keep_syntax_positions() {
    let input = "for x in /etc/a /etc/b; do X=1 eval 'rm $x'; git push; git push; done";
    let mut reads = 0;
    let facts = guardian_analysis::analyze(
        guardian_parser::parse(input),
        &Env {
            home: None,
            tmpdir: None,
            cwd: None,
        },
        &mut |inner| {
            reads += 1;
            guardian_parser::parse(inner)
        },
    );
    assert_eq!(reads, 1);
    assert_eq!(facts.effects.len(), 2);
    assert_eq!(
        facts.effects[0].target,
        guardian_core::Target::Path {
            path: "/etc/a".into(),
            dereference: false
        }
    );
    assert_eq!(
        facts.effects[1].target,
        guardian_core::Target::Path {
            path: "/etc/b".into(),
            dereference: false
        }
    );
    assert_eq!(
        facts
            .invocations
            .iter()
            .filter(|i| i.program == "git")
            .count(),
        2
    );
    let rm = facts
        .invocations
        .iter()
        .find(|i| i.program == "rm")
        .unwrap();
    assert_eq!(rm.words, ["$x"]);
}

// @kotowari[REQ-001, REQ-031, REQ-037]
#[test]
fn req_037_each_word_substitution_is_visited_once() {
    let facts = facts("X=$(rm /etc/a) true; dd of=$(rm /etc/b); echo >$(rm /etc/c)");
    assert_eq!(
        facts
            .invocations
            .iter()
            .filter(|i| i.program == "rm")
            .count(),
        3
    );
    assert_eq!(
        facts
            .effects
            .iter()
            .filter(|e| e.op == guardian_core::Op::Delete)
            .count(),
        3
    );
    let true_ = facts
        .invocations
        .iter()
        .find(|i| i.program == "true")
        .unwrap();
    assert_eq!(true_.env_names, ["X"]);
}

// @kotowari[REQ-039, REQ-002, REQ-031]
#[test]
fn req_039_cpu_traversal_obeys_the_callers_deadline() {
    struct Deadline(std::time::Instant);
    impl guardian_analysis::AnalysisControl for Deadline {
        fn parse(&mut self, input: &str) -> guardian_parser::Outcome {
            guardian_parser::parse(input)
        }
        fn check(&mut self) -> Result<(), guardian_parser::Failure> {
            if std::time::Instant::now() >= self.0 {
                Err(guardian_parser::Failure::Limit)
            } else {
                Ok(())
            }
        }
    }
    let values = (0..100)
        .map(|i| format!("/work/{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let input = format!(
        "{}rm $x{}",
        format!("for x in {values}; do ").repeat(3),
        "; done".repeat(3)
    );
    let outcome = guardian_parser::parse(&input);
    assert!(outcome.failures.is_empty());
    let mut control = Deadline(std::time::Instant::now() + std::time::Duration::from_millis(20));
    let facts = guardian_analysis::analyze_with_control(
        outcome,
        &Env {
            home: None,
            tmpdir: None,
            cwd: None,
        },
        &mut control,
    );
    assert_eq!(
        facts.diagnostics,
        [guardian_core::Ask::Parse(guardian_parser::Failure::Limit)]
    );
    assert!(facts.effects.len() < 100 * 100 * 100);
}

// @kotowari[REQ-001, REQ-002, REQ-031, REQ-037]
#[test]
fn req_002_declarations_propagate_assignments_before_later_substitutions() {
    for builtin in ["export", "local", "declare", "readonly", "typeset"] {
        for wrapper in ["", "sudo "] {
            let input = format!("{wrapper}{builtin} S=/tmp/scratch T=$(rm $S) $(rm $S); rm $S");
            let facts = facts(&input);
            assert!(
                facts.diagnostics.is_empty(),
                "{input}: {:?}",
                facts.diagnostics
            );
            assert_eq!(facts.effects.len(), 3, "{input}");
            for effect in &facts.effects {
                assert_eq!(
                    effect.target,
                    guardian_core::Target::Path {
                        path: "/tmp/scratch".into(),
                        dereference: false
                    },
                    "{input}"
                );
            }
            assert_eq!(
                facts
                    .invocations
                    .iter()
                    .filter(|i| i.program == "rm")
                    .count(),
                3,
                "{input}"
            );
        }
    }
}

// @kotowari[REQ-034, REQ-031, REQ-037, REQ-038]
#[test]
fn req_034_example_analysis_collects_syntax_without_materializing_effects() {
    let input = "for x in /a /b; do for y in /c /d; do X=1 eval 'rm $x'; git push; git push; echo >file; find /a -delete | xargs rm; done; done";
    let facts = guardian_analysis::analyze_invocations(
        guardian_parser::parse(input),
        &mut guardian_parser::parse,
    );
    assert!(facts.effects.is_empty());
    assert!(facts.diagnostics.is_empty());
    assert_eq!(facts.invocations.len(), 6);
    assert_eq!(facts.invocations[0].program, "rm");
    assert_eq!(facts.invocations[0].words, ["$x"]);
    assert_eq!(facts.invocations[1], facts.invocations[2]);
    assert_eq!(facts.invocations[1].program, "git");
    assert_eq!(facts.invocations[1].words, ["push"]);
}

// @kotowari[REQ-034, REQ-031, REQ-035, REQ-037, REQ-038]
#[test]
fn req_034_example_analysis_preserves_apparent_invocations_and_diagnostics() {
    for input in [
        "OVERRIDE=1 sudo git '$ACTION'",
        "X=$(git push)",
        "export S=/tmp/scratch T=$(rm $S)",
        "for x in $(git status) /a; do bash -c 'OVERRIDE=1 git \"$x\"'; done",
        "(cd /work; rm file) >$(git status); cat <(git push)",
        "eval 'git push; if'",
        "fish -c x; bash -c \"$BODY\"; eval $COMMAND",
    ] {
        let full = facts(input);
        let invocations = guardian_analysis::analyze_invocations(
            guardian_parser::parse(input),
            &mut guardian_parser::parse,
        );
        assert_eq!(invocations.invocations, full.invocations, "{input}");
        assert_eq!(invocations.diagnostics, full.diagnostics, "{input}");
        assert!(invocations.effects.is_empty(), "{input}");
    }
}
