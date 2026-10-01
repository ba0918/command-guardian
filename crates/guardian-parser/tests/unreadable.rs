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
