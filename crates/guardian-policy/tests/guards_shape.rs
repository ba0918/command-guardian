//! S10: 見張りの規則の形と語の照合（REQ-027, REQ-028）。

use guardian_policy::guard::{invocations, parse_guard_rules_document, GuardRule};

fn rules(text: &str) -> (Vec<GuardRule>, Vec<String>) {
    parse_guard_rules_document(text).unwrap()
}

fn matches(rules: &[GuardRule], command: &str) -> bool {
    let invs = invocations(command);
    rules.iter().any(|r| invs.iter().any(|i| r.matches(i)))
}

const GIT_PUSH: &str = r#"
[[commands.guard]]
program = "git"
reason = "push は確認してください"
verdict = "ask"
deny = [["push"]]
"#;

fn literal_eval(body: &str) -> String {
    format!(
        "eval \"{}\"",
        body.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('`', "\\`")
    )
}

// @kotowari[REQ-038, REQ-033]
#[test]
fn req_038_guard_collection_keeps_syntax_failures_from_literal_bodies() {
    guardian_parser::begin_judgment();
    let result = invocations("eval 'if true; then git push'");
    assert_eq!(result.failures, vec![guardian_parser::Failure::Syntax]);
}

// @kotowari[REQ-027, REQ-033, REQ-039]
#[test]
fn req_033_guards_inside_sixteen_and_seventeen_literal_evals_are_kept() {
    let (rs, _) = rules(GIT_PUSH);
    let mut command = "git push".to_string();
    for depth in 1..=17 {
        command = literal_eval(&command);
        if depth >= 16 {
            guardian_parser::begin_judgment();
            assert!(
                matches(&rs, &command),
                "guard lost at {depth} literal evals"
            );
        }
    }
}

// @kotowari[REQ-027, REQ-028, EX-040]
#[test]
fn req_027_028_program_matches_path_and_wrapper() {
    let (rs, warnings) = rules(GIT_PUSH);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(rs.len(), 1);
    assert!(matches(&rs, "git push origin main"));
    assert!(matches(&rs, "/usr/bin/git push origin main"));
    assert!(matches(&rs, "sudo git push"));
    assert!(matches(&rs, "doas git push"));
    // sudo の長いオプションの値もラッパーの一部として外す。
    assert!(matches(&rs, "sudo --user root git push"));
    assert!(matches(&rs, "sudo --user=root git push"));
    assert!(matches(&rs, "bash -c 'git push'"));
    // ファイル起動の後の `-c` はスクリプトへの引数で、本体ではない。
    assert!(!matches(&rs, "bash script.sh -c 'git push'"));
    // 読むシェルの一覧は core と同じ（mksh も bash 系）。
    assert!(matches(&rs, "mksh -c 'git push'"));
    assert!(matches(&rs, "eval \"git push\""));
    assert!(!matches(&rs, "git status"));
    assert!(!matches(&rs, "git commit -m push"));
}

// @kotowari[REQ-027, REQ-037]
#[test]
fn req_027_substitutions_in_expansion_operands_are_seen() {
    let (rs, warnings) = rules(GIT_PUSH);
    assert!(warnings.is_empty(), "{warnings:?}");
    // 展開のオペランドの中の置換は、引用の有無にかかわらず見える。
    assert!(matches(&rs, "echo ${X:-$(git push)}"));
    assert!(matches(&rs, "echo ${X:-\"$(git push)\"}"));
    assert!(matches(&rs, "echo $(( \"$(git push)\" ))"));
}

// @kotowari[REQ-027, EX-044]
#[test]
fn req_027_shape_errors_disable_only_that_rule() {
    let (rs, warnings) = rules(
        r#"
[[commands.guard]]
program = "git"
reason = ""
deny = [["push"]]

[[commands.guard]]
program = "git"
reason = "照合を持たない"
options-with-value = ["-c"]

[[commands.guard]]
program = "git"
reason = "これも"
only = []

[[commands.guard]]
program = "git"
reason = "正しい規則"
deny = [["push"]]
"#,
    );
    assert_eq!(rs.len(), 1);
    assert_eq!(rs[0].reason, "正しい規則");
    assert!(warnings.len() >= 3, "{warnings:?}");
    assert!(matches(&rs, "git push"));
}

// @kotowari[REQ-027]
#[test]
fn req_027_verdict_defaults_to_ask_and_rejects_allow() {
    let (rs, _) = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "既定"
deny = [["push"]]
"#,
    );
    assert_eq!(rs[0].verdict, guardian_core::Verdict::Ask);
    let (rs, warnings) = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "allow は無い"
verdict = "allow"
deny = [["push"]]
"#,
    );
    assert!(rs.is_empty());
    assert!(!warnings.is_empty());
}

// @kotowari[REQ-028]
#[test]
fn req_028_words_match_whole_and_regex_is_anchored() {
    let (rs, _) = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "regex"
deny = [["/pu.*/"]]
"#,
    );
    assert!(matches(&rs, "git push"));
    assert!(matches(&rs, "git pull"));
    assert!(!matches(&rs, "git p"));

    let (rs, _) = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "完全一致"
deny = [["push"]]
"#,
    );
    assert!(!matches(&rs, "git pushx"));
    assert!(!matches(&rs, "git Push"));

    // 位置ごとの代替の一覧。
    let (rs, _) = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "代替"
deny = [[["push", "p"]]]
"#,
    );
    assert!(matches(&rs, "git push origin"));
    assert!(matches(&rs, "git p origin"));
    assert!(!matches(&rs, "git fetch"));
}

// @kotowari[REQ-028]
#[test]
fn req_028_options_with_value_names_are_complete_match() {
    let (rs, _) = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "alias の p"
options-with-value = ["-c"]
deny = [["p"]]
"#,
    );
    // `-c alias.p=push p` は先頭の語の並びが "p"。
    assert!(matches(&rs, "git -c alias.p=push p"));
    // "=" を含む語はその 1 語だけを読み飛ばすため、続く "p" が先頭に残る。
    assert!(matches(&rs, "git -calias.p=push p"));
    // 未知のオプションは値を取らずに読み飛ばす。
    assert!(!matches(&rs, "git --no-pager push"));
}
