//! S7: 構文解析由来の ask の合成と文面（REQ-009・REQ-011・REQ-038）。

use guardian_core::Verdict;
use guardian_policy::{Engine, EngineEnv};
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

// @kotowari[REQ-038, EX-049]
#[test]
fn req_038_an_unreadable_input_without_effects_asks() {
    let report = engine().check("if true; then rm -rf /etc/x");
    assert_eq!(report.verdict, Verdict::Ask);
    assert!(
        report.message.contains("構文を読めない"),
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
fn ex_050_input_over_the_limit_asks() {
    let command = "echo x; ".repeat(200_000);
    let report = engine().check(&command);
    assert_eq!(report.verdict, Verdict::Ask);
    assert!(
        report.message.contains("入力が大きすぎる"),
        "{}",
        report.message
    );
    assert!(report.reason.contains("大きすぎる"), "{}", report.reason);
}

// @kotowari[REQ-011, REQ-035]
#[test]
fn req_011_an_unreadable_shell_message_shows_the_reason() {
    let report = engine().check("fish -c 'rm -rf /etc/x'");
    assert_eq!(report.verdict, Verdict::Ask);
    assert!(
        report.message.contains("読めないシェル"),
        "{}",
        report.message
    );
    assert!(
        report.reason.contains("読めないシェル"),
        "{}",
        report.reason
    );
    assert!(report.message.contains("代替:"), "{}", report.message);
}
