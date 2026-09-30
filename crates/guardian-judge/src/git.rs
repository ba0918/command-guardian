//! git の起動。テストから差し替えられるように trait にする。

use std::path::{Path, PathBuf};
use std::process::Command;

/// git の起動の失敗。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitError(pub String);

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// 1 つのパスについて `git status --porcelain -uall` の報告を返す。
pub trait GitRunner: Send + Sync {
    fn status(&self, root: &Path, path: &Path) -> Result<String, GitError>;
}

/// 本物の git を起動する。
pub struct SystemGit;

impl GitRunner for SystemGit {
    fn status(&self, root: &Path, path: &Path) -> Result<String, GitError> {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["status", "--porcelain", "-uall", "--"])
            .arg(path)
            .output()
            .map_err(|e| GitError(format!("git を起動できない: {e}")))?;
        if !out.status.success() {
            return Err(GitError(format!(
                "git status が失敗した: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }
}

/// git の作業ツリーのルートを `.git` の上方探索だけで探す。git は起動しない。
pub fn find_worktree_root(path: &Path) -> Option<PathBuf> {
    let start = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };
    for dir in start.ancestors() {
        if dir.join(".git").symlink_metadata().is_ok() {
            return Some(dir.to_path_buf());
        }
    }
    None
}
