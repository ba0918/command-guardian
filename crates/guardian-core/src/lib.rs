//! hook-guardian の core。シェル構文の解析、破壊的効果の抽出、パスの解決、
//! 判定の共通の型を持つ。設定にも fs にも git にも依存しない。

pub mod extract;
pub mod parse;
pub mod types;

pub use extract::{analyze, extract_effects, Analysis, Env};
pub use parse::strip_quotes_and_heredocs;
pub use types::{Class, Effect, Op, ProtectedKind, Target, Verdict, Why};
