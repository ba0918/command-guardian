//! マージ後の設定値。
use crate::guard::GuardRule;
use guardian_core::Verdict;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomRule {
    pub name: String,
    pub pattern: String,
    pub verdict: Verdict,
}

pub(crate) fn normalize_root(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if out.file_name().is_some_and(|name| name != "..") {
                    out.pop();
                } else if !out.has_root() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub allowed_roots: Vec<PathBuf>,
    pub protected_roots: Vec<PathBuf>,
    pub unknown_verdict: Verdict,
    pub rules_disable: Vec<String>,
    pub rules_custom: Vec<CustomRule>,
    pub guard: Vec<GuardRule>,
    pub git_enabled: bool,
    pub enforce: bool,
    pub trusted_projects: Vec<PathBuf>,
}

impl Config {
    pub fn builtin(tmpdir: Option<&Path>) -> Config {
        let mut allowed = vec![PathBuf::from("/tmp"), PathBuf::from("/var/tmp")];
        if let Some(t) = tmpdir.filter(|t| !t.as_os_str().is_empty()) {
            if !allowed.contains(&t.to_path_buf()) {
                allowed.push(t.to_path_buf());
            }
        }
        Config {
            allowed_roots: allowed,
            protected_roots: Vec::new(),
            unknown_verdict: Verdict::Ask,
            rules_disable: Vec::new(),
            rules_custom: Vec::new(),
            guard: Vec::new(),
            git_enabled: true,
            enforce: true,
            trusted_projects: Vec::new(),
        }
    }
}
