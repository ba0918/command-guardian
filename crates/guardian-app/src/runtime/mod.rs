mod budget;
mod wire;
mod worker;

use guardian_parser::{Failure, Outcome, Script};
pub use worker::run_if_child;
pub use worker::ParserRuntime;

pub(crate) fn begin_judgment() {
    budget::begin();
}
pub(crate) fn judgment_over_budget() -> bool {
    budget::exceeded()
}

impl ParserRuntime {
    pub fn parse(&mut self, input: &str) -> Outcome {
        if input.len() > guardian_parser::LIMIT_BYTES {
            return Outcome {
                script: Script::default(),
                failures: vec![Failure::TooLarge],
            };
        }
        self.request_parse(input).unwrap_or_else(|failure| Outcome {
            script: Script::default(),
            failures: vec![failure],
        })
    }

    pub fn strip_quotes(&mut self, input: &str) -> Result<String, Failure> {
        self.request_strip_quotes(input)
    }
}

fn parse_in_child(input: &str) -> (Vec<Failure>, Option<Script>) {
    match in_bounded_thread(worker::CHILD_STACK_BYTES, || guardian_parser::parse(input)) {
        Some(outcome) => (outcome.failures, Some(outcome.script)),
        None => (vec![Failure::Panic], None),
    }
}

fn strip_quotes_in_child(input: &str) -> Result<String, Failure> {
    in_bounded_thread(worker::STRIP_STACK_BYTES, || {
        guardian_parser::strip_quotes(input)
    })
    .unwrap_or(Err(Failure::Panic))
}

fn in_bounded_thread<T: Send>(stack: usize, f: impl FnOnce() -> T + Send) -> Option<T> {
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("guardian-parser".to_string())
            .stack_size(stack)
            .spawn_scoped(scope, f)
            .ok()?;
        handle.join().ok()
    })
}
