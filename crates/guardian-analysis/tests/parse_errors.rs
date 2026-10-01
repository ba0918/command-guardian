//! S6: 解析の失敗の検出（REQ-010）。

use guardian_analysis::analyze;
use guardian_core::{Ask, Env};
use std::path::PathBuf;

fn env() -> Env {
    Env {
        home: Some(PathBuf::from("/home/you")),
        tmpdir: Some(PathBuf::from("/tmp")),
        cwd: Some(PathBuf::from("/home/you/work/repo")),
    }
}

// @kotowari[REQ-010]
#[test]
fn req_010_non_literal_program_word_is_unreadable() {
    // プログラムの語がリテラルでないと、何が実行されるか読めない。
    for cmd in ["CMD=rm; $CMD -rf /etc/x", "$(echo rm) -rf /etc/x"] {
        let a = analyze(cmd, &env());
        assert!(!a.parse_errors.is_empty(), "{cmd}");
    }
    // リテラルのプログラムはそのまま読める。
    assert!(analyze("rm -rf /etc/x", &env()).parse_errors.is_empty());
}

// @kotowari[REQ-010]
#[test]
fn req_010_unterminated_quotes_are_parse_errors() {
    for cmd in [
        "rm -rf '/etc/foo",
        "rm -rf \"/etc/foo",
        "rm -rf `rm /etc/foo",
        "rm -rf $(rm /etc/foo",
        "rm -rf ${HOME",
    ] {
        let a = analyze(cmd, &env());
        assert!(!a.parse_errors.is_empty(), "{cmd}");
    }
    // 閉じた入力は失敗にしない。
    assert!(analyze("rm -rf /etc/foo", &env()).parse_errors.is_empty());
    assert!(analyze("rm -rf $(rm /etc/foo)", &env())
        .parse_errors
        .is_empty());
}

// @kotowari[REQ-039]
#[test]
fn req_039_limit_failures_become_blocks() {
    // 上限の超過と、入力に帰せる子の死（Limit）は block の原因。
    for failure in [
        guardian_parser::Failure::TooLarge,
        guardian_parser::Failure::TooDeep,
        guardian_parser::Failure::Limit,
    ] {
        assert!(Ask::Parse(failure.clone()).is_limit(), "{failure:?}");
    }
    // 自分に帰せる死（panic、起動とプロトコルの失敗）は ask のまま（REQ-039・A23）。
    for failure in [
        guardian_parser::Failure::Panic,
        guardian_parser::Failure::Internal,
    ] {
        assert!(!Ask::Parse(failure.clone()).is_limit(), "{failure:?}");
    }
    // 上限の内側で読めないものは ask のまま。
    assert!(!Ask::Parse(guardian_parser::Failure::Syntax).is_limit());
    assert!(!Ask::Parse(guardian_parser::Failure::UnknownNode("x".into())).is_limit());
    assert!(!Ask::UnreadableEval.is_limit());
    assert!(!Ask::UnsupportedShell("fish".into()).is_limit());
}
