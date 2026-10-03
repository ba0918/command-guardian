//! command-guardian の judge。fs と git からパスを 4 分類する。設定は読まない。

pub mod classify;
pub mod git;
pub mod observation;

pub use classify::{Classification, Judge, JudgeEnv, under_root};
pub use git::{GitError, GitFailure, GitRunner, SystemGit, find_worktree_root};
pub use observation::{PathObserver, SystemPaths};
