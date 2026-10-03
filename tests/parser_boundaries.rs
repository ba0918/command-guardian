use guardian_core::Failure;
use guardian_parser::{LIMIT_BYTES, LIMIT_DEPTH};
fn runtime() -> guardian_app::runtime::ParserRuntime {
    guardian_app::runtime::ParserRuntime::new(env!("CARGO_BIN_EXE_command-guardian").into())
}

// @kotowari[REQ-039]
#[test]
fn req_039_cumulative_substitution_reads_are_measured() {
    let input = format!(
        "echo {}{}{}",
        "$(".repeat(100),
        "echo y; ".repeat(2500),
        ")".repeat(100)
    );
    assert!(input.len() < LIMIT_BYTES);
    assert!(
        runtime()
            .validation()
            .parse(&input)
            .failures
            .contains(&Failure::TooLarge)
    );
}
// @kotowari[REQ-039]
#[test]
fn req_039_depth_over_the_limit_is_too_deep() {
    let input = format!(
        "{}true{}",
        "{ ".repeat(LIMIT_DEPTH + 1),
        " ; }".repeat(LIMIT_DEPTH + 1)
    );
    let outcome = runtime().validation().parse(&input);
    assert_eq!(outcome.failures, vec![Failure::TooDeep]);
    assert!(outcome.script.is_empty());
}
// @kotowari[REQ-039]
#[test]
fn req_039_depth_at_the_limit_is_read() {
    let input = format!(
        "echo {}true{}",
        "$(".repeat(LIMIT_DEPTH),
        ")".repeat(LIMIT_DEPTH)
    );
    assert!(runtime().validation().parse(&input).failures.is_empty());
}
// @kotowari[REQ-036, REQ-039]
#[test]
fn req_036_strip_quotes_survives_a_deep_input() {
    let deep = format!(
        "echo $(true)#{}: {} ; git push origin main",
        "$(".repeat(2000),
        ")".repeat(2000)
    );
    assert!(
        runtime()
            .validation()
            .strip_quotes(&deep)
            .unwrap()
            .contains("git push origin main")
    );
}
