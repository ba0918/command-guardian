//! 判定を構成する共通の型。設定にも fs にも依存しない。

use std::fmt;
use std::path::PathBuf;

/// 判定の 3 値。順序は重さの順（`Allow` < `Ask` < `Block`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    Allow,
    Ask,
    Block,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Allow => "allow",
            Verdict::Ask => "ask",
            Verdict::Block => "block",
        }
    }

    /// 2 つの判定を最悪値で合成する。
    pub fn worst(self, other: Verdict) -> Verdict {
        self.max(other)
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 破壊的効果の種類。名前はルール名（`rules.disable`）と同じ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Op {
    Delete,
    Truncate,
    Format,
}

impl Op {
    pub fn name(self) -> &'static str {
        match self {
            Op::Delete => "delete",
            Op::Truncate => "truncate",
            Op::Format => "format",
        }
    }
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// 効果の対象。解決の結果をそのまま持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// 値の確定した 1 つのパス。`dereference` は末尾スラッシュ付きの削除。
    Path { path: PathBuf, dereference: bool },
    /// glob を含む対象。分類にはこの base（広がり得る最も外側のディレクトリ）を使う。
    GlobBase(PathBuf),
    /// 対象がコマンド本文に無い効果。base の子を分類する（find / xargs / for）。
    Children(PathBuf),
    /// コマンド自身が mktemp で作ったパス。
    Mktemp,
    /// 供給元も確定できない対象集合（find 以外のパイプ元、非リテラルの for など）。
    UnknownSource,
    /// 解決できないパス。原文を保持する。
    Unresolved(String),
}

/// 取り出した 1 つの破壊的効果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effect {
    pub op: Op,
    pub target: Target,
}

/// パスの 4 分類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Ephemeral,
    Vcs,
    Protected,
    Unknown,
}

impl Class {
    pub fn as_str(self) -> &'static str {
        match self {
            Class::Ephemeral => "ephemeral",
            Class::Vcs => "vcs",
            Class::Protected => "protected",
            Class::Unknown => "unknown",
        }
    }
}

impl fmt::Display for Class {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 分類の内訳。文面が「なぜ」を組み立てるのに使う。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// 一時領域の中。
    Ephemeral,
    /// コマンド自身が mktemp で作ったパス。
    Mktemp,
    /// git の作業ツリーの中で、報告が無い。
    Vcs,
    /// 作業ツリーの中だが未追跡のものがある。
    Untracked,
    /// 作業ツリーの中だが未コミットの変更がある。
    Uncommitted,
    /// どの規則にも当たらない。
    Unmanaged,
    /// 保護領域。種類を持つ。
    Protected(ProtectedKind),
    /// パスを解決できない。
    Unresolved(String),
    /// 供給元が確定できない対象集合。
    UnknownSource,
    /// git の起動に失敗した。
    GitFailed,
}

/// 保護領域の種類。文面と分類の理由に使う。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtectedKind {
    /// "/" それ自体。
    Root,
    /// システムの領域とその配下。
    SystemArea,
    /// 一時領域のルートそれ自体。
    EphemeralRoot,
    /// ホームディレクトリそれ自体。
    Home,
    /// 作業ディレクトリそれ自体。
    Cwd,
    /// リポジトリのルートそれ自体。
    RepoRoot,
    /// ".git" とその配下。
    DotGit,
    /// ほかの利用者のホームとその配下。
    OtherHome,
    /// 設定で追加した保護ルートとその配下。
    ConfiguredRoot,
}
