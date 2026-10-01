//! パスの分類。fs を見て 4 分類する。設定は読まない。

use crate::git::{find_worktree_root, GitFailure, GitRunner, SystemGit};
use guardian_core::{Class, Failure, ObservedPath, ProtectedKind, Why};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

/// 分類に渡す環境。
#[derive(Debug, Clone, Default)]
pub struct JudgeEnv {
    pub home: Option<PathBuf>,
    pub tmpdir: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    pub git_enabled: bool,
}

/// 分類の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    pub class: Class,
    pub why: Why,
}

impl Classification {
    fn new(class: Class, why: Why) -> Classification {
        Classification { class, why }
    }
}

pub struct Judge {
    env: JudgeEnv,
    git: Box<dyn GitRunner>,
}

/// 配下のすべてに当てるシステムの領域。
const SYSTEM_AREAS: &[&str] = &[
    "/etc", "/usr", "/bin", "/sbin", "/lib", "/lib64", "/boot", "/dev", "/proc", "/sys", "/run",
    "/opt", "/srv", "/root", "/var",
];

impl Judge {
    pub fn new(env: JudgeEnv) -> Judge {
        Judge {
            env,
            git: Box::new(SystemGit::default()),
        }
    }

    /// git の起動を差し替えた judge。テストに使う。
    pub fn with_git(env: JudgeEnv, git: Box<dyn GitRunner>) -> Judge {
        Judge { env, git }
    }

    /// 1 つのパスを分類する。`dereference` は末尾スラッシュ付きの削除。
    pub fn classify_path(&self, path: &Path, dereference: bool) -> Classification {
        let path = self.resolve_dereference(path, dereference);
        self.classify_inner(&path, false, Instant::now() + Duration::from_secs(5))
            .unwrap_or_else(|_| Classification::new(Class::Unknown, Why::GitFailed))
    }

    /// 供給元の子（find・xargs・for の対象集合）を分類する。
    pub fn classify_children(&self, base: &Path) -> Classification {
        self.classify_children_deref(base, false)
    }

    /// 末尾スラッシュ付きの起点は、リンク先を解決してから子を分類する。
    pub fn classify_children_deref(&self, base: &Path, dereference: bool) -> Classification {
        let base = self.resolve_dereference(base, dereference);
        self.classify_inner(&base, true, Instant::now() + Duration::from_secs(5))
            .unwrap_or_else(|_| Classification::new(Class::Unknown, Why::GitFailed))
    }
    pub fn classify_observed(
        &self,
        path: &ObservedPath,
        deadline: Instant,
    ) -> Result<Classification, Failure> {
        self.classify_inner(&path.path, path.children, deadline)
    }

    fn resolve_dereference(&self, path: &Path, dereference: bool) -> PathBuf {
        if dereference {
            std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        } else {
            path.to_path_buf()
        }
    }

    /// パスを含む git の作業ツリーのルート。`.git` の上方探索だけで決め、git は起動しない。
    pub fn worktree_root(&self, path: &Path) -> Option<PathBuf> {
        find_worktree_root(path)
    }

