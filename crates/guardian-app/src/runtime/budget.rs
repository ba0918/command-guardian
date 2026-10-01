//! 一判定だけが所有する回数と経過時間。
use std::time::{Duration, Instant};
const LIMIT_EXCHANGES: usize = 1000;
const LIMIT_JUDGMENT_TIME: Duration = Duration::from_secs(5);

pub(super) struct Budget {
    exchanges: usize,
    started: Instant,
    exceeded: bool,
}
impl Budget {
    pub(super) fn new() -> Self {
        Self {
            exchanges: 0,
            started: Instant::now(),
            exceeded: false,
        }
    }
    pub(super) fn take(&mut self) -> bool {
        if self.exchanges >= LIMIT_EXCHANGES || !self.within_time() {
            self.exceeded = true;
            return false;
        }
        self.exchanges += 1;
        true
    }
    pub(super) fn within_time(&self) -> bool {
        !self.remaining().is_zero()
    }
    pub(super) fn remaining(&self) -> Duration {
        LIMIT_JUDGMENT_TIME.saturating_sub(self.started.elapsed())
    }
    pub(super) fn exceeded(&self) -> bool {
        self.exceeded || !self.within_time()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // @kotowari[REQ-039]
    #[test]
    fn req_039_completion_checks_elapsed_time_without_another_request() {
        let mut budget = Budget::new();
        budget.started = Instant::now() - LIMIT_JUDGMENT_TIME;
        assert!(budget.exceeded());
        assert!(!budget.take());
        assert!(!Budget::new().exceeded());
    }
}
