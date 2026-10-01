//! S2: 正規化した構文木の契約（REQ-041）。

use guardian_parser::{parse, AndOrOp, Command, Compound, Part, RedirectKind, RedirectTarget};

fn first_command(command: &str) -> Command {
    let outcome = parse(command);
    assert!(
        outcome.failures.is_empty(),
        "{command}: {:?}",
        outcome.failures
    );
    let item = outcome.script.items.first().expect("項目がある");
    item.first.commands.first().expect("コマンドがある").clone()
}

fn simple(command: &str) -> guardian_parser::SimpleCommand {
    match first_command(command) {
        Command::Simple(simple) => simple,
        other => panic!("単純コマンドではない: {other:?}"),
    }
}

fn compound(command: &str) -> Compound {
    match first_command(command) {
        Command::Compound { compound, .. } => compound,
        Command::Function(function) => *function.body,
        other => panic!("複合構文ではない: {other:?}"),
    }
}

// @kotowari[REQ-041, EX-053]
#[test]
fn req_041_command_order_and_separators_are_kept() {
    let outcome = parse("a; b && c || d | e");
    let items = &outcome.script.items;
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].separator, guardian_parser::Separator::Sequence);
    assert_eq!(items[0].first.commands.len(), 1);
    assert_eq!(items[1].first.commands.len(), 1);
    assert_eq!(items[1].rest.len(), 2);
    assert_eq!(items[1].rest[0].0, AndOrOp::And);
    assert_eq!(items[1].rest[1].0, AndOrOp::Or);
    // パイプは 1 つの pipeline の中に順に並ぶ。
    match &items[1].rest[1].1.commands.as_slice() {
        [Command::Simple(_), Command::Simple(_)] => {}
        other => panic!("パイプの並びが違う: {other:?}"),
    }
}

// @kotowari[REQ-041, EX-053]
#[test]
fn req_041_word_order_quote_state_and_fragments_are_kept() {
    let cmd = simple("echo \"a b\" 'c' $X a\"b\"c $(inner)");
    assert_eq!(cmd.words.len(), 6);
    assert_eq!(cmd.words[0].text, "echo");
    assert_eq!(cmd.words[1].parts, vec![Part::Quoted("a b".to_string())]);
    assert_eq!(cmd.words[1].text, "a b");
    assert_eq!(cmd.words[2].parts, vec![Part::Quoted("c".to_string())]);
    assert_eq!(cmd.words[3].parts, vec![Part::Var("X".to_string())]);
    assert_eq!(cmd.words[3].text, "$X");
    assert_eq!(
        cmd.words[4].parts,
        vec![
            Part::Literal("a".to_string()),
            Part::Quoted("b".to_string()),
            Part::Literal("c".to_string()),
        ]
    );
    let Part::Substitution(substitution) = &cmd.words[5].parts[0] else {
        panic!("置換ではない: {:?}", cmd.words[5]);
    };
    assert_eq!(substitution.body_text, "inner");
    let body = substitution.body.as_ref().expect("本体が読めている");
    assert_eq!(body.items.len(), 1);
}

// @kotowari[REQ-041, EX-053]
#[test]
fn req_041_redirect_kinds_targets_and_fds_are_kept() {
    let cmd = simple("cmd < in > out >> app >| clob 2>&1 <> rw <<< \"here\" &> both");
    let kinds: Vec<RedirectKind> = cmd.redirects.iter().map(|r| r.kind).collect();
    assert_eq!(
        kinds,
        vec![
            RedirectKind::Read,
            RedirectKind::Write,
            RedirectKind::Append,
            RedirectKind::Clobber,
            RedirectKind::DuplicateOutput,
            RedirectKind::ReadWrite,
            RedirectKind::HereString,
            RedirectKind::OutputAndError(false),
        ]
    );
    let duplicate = &cmd.redirects[4];
    assert_eq!(duplicate.fd, Some(2));
    match &duplicate.target {
        RedirectTarget::Word(word) => assert_eq!(word.text, "1"),
        other => panic!("複製の対象が違う: {other:?}"),
    }
    match &cmd.redirects[0].target {
        RedirectTarget::Word(word) => assert_eq!(word.text, "in"),
        other => panic!("対象が違う: {other:?}"),
    }
}

