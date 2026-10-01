//! command-guardian の policy。設定の読み込み、判定の合成、文面を作る。

pub mod config;
pub mod evaluation;
pub mod guard;
pub mod layers;
pub mod message;

pub use config::{Config, CustomRule};
pub use evaluation::{EffectReport, Policy, Report, RuleReport};
pub use guard::{GuardRule, Invocation};
