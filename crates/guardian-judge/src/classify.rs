//! パスの分類。fs を見て 4 分類する。設定は読まない。

use guardian_core::{Class, ProtectedKind, Why};
use std::path::{Component, Path, PathBuf};

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
}

/// 配下のすべてに当てるシステムの領域。
const SYSTEM_AREAS: &[&str] = &[
    "/etc", "/usr", "/bin", "/sbin", "/lib", "/lib64", "/boot", "/dev", "/proc", "/sys", "/run",
    "/opt", "/srv", "/root", "/var",
];

impl Judge {
    pub fn new(env: JudgeEnv) -> Judge {
        Judge { env }
    }

    /// 1 つのパスを分類する。`dereference` は末尾スラッシュ付きの削除。
    pub fn classify_path(&self, path: &Path, dereference: bool) -> Classification {
        let path = if dereference {
            std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        } else {
            path.to_path_buf()
        };
        self.classify_inner(&path, false)
    }

    /// 供給元の子（find・xargs・for の対象集合）を分類する。
    pub fn classify_children(&self, base: &Path) -> Classification {
        self.classify_inner(base, true)
    }

    /// パスを含む git の作業ツリーのルート。`.git` の上方探索だけで決め、git は起動しない。
    pub fn worktree_root(&self, path: &Path) -> Option<PathBuf> {
        let start = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent().map(|p| p.to_path_buf())?
        };
        for dir in start.ancestors() {
            if dir.join(".git").symlink_metadata().is_ok() {
                return Some(dir.to_path_buf());
            }
        }
        None
    }

    fn ephemeral_roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![PathBuf::from("/tmp"), PathBuf::from("/var/tmp")];
        if let Some(t) = &self.env.tmpdir {
            if !roots.contains(t) {
                roots.push(t.clone());
            }
        }
        roots
    }

    fn classify_inner(&self, path: &Path, children: bool) -> Classification {
        // "/" それ自体、およびその子（children のとき）は保護。
        if path == Path::new("/") {
            return Classification::new(Class::Protected, Why::Protected(ProtectedKind::Root));
        }

        // システムの領域とその配下。"/var/tmp" とその配下だけは除く。
        for area in SYSTEM_AREAS {
            let area = Path::new(area);
            if path.starts_with(area) {
                let var_tmp = Path::new("/var/tmp");
                if path.starts_with(var_tmp) {
                    continue;
                }
                return Classification::new(
                    Class::Protected,
                    Why::Protected(ProtectedKind::SystemArea),
                );
            }
        }

        // ".git" とその配下。
        if path.components().any(|c| c.as_os_str() == ".git") {
            return Classification::new(Class::Protected, Why::Protected(ProtectedKind::DotGit));
        }

        // ほかの利用者のホームとその配下。
        if self.is_other_home(path) {
            return Classification::new(Class::Protected, Why::Protected(ProtectedKind::OtherHome));
        }

        // 一時領域。
        for root in self.ephemeral_roots() {
            if path == root {
                if children {
                    return Classification::new(Class::Ephemeral, Why::Ephemeral);
                }
                return Classification::new(
                    Class::Protected,
                    Why::Protected(ProtectedKind::EphemeralRoot),
                );
            }
            if path.starts_with(&root) {
                return Classification::new(Class::Ephemeral, Why::Ephemeral);
            }
        }

        if !children {
            if let Some(home) = &self.env.home {
                if path == home {
                    return Classification::new(
                        Class::Protected,
                        Why::Protected(ProtectedKind::Home),
                    );
                }
            }
            if let Some(cwd) = &self.env.cwd {
                if path == cwd {
                    return Classification::new(
                        Class::Protected,
                        Why::Protected(ProtectedKind::Cwd),
                    );
                }
            }
        }

        if let Some(root) = self.worktree_root(path) {
            if !children && path == root {
                return Classification::new(
                    Class::Protected,
                    Why::Protected(ProtectedKind::RepoRoot),
                );
            }
            // 作業ツリーの中の分類は git status で決める（git が無効なら unknown のまま）。
            return Classification::new(Class::Unknown, Why::Unmanaged);
        }

        Classification::new(Class::Unknown, Why::Unmanaged)
    }

    fn is_other_home(&self, path: &Path) -> bool {
        let under_home = Path::new("/home");
        if path.starts_with(under_home) {
            if let Some(home) = &self.env.home {
                if path == home || path.starts_with(home) {
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
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}