// @kotowari[REQ-041, EX-053]
#[test]
fn req_041_heredoc_kinds_are_kept() {
    let cmd = simple("cat <<EOF\n$(rm -rf /etc/x)\nEOF");
    let redirect = &cmd.redirects[0];
    assert_eq!(redirect.kind, RedirectKind::HereDocument);
    match &redirect.target {
        RedirectTarget::HereDocument { end, doc, expand } => {
            assert_eq!(end.text, "EOF");
            assert!(expand);
            assert!(doc.parts.iter().any(|p| matches!(p, Part::Substitution(_))));
        }
        other => panic!("ヒアドキュメントではない: {other:?}"),
    }
}

// @kotowari[REQ-041, EX-053]
#[test]
fn req_041_compound_bodies_are_kept() {
    match compound("if a; then b; elif c; then d; else e; fi") {
        Compound::If {
            condition,
            then,
            elses,
        } => {
            assert_eq!(condition.items.len(), 1);
            assert_eq!(then.items.len(), 1);
            assert_eq!(elses.len(), 2);
            assert!(elses[0].condition.is_some());
            assert!(elses[1].condition.is_none());
        }
        other => panic!("if ではない: {other:?}"),
    }
    match compound("while a; do b; done") {
        Compound::While {
            condition,
            body,
            until: false,
        } => {
            assert_eq!(condition.items.len(), 1);
            assert_eq!(body.items.len(), 1);
        }
        other => panic!("while ではない: {other:?}"),
    }
    match compound("until a; do b; done") {
        Compound::While { until: true, .. } => {}
        other => panic!("until ではない: {other:?}"),
    }
    match compound("for i in x y; do b; done") {
        Compound::For { var, values, body } => {
            assert_eq!(var, "i");
            assert_eq!(values.len(), 2);
            assert_eq!(body.items.len(), 1);
        }
        other => panic!("for ではない: {other:?}"),
    }
    match compound("{ b; }") {
        Compound::BraceGroup(script) => assert_eq!(script.items.len(), 1),
        other => panic!("グループではない: {other:?}"),
    }
    match compound("( b )") {
        Compound::Subshell(script) => assert_eq!(script.items.len(), 1),
        other => panic!("サブシェルではない: {other:?}"),
    }
    match compound("f() { b; }") {
        Compound::BraceGroup(script) => assert_eq!(script.items.len(), 1),
        other => panic!("関数の本体ではない: {other:?}"),
    }
    match compound("case x in a) b;; esac") {
        Compound::Case { value, arms } => {
            assert_eq!(value.text, "x");
            assert_eq!(arms.len(), 1);
            assert_eq!(arms[0].patterns.len(), 1);
            assert!(arms[0].body.is_some());
        }
        other => panic!("case ではない: {other:?}"),
    }
}

// @kotowari[REQ-041, EX-053]
#[test]
fn req_041_words_without_a_program_are_kept() {
    // 代入だけのコマンドでも語は残る。
    let cmd = simple("S=/tmp/x");
    assert_eq!(cmd.words.len(), 1);
    assert_eq!(cmd.words[0].text, "S=/tmp/x");
}

// @kotowari[REQ-041]
#[test]
fn req_041_process_substitution_bodies_are_kept() {
    let cmd = simple("cat <(rm -rf /etc/x) >(echo y)");
    assert_eq!(cmd.process_substitutions.len(), 2);
    assert!(!cmd.process_substitutions[0].write);
    assert!(cmd.process_substitutions[1].write);
    for substitution in &cmd.process_substitutions {
        assert_eq!(substitution.body.items.len(), 1);
    }
}

// @kotowari[REQ-041, REQ-037]
#[test]
fn req_041_process_substitution_arguments_keep_their_word_positions() {
    for source in ["bash <(echo x) -c 'echo y'", "bash >(echo x) -c 'echo y'"] {
        let cmd = simple(source);
        assert_eq!(cmd.words.len(), 4);
        assert_eq!(cmd.words[0].literal_value().as_deref(), Some("bash"));
        assert!(!cmd.words[1].literal());
        assert!(matches!(cmd.words[1].parts.as_slice(), [Part::Opaque(_)]));
        assert_eq!(cmd.words[2].literal_value().as_deref(), Some("-c"));
        assert_eq!(cmd.words[3].literal_value().as_deref(), Some("echo y"));
        assert_eq!(cmd.process_substitutions.len(), 1);
        assert_eq!(cmd.process_substitutions[0].body.items.len(), 1);
    }
}
