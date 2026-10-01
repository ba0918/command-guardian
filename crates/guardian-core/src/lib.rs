//! 解析・観測・規則が共有する値。実行基盤と構文木には依存しない。

mod diagnostic;
pub mod types;

pub use diagnostic::{Ask, Env, Failure};
pub use types::{Class, Effect, Op, ProtectedKind, Target, Verdict, Why};
