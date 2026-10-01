//! S8: 読めない構文は ask のコーパス（REQ-038）。

use guardian_parser::{parse, Failure};

/// 読めない入力のコーパス。
const UNREADABLE: &[&str] = &[
    "if true; then",
    "rm -rf '/etc/foo",
    "rm -rf \"/etc/foo",
    "rm -rf `rm /etc/foo",
    "rm -rf $(rm /etc/foo",
    "rm -rf ${HOME",
    "rm -rf $(cat",
    "while true; do",
    "until true; do",
    "for i in a b; do",
    "case x in a)",
    "{ echo x",
    "( echo x",
    "f() { echo x",
    "cat <<EOF\nbody",
    "[[ -n x",
    "a=(1 2",
    "echo )",
    "echo x |",
    "&& echo x",
];

/// 読める入力のコーパス（問い合わせ側が区別できることの確認）。
const READABLE: &[&str] = &[
    "rm -rf '/etc/foo'",
    "rm -rf \"/etc/foo\"",
    "cat <<EOF\nbody\nEOF",
    "echo $(echo x)",
    "if true; then echo x; fi",
    "for i in a b; do echo x; done",
    "[[ -n x ]]",
    "a=(1 2)",
];

// @kotowari[REQ-038]
#[test]
fn req_038_unreadable_inputs_are_reported() {
    for input in UNREADABLE {
        let outcome = parse(input);
        assert!(!outcome.failures.is_empty(), "{input}");
    }
}

// @kotowari[REQ-038]
#[test]
fn req_038_readable_inputs_are_not_reported() {
    for input in READABLE {
        let outcome = parse(input);
        assert!(
            outcome.failures.is_empty(),
            "{input}: {:?}",
            outcome.failures
        );
    }
}

// @kotowari[REQ-039]
#[test]
fn req_039_parentheses_in_parameter_operands_do_not_hide_depth() {
    // `${x:-)}` の中の ")" は本物の "(" を閉じない。深さを少なく数えない。
    let mut input = "( ${x:-)}; ".repeat(2000);
    input.push_str(": ");
    input.push_str(&")".repeat(2000));
    let outcome = parse(&input);
    assert_eq!(outcome.failures, vec![Failure::TooDeep]);
    assert!(outcome.script.is_empty());
}

// @kotowari[REQ-039]
#[test]
fn req_039_openers_in_backquotes_do_not_hide_depth() {
    // バッククォートの本体の中の `{` も入れ子として数える。
    let mut input = String::from("echo `");
    for _ in 0..2000 {
        input.push_str("{ ");
    }
    input.push('`');
    let outcome = parse(&input);
    assert_eq!(outcome.failures, vec![Failure::TooDeep]);
    assert!(outcome.script.is_empty());
}

// @kotowari[REQ-039]
#[test]
fn req_039_comment_char_does_not_hide_depth() {
    // `)` の直後の `#` はコメントではない。続く `$(` の入れ子を隠さない。
    let mut input = String::from("echo $(true)#");
    for _ in 0..2700 {
        input.push_str("$(");
    }
    input.push_str(": ");
    for _ in 0..2700 {
        input.push(')');
    }
    let outcome = parse(&input);
    assert_eq!(outcome.failures, vec![Failure::TooDeep]);
    assert!(outcome.script.is_empty());
}

// @kotowari[REQ-039]
#[test]
fn req_039_shallow_spellings_are_still_read() {
    // 曖昧な字面でも、浅い入力は読める（過大に見積もりすぎない）。
    for input in [
        "echo ${x:-)}",
        "echo ${#x}",
        "echo $#",
        "echo $(true)#$(true)",
        "echo `echo hi`",
        "echo a#b",
        "echo {a,b}#c",
        "echo $'a\\'b'",
    ] {
        let outcome = parse(input);
        assert!(
            outcome.failures.is_empty(),
            "{input}: {:?}",
            outcome.failures
        );
    }

    // `$'...'` の中の `\'` は引用を閉じない。中の `$(` は字面であって
    // 入れ子ではない。
    let mut inside = String::from("echo $'a\\')");
    for _ in 0..2000 {
        inside.push_str("$(");
    }
    inside.push('\'');
    let outcome = parse(&inside);
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
}

// @kotowari[REQ-039]
#[test]
fn req_039_other_hiding_spellings_do_not_hide_depth() {
    // パラメータ展開の閉じの直後の `#`、here-doc の本体、case の枝でも、
    // 隠れた入れ子を深さとして数える。
    let mut after_parameter = String::from("echo ${x}#");
    let mut here_doc = String::from("cat <<EOF\n");
    let mut case_chain = String::new();
    for _ in 0..2000 {
        after_parameter.push_str("$(");
        here_doc.push_str("$(");
        case_chain.push_str("case x in a) echo $(");
    }
    after_parameter.push_str(": ");
    after_parameter.push_str(&")".repeat(2000));
    here_doc.push_str(":\nEOF\n");
    case_chain.push(':');
    for _ in 0..2000 {
        case_chain.push_str(");; esac");
    }
    for input in [after_parameter, here_doc, case_chain] {
        let outcome = parse(&input);
        assert_eq!(outcome.failures, vec![Failure::TooDeep], "{input}");
        assert!(outcome.script.is_empty());
    }
}

// @kotowari[REQ-039]
#[test]
fn req_039_keyword_words_in_arguments_do_not_hide_depth() {
    // `done` が引数の位置にあるときは、閉じの予約語ではない。区切りの
    // 直後にだけ閉じとして数え、入れ子を隠さない。
    let mut input = String::new();
    for _ in 0..2000 {
        input.push_str("while :; do echo $[1] done; ");
    }
    input.push(':');
    for _ in 0..2000 {
        input.push_str("; done");
    }
    let outcome = parse(&input);
    assert_eq!(outcome.failures, vec![Failure::TooDeep]);
    assert!(outcome.script.is_empty());
}

// @kotowari[REQ-039]
#[test]
fn req_039_an_ordinary_script_is_not_too_deep() {
    // here-doc と case と繰り返しを含む普通のスクリプトは、深さの上限に
    // 達しない（保守側に倒すのは曖昧な字面だけで足りる）。
    let mut script = String::from(
        "#!/bin/bash\nset -euo pipefail\ncleanup() {\n  rm -rf \"$TMPDIR/build-$$\"\n}\ntrap cleanup EXIT\n",
    );
    for i in 0..60 {
        script.push_str(&format!(
            "echo \"step {i}: $(date +%s)\" | tee -a \"$LOG\"\n"
        ));
    }
    script.push_str("cat <<EOF > config.txt\nname=$(whoami)\npath=$PWD\nEOF\n");
    for i in 0..40 {
        script.push_str(&format!(
            "if [ -f \"file{i}\" ]; then echo \"$(( {i} + 1 ))\"; fi\n"
        ));
    }
    script
        .push_str("case \"$1\" in\n  a) echo one;;\n  b|c) echo two;;\n  *) echo other;;\nesac\n");
    for i in 0..40 {
        script.push_str(&format!("for x in a b c; do echo \"$x{i}\"; done\n"));
    }
    let outcome = parse(&script);
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
}
