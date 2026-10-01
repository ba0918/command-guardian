//! S11: 使い方の先頭一致（REQ-029）。

mod common;
use common::{invocations, parse_guard_rules_document};
use guardian_policy::guard::GuardRule;

fn rules(text: &str) -> Vec<GuardRule> {
    parse_guard_rules_document(text).unwrap().0
}

fn matches(rules: &[GuardRule], command: &str) -> bool {
    let invs = invocations(command);
    rules.iter().any(|r| invs.iter().any(|i| r.matches(i)))
}

// @kotowari[REQ-029, EX-038]
#[test]
fn req_029_prefix_match_does_not_hit_arguments() {
    let rs = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "push"
deny = [["push"]]
"#,
    );
    // EX-038: 先頭一致なので引数の中の語には当たらない。
    assert!(!matches(&rs, "git commit -m push"));
    assert!(matches(&rs, "git push"));
    assert!(matches(&rs, "git push origin main"));
}

// @kotowari[REQ-029]
#[test]
fn req_029_options_are_skipped_before_matching() {
    let rs = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "push"
deny = [["push"]]
"#,
    );
    // 値を取らないオプションは読み飛ばす。
    assert!(matches(&rs, "git -q push"));
    assert!(matches(&rs, "git --no-pager push"));
    // "=" を含む語はその 1 語だけ読み飛ばす。
    assert!(matches(&rs, "git --git-dir=/repo push"));
    // `--` を読み飛ばして読み飛ばしを終える。
    assert!(matches(&rs, "git -- push"));
    assert!(!matches(&rs, "git -- -x push"));
}

// @kotowari[REQ-029]
#[test]
fn req_029_options_with_value_skip_the_next_word() {
    let rs = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "push"
options-with-value = ["-C", "--git-dir"]
deny = [["push"]]
"#,
    );
    assert!(matches(&rs, "git -C /some/repo push"));
    assert!(matches(&rs, "git --git-dir /some/repo push"));

    // 読み飛ばしの指定が無ければ、値が先頭の語になって当たらない。
    let rs = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "push"
deny = [["push"]]
"#,
    );
    assert!(!matches(&rs, "git -C /some/repo push"));
}

// @kotowari[REQ-029]
#[test]
fn req_029_sequence_is_matched_position_by_position() {
    let rs = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "commit -m"
deny = [["commit", "-m"]]
"#,
    );
    assert!(matches(&rs, "git commit -m fix"));
    assert!(!matches(&rs, "git commit fix"));
    // 引数の中の語の並びには当たらない。
    assert!(!matches(&rs, "git -m commit"));
}
