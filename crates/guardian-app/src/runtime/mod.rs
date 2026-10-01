mod budget;
mod wire;
mod worker;

use guardian_parser::{Failure, Outcome, Script};
pub use worker::run_if_child;
pub use worker::ParserRuntime;

impl ParserRuntime {
    pub fn judgment(&mut self) -> JudgmentSession<'_> {
        JudgmentSession {
            runtime: self,
            budget: budget::Budget::new(),
        }
    }
    pub fn validation(&mut self) -> ValidationSession<'_> {
        ValidationSession { runtime: self }
    }
    fn parse(&mut self, input: &str, budget: Option<&mut budget::Budget>) -> Outcome {
        if input.len() > guardian_parser::LIMIT_BYTES {
            return Outcome {
                script: Script::default(),
                failures: vec![Failure::TooLarge],
            };
        }
        self.request_parse(input, budget)
            .unwrap_or_else(|failure| Outcome {
                script: Script::default(),
                failures: vec![failure],
            })
    }
}

pub struct JudgmentSession<'a> {
    runtime: &'a mut ParserRuntime,
    budget: budget::Budget,
}
impl JudgmentSession<'_> {
    pub fn parse(&mut self, input: &str) -> Outcome {
        self.runtime.parse(input, Some(&mut self.budget))
    }
    pub fn strip_quotes(&mut self, input: &str) -> Result<String, Failure> {
        self.runtime
            .request_strip_quotes(input, Some(&mut self.budget))
    }
    pub fn over_budget(&self) -> bool {
        self.budget.exceeded()
    }
    pub fn remaining(&self) -> std::time::Duration {
        self.budget.remaining()
    }
}
pub struct ValidationSession<'a> {
    runtime: &'a mut ParserRuntime,
}
impl ValidationSession<'_> {
    pub fn parse(&mut self, input: &str) -> Outcome {
        self.runtime.parse(input, None)
    }
    pub fn strip_quotes(&mut self, input: &str) -> Result<String, Failure> {
        self.runtime.request_strip_quotes(input, None)
    }
}

fn parse_in_child(input: &str) -> (Vec<Failure>, Option<Script>) {
    parse_in_child_with(input, guardian_parser::parse)
}
fn parse_in_child_with(input: &str, parse: fn(&str) -> Outcome) -> (Vec<Failure>, Option<Script>) {
    match in_bounded_thread(worker::CHILD_STACK_BYTES, || parse(input)) {
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

#[cfg(test)]
mod tests {
    use super::*;
    // @kotowari[REQ-039]
    #[test]
    fn req_039_a_panicking_parser_becomes_a_failure() {
        fn boom(_: &str) -> Outcome {
            panic!("panic in the injected parser")
        }
        let (failures, script) = parse_in_child_with("true", boom);
        assert_eq!(failures, vec![Failure::Panic]);
        assert!(script.is_none());
    }
}
