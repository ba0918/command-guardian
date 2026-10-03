//! 正規化したシェル構文の意味解析。

pub mod command;
mod effects;

pub use command::{
    ShellInvocation, ShellKind, basename, shell_c_index, shell_invocation, shell_kind,
    split_assignment, split_prefix_assignments, strip_wrapper,
};

pub use effects::{
    Analysis, AnalysisControl, analyze, analyze_invocations, analyze_with_control, extract_effects,
};
pub use guardian_core::{Ask, Env};
