//! 解析・観測・規則が共有する値。実行基盤と構文木には依存しない。

mod diagnostic;
mod facts;
mod observation;
pub mod types;

pub use diagnostic::{Ask, Env, Failure};
pub use facts::{CommandFacts, Invocation};
pub use observation::ObservedPath;
pub use types::{Class, Effect, Op, ProtectedKind, Target, Verdict, Why};
