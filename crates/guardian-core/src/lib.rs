//! command-guardian の core。正規化した構文木からの破壊的効果の抽出、パスの解決、
//! 判定の共通の型を持つ。字句解析と構文解析は guardian-parser が担う。

pub mod extract;
pub mod types;

pub use extract::{analyze, extract_effects, Analysis, Ask, Env};
pub use types::{Class, Effect, Op, ProtectedKind, Target, Verdict, Why};
