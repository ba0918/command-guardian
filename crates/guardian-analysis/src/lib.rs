//! 正規化したシェル構文の意味解析。

pub mod command;
mod effects;

pub use command::{
    basename, shell_c_index, shell_invocation, shell_kind, split_assignment,
    split_prefix_assignments, strip_wrapper, ShellInvocation, ShellKind,
};

pub use effects::{analyze, extract_effects, Analysis};
pub use guardian_core::{Ask, Env};
