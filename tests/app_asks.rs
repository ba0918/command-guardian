//! S7: 構文解析由来の ask の合成と文面（REQ-009・REQ-011・REQ-038）。

use guardian_core::Verdict;
mod app_support;
use app_support::{Engine, EngineEnv};
use std::path::PathBuf;

fn engine() -> Engine {
    Engine::load(
        None,
        EngineEnv {
            home: Some(PathBuf::from("/home/you")),
            tmpdir: Some(PathBuf::from("/tmp")),
            cwd: PathBuf::from("/tmp/scratch"),
        },
    )
}

// @kotowari[REQ-009, REQ-038]
#[test]
fn req_009_a_readable_block_is_not_overwritten_by_a_parse_ask() {
    let report = engine().check("rm -rf /etc/x; if true; then");
    assert_eq!(report.verdict, Verdict::Block);
    assert!(!report.parse_errors.is_empty());
    assert!(report.message.contains("protected"), "{}", report.message);
    assert!(report.reason.contains("protected"), "{}", report.reason);
}

// @kotowari[REQ-009]
#[test]
fn req_009_a_rule_block_names_the_rule_in_the_reason() {
    // 見張りの規則で block になるとき、構文解析の ask が理由を上書きしない。
    let dir = tempfile::Builder::new()
        .prefix("command-guardian-asks-")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap();
    let root = dir.path().canonicalize().unwrap();
    let user = root.join("config.toml");
    std::fs::write(
        &user,
        r#"
[[commands.guard]]
program = "git"
reason = "別名で push を定義して実行することはできません"
verdict = "block"
options-with-value = ["-c"]
deny-option-values = { "-c" = ["/alias[.].*/"] }
"#,
    )
    .unwrap();
    let engine = Engine::load(
        Some(&user),
        EngineEnv {
            home: Some(PathBuf::from("/home/you")),
            tmpdir: Some(PathBuf::from("/tmp")),
            cwd: root.clone(),
        },
    );
    let report = engine.check("git -c alias.p=push p; if true; then");
    assert_eq!(report.verdict, Verdict::Block);
    assert!(!report.parse_errors.is_empty());
    assert!(report.reason.contains("別名"), "{}", report.reason);
}

// @kotowari[REQ-038, EX-049]
#[test]
fn req_038_an_unreadable_input_without_effects_asks() {
    let report = engine().check("if true; then rm -rf /etc/x");
    assert_eq!(report.verdict, Verdict::Ask);
    assert!(
        report.message.contains("command syntax could not be read"),
        "{}",
        report.message
    );
    assert!(
        (2..=4).contains(&report.message.lines().count()),
        "{}",
        report.message
    );
}

// @kotowari[EX-050]
#[test]
fn ex_050_input_over_the_limit_blocks() {
    let command = "echo x; ".repeat(200_000);
    let report = engine().check(&command);
    assert_eq!(report.verdict, Verdict::Block);
    assert!(report.message.contains("too large"), "{}", report.message);
    assert!(report.reason.contains("too large"), "{}", report.reason);
}

// @kotowari[REQ-039]
#[test]
fn req_039_input_over_the_depth_limit_blocks() {
    // 算術式の内側の 129 段は上限を超える。理由をつけて block にする。
    let at = "(".repeat(129);
    let close = ")".repeat(129);
    let report = engine().check(&format!("echo $(( {at}1{close} ))"));
    assert_eq!(report.verdict, Verdict::Block);
    assert!(
        report.message.contains("nested too deeply"),
        "{}",
        report.message
    );
    assert!(
        report.reason.contains("nested too deeply"),
        "{}",
        report.reason
    );
}

// @kotowari[REQ-039]
#[test]
fn req_039_depth_at_the_limit_is_judged_normally() {
    let at = "(".repeat(128);
    let close = ")".repeat(128);
    let report = engine().check(&format!("echo $(( {at}1{close} ))"));
    assert_eq!(report.verdict, Verdict::Allow);
}

// @kotowari[REQ-011, REQ-035]
#[test]
fn req_011_an_unreadable_shell_message_shows_the_reason() {
    let report = engine().check("fish -c 'rm -rf /etc/x'");
    assert_eq!(report.verdict, Verdict::Ask);
    assert!(
        report.message.contains("Unsupported shell"),
        "{}",
        report.message
    );
    assert!(
        report.reason.contains("Unsupported shell"),
        "{}",
        report.reason
    );
    assert!(
        report.message.contains("Alternative:"),
        "{}",
        report.message
    );
}
