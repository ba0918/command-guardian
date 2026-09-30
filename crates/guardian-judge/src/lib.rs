//! hook-guardian の judge。fs と git からパスを 4 分類する。設定は読まない。

pub mod classify;
pub mod git;

pub use classify::{under_root, Classification, Judge, JudgeEnv};
pub use git::{find_worktree_root, GitError, GitRunner, SystemGit};
