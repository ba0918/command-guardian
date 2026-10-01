//! 1 回の判定で行う構文解析の予算（REQ-039）。
//!
//! 回数（1000 回）と時間（5 秒）の上限は固定値にする。判定の入口が `begin()` を
//! 呼び、判定の間の子とのやり取りが `take()` で 1 回ずつ消費する。`begin()` を
//! 呼ばない直接の解析は判定の外なので数えない。

use guardian_parser::{LIMIT_EXCHANGES, LIMIT_JUDGMENT_TIME};
use std::cell::Cell;
use std::time::{Duration, Instant};

thread_local! {
    static STATE: Budget = const { Budget {
        exchanges: Cell::new(0),
        started: Cell::new(None),
        exceeded: Cell::new(false),
    } };
}

struct Budget {
    exchanges: Cell<usize>,
    started: Cell<Option<Instant>>,
    exceeded: Cell<bool>,
}

/// 判定の始まりで予算を数え直す。
pub(crate) fn begin() {
    STATE.with(|state| {
        state.exchanges.set(0);
        state.started.set(Some(Instant::now()));
        state.exceeded.set(false);
    });
}

/// 構文解析の 1 回分を使う。回数か時間の上限を超えていれば false にして、
/// その判定が予算を使い切ったことを記録する。
pub(crate) fn take() -> bool {
    STATE.with(|state| {
        let Some(started) = state.started.get() else {
            return true;
        };
        if state.exchanges.get() >= LIMIT_EXCHANGES || started.elapsed() >= LIMIT_JUDGMENT_TIME {
            state.exceeded.set(true);
            return false;
        }
        state.exchanges.set(state.exchanges.get() + 1);
        true
    })
}

/// 判定の時間の上限の内側か。構文解析以外のやり取り（引用の除去）に使う。
/// 回数の予算は消費しない。
pub(crate) fn within_time() -> bool {
    STATE.with(|state| {
        let Some(started) = state.started.get() else {
            return true;
        };
        if started.elapsed() >= LIMIT_JUDGMENT_TIME {
            state.exceeded.set(true);
            return false;
        }
        true
    })
}

/// この判定の残り時間。判定の外では None（時間の上限を当てない）。
pub(crate) fn remaining() -> Option<Duration> {
    STATE.with(|state| {
        let started = state.started.get()?;
        Some(LIMIT_JUDGMENT_TIME.saturating_sub(started.elapsed()))
    })
}

/// この判定で予算を使い切ったか。
pub(crate) fn exceeded() -> bool {
    STATE.with(|state| state.exceeded.get())
}
