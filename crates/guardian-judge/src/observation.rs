use guardian_core::{ObservedPath, Target};
use std::path::{Path, PathBuf};

pub trait PathObserver: Send + Sync {
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf>;
}
pub struct SystemPaths;
impl PathObserver for SystemPaths {
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}
pub fn observe(target: &Target, paths: &dyn PathObserver) -> Option<ObservedPath> {
    let (path, dereference, children) = match target {
        Target::Path { path, dereference } => (path, *dereference, false),
        Target::Children { base, dereference } => (base, *dereference, true),
        Target::GlobBase(base) => (base, false, false),
        _ => return None,
    };
    Some(ObservedPath {
        path: if dereference {
            paths.canonicalize(path).unwrap_or_else(|_| path.clone())
        } else {
            path.clone()
        },
        children,
    })
}
