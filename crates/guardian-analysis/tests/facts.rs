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