    fn ephemeral_roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![PathBuf::from("/tmp"), PathBuf::from("/var/tmp")];
        if let Some(t) = &self.env.tmpdir {
            // 空のルートはすべてのパスに一致してしまうため足さない。
            if !t.as_os_str().is_empty() && !roots.contains(t) {
                roots.push(t.clone());
            }
        }
        roots
    }

    fn classify_inner(
        &self,
        path: &Path,
        children: bool,
        deadline: Instant,
    ) -> Result<Classification, Failure> {
        // "/" それ自体、およびその子（children のとき）は保護。
        if path == Path::new("/") {
            return Ok(Classification::new(
                Class::Protected,
                Why::Protected(ProtectedKind::Root),
            ));
        }

        // システムの領域とその配下。"/var/tmp" とその配下だけは除く。
        for area in SYSTEM_AREAS {
            let area = Path::new(area);
            if path.starts_with(area) {
                let var_tmp = Path::new("/var/tmp");
                if path.starts_with(var_tmp) {
                    continue;
                }
                return Ok(Classification::new(
                    Class::Protected,
                    Why::Protected(ProtectedKind::SystemArea),
                ));
            }
        }

        // ".git" とその配下。
        if path.components().any(|c| c.as_os_str() == ".git") {
            return Ok(Classification::new(
                Class::Protected,
                Why::Protected(ProtectedKind::DotGit),
            ));
        }

        // ほかの利用者のホームとその配下。
        if self.is_other_home(path) {
            return Ok(Classification::new(
                Class::Protected,
                Why::Protected(ProtectedKind::OtherHome),
            ));
        }

        let ephemeral_roots = self.ephemeral_roots();
        // TMPDIR が別の一時ルート配下でも、ルート自体の保護が先に適用される。
        if !children && ephemeral_roots.iter().any(|root| path == root) {
            return Ok(Classification::new(
                Class::Protected,
                Why::Protected(ProtectedKind::EphemeralRoot),
            ));
        }
        if ephemeral_roots.iter().any(|root| path.starts_with(root)) {
            return Ok(Classification::new(Class::Ephemeral, Why::Ephemeral));
        }

        if !children {
            if let Some(home) = &self.env.home {
                if !home.as_os_str().is_empty() && path == home {
                    return Ok(Classification::new(
                        Class::Protected,
                        Why::Protected(ProtectedKind::Home),
                    ));
                }
            }
            if let Some(cwd) = &self.env.cwd {
                if path == cwd {
                    return Ok(Classification::new(
                        Class::Protected,
                        Why::Protected(ProtectedKind::Cwd),
                    ));
                }
            }
        }

        if let Some(root) = self.worktree_root(path) {
            if !children && path == root {
                return Ok(Classification::new(
                    Class::Protected,
                    Why::Protected(ProtectedKind::RepoRoot),
                ));
            }
            return self.git_classify(&root, path, deadline);
        }

        Ok(Classification::new(Class::Unknown, Why::Unmanaged))
    }

    /// 作業ツリーの中のパスを、git status の報告の有無で分ける。
    fn git_classify(
        &self,
        root: &Path,
        path: &Path,
        deadline: Instant,
    ) -> Result<Classification, Failure> {
        if !self.env.git_enabled {
            return Ok(Classification::new(Class::Unknown, Why::Unmanaged));
        }
        match self.git.status_until(root, path, deadline) {
            Ok(report) => {
                let report = report.trim();
                if report.is_empty() {
                    Ok(Classification::new(Class::Vcs, Why::Vcs))
                } else if report.lines().any(|l| l.starts_with("??")) {
                    Ok(Classification::new(Class::Unknown, Why::Untracked))
                } else {
                    Ok(Classification::new(Class::Unknown, Why::Uncommitted))
                }
            }
            Err(GitFailure::Failed(_)) => Ok(Classification::new(Class::Unknown, Why::GitFailed)),
            Err(GitFailure::Limit) => Err(Failure::Limit),
        }
    }

    fn is_other_home(&self, path: &Path) -> bool {
        let under_home = Path::new("/home");
        if path.starts_with(under_home) {
            if let Some(home) = &self.env.home {
                // 空のホームは「無い」として扱う。すべてのパスが空で始まってしまう。
                if !home.as_os_str().is_empty() && (path == home || path.starts_with(home)) {
                    return false;
                }
            }
            return true;
        }
        // "/root" はシステムの領域として先に処理される。
        false
    }
}

/// 設定で追加した保護ルートに当たるか。ルートそれ自体と配下の両方。
pub fn under_root(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}

/// パスの見かけをそろえる（`.` の除去と `..` の巻き上げ）。
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                // ルートより上へは行かない。相対パスの先頭の ".." は残す。
                if !out.pop() && !out.has_root() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}
