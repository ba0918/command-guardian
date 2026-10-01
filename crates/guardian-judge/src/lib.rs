//! command-guardian の judge。fs と git からパスを 4 分類する。設定は読まない。

pub mod classify;
pub mod git;
pub mod observation;

pub use classify::{under_root, Classification, Judge, JudgeEnv};
pub use git::{find_worktree_root, GitError, GitFailure, GitRunner, SystemGit};
pub use observation::{PathObserver, SystemPaths};
