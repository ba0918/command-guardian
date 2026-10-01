use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct Env {
    pub home: Option<PathBuf>,
    pub tmpdir: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
}

/// 構文解析と隔離された実行の共通診断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    TooLarge,
    TooDeep,
    Syntax,
    UnknownNode(String),
    Panic,
    Internal,
    Limit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    Parse(Failure),
    UnreadableProgram(String),
    UnreadableShellBody(String),
    UnreadableEval,
    UnsupportedShell(String),
}

impl Ask {
    pub fn is_limit(&self) -> bool {
        matches!(
            self,
            Ask::Parse(Failure::TooLarge | Failure::TooDeep | Failure::Limit)
        )
    }
}
