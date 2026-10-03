//! S12: フラグ・オプションの値・環境変数（REQ-030, REQ-031）。

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

// @kotowari[REQ-030]
#[test]
fn req_030_non_ascii_single_character_flags_match_bundles_and_equals() {
    let rs = rules(
        r#"
[[commands.guard]]
program = "tool"
reason = "flag needs confirmation"
deny-flags = ["-é"]
"#,
    );
    for command in ["tool -é", "tool -éx", "tool -xé=value"] {
        assert!(matches(&rs, command), "{command}");
    }
    assert!(!matches(&rs, "tool -- -éx"));
    assert!(!matches(&rs, "tool -x"));
}

// @kotowari[REQ-030, EX-039]
#[test]
fn req_030_deny_option_values_match_by_name_and_value() {
    let rs = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "別名の定義"
options-with-value = ["-c", "--config-env"]
deny-option-values = { "-c" = ["/alias[.].*/"], "--config-env" = ["/alias[.].*/"] }
"#,
    );
    assert!(matches(&rs, "git -c alias.p=push p"));
    assert!(matches(&rs, "git --config-env=alias.p=ENV p"));
    assert!(matches(&rs, "git -c user.name=x -c alias.p=push p"));
    assert!(!matches(&rs, "git -c user.name=x push"));
    // 最初の `--` より後は見ない。
    assert!(!matches(&rs, "git -- -c alias.p=push p"));
}

// @kotowari[REQ-030]
#[test]
fn req_030_deny_flags_match_bundles_and_equals() {
    let rs = rules(
        r#"
[[commands.guard]]
program = "rm"
reason = "再帰の削除"
deny-flags = ["-r"]
"#,
    );
    assert!(matches(&rs, "rm -rf /tmp/x"));
    assert!(matches(&rs, "rm -r /tmp/x"));
    assert!(matches(&rs, "rm --preserve-root -r /tmp/x"));
    assert!(!matches(&rs, "rm -f /tmp/x"));
    // 最初の `--` より後は見ない。
    assert!(!matches(&rs, "rm -- -r"));

    let rs = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "amend"
deny-flags = ["--amend"]
"#,
    );
    assert!(matches(&rs, "git commit --amend"));
    assert!(matches(&rs, "git commit --amend=true"));
    assert!(!matches(&rs, "git commit --no-amend"));
}

// @kotowari[REQ-030]
#[test]
fn req_030_for_gates_flags_and_option_values() {
    let rs = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "push の強制"
for = [["push"]]
deny-flags = ["--force"]
"#,
    );
    assert!(matches(&rs, "git push --force origin"));
    assert!(!matches(&rs, "git commit --force"));
    assert!(!matches(&rs, "git fetch --force"));

    let rs = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "push の別名"
for = [["push"]]
options-with-value = ["-c"]
deny-option-values = { "-c" = ["/alias[.].*/"] }
"#,
    );
    assert!(matches(&rs, "git -c alias.p=push push origin"));
    assert!(!matches(&rs, "git -c alias.c=commit status"));
}

// @kotowari[REQ-031, EX-045]
#[test]
fn req_031_deny_env_matches_assignments_in_the_command_body() {
    let rs = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "環境変数で設定を差し替える"
deny-env = ["GIT_CONFIG_COUNT"]
"#,
    );
    // EX-045: 最初だけが一致する。
    assert!(matches(&rs, "GIT_CONFIG_COUNT=1 git status"));
    assert!(!matches(&rs, "git status"));
    // ラッパー越しの代入にも当たる。
    assert!(matches(&rs, "sudo GIT_CONFIG_COUNT=1 git status"));
    // 引数の中の代入には当たらない。
    assert!(!matches(&rs, "git status GIT_CONFIG_COUNT=1"));
}

// @kotowari[REQ-031]
#[test]
fn req_031_deny_env_is_gated_by_for_and_ignores_the_hook_environment() {
    let rs = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "push の環境変数"
for = [["push"]]
deny-env = ["GIT_CONFIG_COUNT"]
"#,
    );
    assert!(matches(&rs, "GIT_CONFIG_COUNT=1 git push"));
    assert!(!matches(&rs, "GIT_CONFIG_COUNT=1 git status"));

    // フック自身の環境は見ない。
    let key = "COMMAND_GUARDIAN_PROBE_ENV";
    // SAFETY: この試験バイナリの他の試験も guardian-policy も環境変数を読まないため、
    // 並列実行中の書換えと読取りが競合しない。
    unsafe { std::env::set_var(key, "1") };
    let rs = rules(&format!(
        r#"
[[commands.guard]]
program = "git"
reason = "フックの環境は見ない"
deny-env = ["{key}"]
"#
    ));
    assert!(!matches(&rs, "git status"));
    // SAFETY: set_var と同じ理由で競合しない。
    unsafe { std::env::remove_var(key) };
}
