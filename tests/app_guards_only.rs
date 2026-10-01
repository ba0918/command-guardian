//! S13: only・合成・例の検証（REQ-032, REQ-033, REQ-034）。

use guardian_core::Verdict;
mod app_support;
use app_support::{invocations, parse_guard_rules_document, Engine, EngineEnv};
use guardian_policy::guard::GuardRule;
use std::path::{Path, PathBuf};

fn rules(text: &str) -> (Vec<GuardRule>, Vec<String>) {
    parse_guard_rules_document(text).unwrap()
}

fn matches(rules: &[GuardRule], command: &str) -> bool {
    let invs = invocations(command);
    rules.iter().any(|r| invs.iter().any(|i| r.matches(i)))
}

fn env(cwd: &Path) -> EngineEnv {
    EngineEnv {
        home: Some(PathBuf::from("/home/you")),
        tmpdir: Some(PathBuf::from("/tmp")),
        cwd: cwd.to_path_buf(),
    }
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn fixture_dir(prefix: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap()
}

// @kotowari[REQ-032, EX-042]
#[test]
fn req_032_only_allows_written_usages_and_matches_the_rest() {
    let (rs, warnings) = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "status と commit 以外は確認"
verdict = "ask"
only = [["status"], ["commit"]]
"#,
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(!matches(&rs, "git status"));
    assert!(!matches(&rs, "git commit -m x"));
    assert!(matches(&rs, "git push"));
}

// @kotowari[REQ-032]
#[test]
fn req_032_only_rules_with_the_same_list_only_skip_when_all_match() {
    let (rs, _) = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "A"
verdict = "ask"
only = [["status"]]

[[commands.guard]]
program = "git"
reason = "B"
verdict = "ask"
only = [["status"]]
"#,
    );
    // すべての only に当たる起動だけが一致しない。
    assert!(!matches(&rs, "git status"));
    assert!(matches(&rs, "git push"));
}

// @kotowari[REQ-032]
#[test]
fn req_032_only_is_not_applied_when_a_deny_check_matched() {
    let (rs, _) = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "push は only にあっても止める"
verdict = "block"
deny = [["push"]]
only = [["status"], ["push"]]
"#,
    );
    // deny に当たった起動には only を当てない。
    assert!(matches(&rs, "git push"));
    assert!(!matches(&rs, "git status"));
}

// @kotowari[REQ-034, EX-041]
#[test]
fn req_034_examples_deny_mismatch_disables_the_rule() {
    let (rs, warnings) = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "push"
deny = [["push"]]
examples = { deny = ["git status"], allow = ["git push"] }
"#,
    );
    assert!(rs.is_empty());
    assert!(
        warnings.iter().any(|w| w.contains("examples.deny")),
        "{warnings:?}"
    );
}

// @kotowari[REQ-034]
#[test]
fn req_034_examples_allow_mismatch_disables_the_rule() {
    let (rs, warnings) = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "push"
deny = [["push"]]
examples = { deny = ["git push"], allow = ["git push"] }
"#,
    );
    assert!(rs.is_empty());
    assert!(
        warnings.iter().any(|w| w.contains("examples.allow")),
        "{warnings:?}"
    );
}

// @kotowari[REQ-034, EX-043]
#[test]
fn req_034_broken_regex_disables_the_rule() {
    let (rs, warnings) = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "壊れた正規表現"
deny = [["/pu(sh/"]]
"#,
    );
    assert!(rs.is_empty());
    assert!(
        warnings.iter().any(|w| w.contains("regular expression")),
        "{warnings:?}"
    );
}

// @kotowari[REQ-034]
#[test]
fn req_034_examples_are_split_like_the_shell_and_read_env() {
    let (rs, warnings) = rules(
        r#"
[[commands.guard]]
program = "git"
reason = "環境変数"
deny-env = ["GIT_CONFIG_COUNT"]
examples = { deny = ["GIT_CONFIG_COUNT=1 git push"], allow = ["git push"] }
"#,
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(rs.len(), 1);
    assert!(matches(&rs, "GIT_CONFIG_COUNT=1 git push"));
}

// @kotowari[REQ-034]
#[test]
fn req_034_diagnostics_in_outer_or_nested_examples_disable_only_that_rule() {
    for example in ["git push; if true; then", "eval 'git push &&'"] {
        let (rs, warnings) = rules(&format!(
            r#"
[[commands.guard]]
program = "git"
reason = "invalid example"
deny = [["push"]]
examples = {{ deny = ["{example}"] }}
[[commands.guard]]
program = "git"
reason = "valid example"
deny = [["push"]]
examples = {{ deny = ["git push"], allow = ["git status"] }}
"#
        ));
        assert_eq!(rs.len(), 1, "{example}");
        assert_eq!(rs[0].reason, "valid example");
        assert_eq!(warnings.len(), 1);
    }
}

// @kotowari[REQ-033, EX-037]
#[test]
fn req_033_matched_rule_joins_composition_with_reason() {
    let dir = fixture_dir("command-guardian-only-");
    let root = dir.path().canonicalize().unwrap();
    let user = root.join("config.toml");
    write(
        &user,
        r#"
[[commands.guard]]
program = "git"
reason = "push はほかのブランチに影響します"
verdict = "ask"
deny = [["push"]]
"#,
    );
    let e = Engine::load(Some(&user), env(&root));
    let r = e.check("git push origin main");
    assert_eq!(r.verdict, Verdict::Ask);
    assert_eq!(r.rules.len(), 1);
    assert!(
        r.message.contains("push はほかのブランチに影響します"),
        "{}",
        r.message
    );
    assert_eq!(e.check("git status").verdict, Verdict::Allow);
}

// @kotowari[REQ-033]
#[test]
fn req_033_multiple_rules_compose_with_the_worst_verdict() {
    let dir = fixture_dir("command-guardian-only-");
    let root = dir.path().canonicalize().unwrap();
    let user = root.join("config.toml");
    write(
        &user,
        r#"
[[commands.guard]]
program = "git"
reason = "push は確認"
verdict = "ask"
deny = [["push"]]

[[commands.guard]]
program = "git"
reason = "push --force は止める"
verdict = "block"
deny = [["push"]]
deny-flags = ["--force"]
"#,
    );
    let e = Engine::load(Some(&user), env(&root));
    let r = e.check("git push --force");
    assert_eq!(r.verdict, Verdict::Block);
    assert_eq!(r.rules.len(), 2);
}

// @kotowari[REQ-033]
#[test]
fn req_033_project_config_guard_rules_apply() {
    let dir = fixture_dir("command-guardian-only-");
    let root = dir.path().canonicalize().unwrap();
    write(
        &root.join(".command-guardian.toml"),
        r#"
[[commands.guard]]
program = "git"
reason = "プロジェクトの規則"
verdict = "block"
deny = [["push"]]
"#,
    );
    let e = Engine::load(None, env(&root));
    let r = e.check("git push origin main");
    assert_eq!(r.verdict, Verdict::Block);
    assert_eq!(r.rules.len(), 1);
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
}
