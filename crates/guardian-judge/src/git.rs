//! git の起動。テストから差し替えられるように trait にする。

use std::io::Read;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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
    fn status_until(
        &self,
        root: &Path,
        path: &Path,
        deadline: Instant,
    ) -> Result<String, GitFailure> {
        let result = self.status(root, path).map_err(GitFailure::Failed);
        if Instant::now() >= deadline {
            Err(GitFailure::Limit)
        } else {
            result
        }
    }
}

#[derive(Debug)]
pub enum GitFailure {
    Limit,
    Failed(GitError),
}

/// 本物の git を起動する。
pub struct SystemGit {
    executable: PathBuf,
}
impl Default for SystemGit {
    fn default() -> Self {
        Self::new("git".into())
    }
}
impl SystemGit {
    pub fn new(executable: PathBuf) -> Self {
        Self { executable }
    }
}

impl GitRunner for SystemGit {
    fn status(&self, root: &Path, path: &Path) -> Result<String, GitError> {
        self.status_until(root, path, Instant::now() + Duration::from_secs(5))
            .map_err(|failure| match failure {
                GitFailure::Failed(error) => error,
                GitFailure::Limit => GitError("git の待ち時間が上限を超えた".into()),
            })
    }
    fn status_until(
        &self,
        root: &Path,
        path: &Path,
        deadline: Instant,
    ) -> Result<String, GitFailure> {
        if Instant::now() >= deadline {
            return Err(GitFailure::Limit);
        }
        let mut command = Command::new(&self.executable);
        command
            .arg("-C")
            .arg(root)
            .args(["status", "--porcelain", "-uall", "--"])
            .arg(path);
        // git がフックなどに渡す環境（GIT_DIR・GIT_INDEX_FILE など）を引き継ぐと、
        // 対象の作業ツリーではなく呼び出し元のリポジトリを見てしまう。
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("GIT_") {
                command.env_remove(&name);
            }
        }
        collect_status(command, deadline)
    }
}

/// 出力の読取も同じ期限の中で進める。読取threadを作らないので、子の死亡後に
/// 継承された出力FDを持つ別プロセスがいてもreaderのjoinを待ち続けない。
fn collect_status(mut command: Command, deadline: Instant) -> Result<String, GitFailure> {
    let failed = |error: std::io::Error| GitFailure::Failed(GitError(error.to_string()));
    let (mut stdout, out_child) = UnixStream::pair().map_err(failed)?;
    let (mut stderr, err_child) = UnixStream::pair().map_err(failed)?;
    stdout.set_nonblocking(true).map_err(failed)?;
    stderr.set_nonblocking(true).map_err(failed)?;
    let child = command
        .stdout(Stdio::from(OwnedFd::from(out_child)))
        .stderr(Stdio::from(OwnedFd::from(err_child)))
        .spawn()
        .map_err(failed)?;
    let mut child = ReapedChild(child);
    drop(command);
    let mut text = StatusText::default();
    let mut error_text = Vec::new();
    let mut out_eof = false;
    let mut err_eof = false;
    let mut status = None;
    loop {
        if Instant::now() >= deadline {
            return Err(GitFailure::Limit);
        }
        let mut progress = false;
        for (reader, eof, is_stdout) in [
            (&mut stdout, &mut out_eof, true),
            (&mut stderr, &mut err_eof, false),
        ] {
            if *eof {
                continue;
            }
            let mut buffer = [0u8; 8192];
            match reader.read(&mut buffer) {
                Ok(0) => {
                    *eof = true;
                    progress = true;
                }
                Ok(n) => {
                    progress = true;
                    if is_stdout {
                        text.feed(&buffer[..n]);
                    } else {
                        error_text.extend_from_slice(
                            &buffer[..n.min(4096usize.saturating_sub(error_text.len()))],
                        );
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(failed(error)),
            }
        }
        if status.is_none() {
            status = child.0.try_wait().map_err(failed)?;
        }
        if let Some(status) = status {
            if out_eof && err_eof {
                return if status.success() {
                    Ok(text.report())
                } else {
                    Err(GitFailure::Failed(GitError(format!(
                        "git status が失敗した: {}",
                        String::from_utf8_lossy(&error_text).trim()
                    ))))
                };
            }
        }
        if !progress {
            std::thread::sleep(
                Duration::from_millis(1).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }
}

struct ReapedChild(std::process::Child);
impl Drop for ReapedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// 分類に使う情報だけを保存する。出力の量による新しい受理上限は設けない。
#[derive(Default)]
struct StatusText {
    changed: bool,
    untracked: bool,
    column: u8,
    first_question: bool,
}
impl StatusText {
    fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.changed |= !byte.is_ascii_whitespace();
            if byte == b'\n' {
                self.column = 0;
                self.first_question = false;
                continue;
            }
            if self.column == 0 {
                self.first_question = byte == b'?';
            }
            if self.column == 1 && self.first_question && byte == b'?' {
                self.untracked = true;
            }
            self.column = self.column.saturating_add(1);
        }
    }
    fn report(self) -> String {
        if self.untracked {
            "??".into()
        } else if self.changed {
            " M".into()
        } else {
            String::new()
        }
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
