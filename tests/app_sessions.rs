mod app_support;
use app_support::{Engine, EngineEnv};
use guardian_core::Verdict;
use std::path::PathBuf;

fn env() -> EngineEnv {
    EngineEnv {
        home: None,
        tmpdir: Some(PathBuf::from("/tmp")),
        cwd: PathBuf::from("/tmp"),
    }
}

// @kotowari[REQ-034, REQ-039]
#[test]
fn req_034_many_valid_examples_and_later_checks_are_independent() {
    let previous = Engine::new(guardian_policy::Config::builtin(None), env());
    assert_eq!(previous.check("true").verdict, Verdict::Allow);
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let file = dir.path().join("config.toml");
    let examples = std::iter::repeat_n("\"git push\"", 1002)
        .collect::<Vec<_>>()
        .join(",");
    std::fs::write(&file, format!("[[commands.guard]]\nprogram='git'\nreason='push'\ndeny=[['push']]\n[commands.guard.examples]\ndeny=[{examples}]\nallow=['git status']\n")).unwrap();
    let engine = Engine::load(Some(&file), env());
    assert_eq!(engine.config().guard.len(), 1);
    for _ in 0..2 {
        let report = engine.check("git push");
        assert_eq!(report.verdict, Verdict::Ask);
        assert!(report.parse_errors.is_empty(), "{:?}", report.parse_errors);
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }
}

// @kotowari[REQ-039, EX-050]
#[test]
fn req_039_exchange_limit_quotes_and_next_judgment() {
    let mut runtime =
        guardian_app::runtime::ParserRuntime::new(env!("CARGO_BIN_EXE_command-guardian").into());
    {
        let mut session = runtime.judgment();
        for _ in 0..1000 {
            assert!(session.parse("true").failures.is_empty());
        }
        assert_eq!(session.strip_quotes("'true'").unwrap(), "true");
        assert_eq!(
            session.parse("true").failures,
            vec![guardian_core::Failure::Limit]
        );
        assert!(session.over_budget());
    }
    assert!(runtime.judgment().parse("true").failures.is_empty());
    for _ in 0..1002 {
        assert!(runtime.validation().parse("true").failures.is_empty());
    }
    assert!(runtime.judgment().parse("true").failures.is_empty());
}

// @kotowari[REQ-039]
#[test]
fn req_039_failed_start_is_internal_and_children_are_reaped() {
    let mut missing =
        guardian_app::runtime::ParserRuntime::new("/nonexistent/guardian-host".into());
    assert_eq!(
        missing.judgment().parse("true").failures,
        vec![guardian_core::Failure::Internal]
    );
    let children = || std::fs::read_to_string("/proc/thread-self/children").unwrap();
    let before = children();
    {
        let mut invalid = guardian_app::runtime::ParserRuntime::new("/bin/false".into());
        assert_eq!(
            invalid.judgment().parse("true").failures,
            vec![guardian_core::Failure::Internal]
        );
        assert_eq!(children(), before);
        let mut runtime = guardian_app::runtime::ParserRuntime::new(
            env!("CARGO_BIN_EXE_command-guardian").into(),
        );
        assert!(runtime.judgment().parse("true").failures.is_empty());
        assert_ne!(children(), before);
        let deep = format!("echo {}true{}", "$(".repeat(2000), ")".repeat(2000));
        assert!(runtime
            .judgment()
            .parse(&deep)
            .failures
            .iter()
            .any(|f| matches!(
                f,
                guardian_core::Failure::Limit | guardian_core::Failure::TooDeep
            )));
        assert!(runtime.judgment().parse("true").failures.is_empty());
    }
    assert_eq!(children(), before);
}

// @kotowari[REQ-034, REQ-037]
#[test]
fn req_034_assignment_substitution_is_one_example_invocation() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let file = dir.path().join("config.toml");
    std::fs::write(&file, "[[commands.guard]]\nprogram='git'\nreason='push'\ndeny=[['push']]\n[commands.guard.examples]\ndeny=['X=$(git push)']\n").unwrap();
    let engine = Engine::load(Some(&file), env());
    assert_eq!(engine.config().guard.len(), 1);
    assert_eq!(engine.check("X=$(git push)").verdict, Verdict::Ask);
}
