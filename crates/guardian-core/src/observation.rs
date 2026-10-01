use std::path::PathBuf;
/// 一度取得した、分類とルート照合で共有するパス。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedPath {
    pub path: PathBuf,
    pub children: bool,
}
