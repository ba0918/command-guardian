//! 正規化した構文木。判定に必要な事実だけを保ち、OSS パーサの型を外へ出さない。
//!
//! 契約（docs/ir/parser.md#REQ-041）が保つもの:
//!
//! - コマンド列の順序と区切り（";"、"&&"、"||"、パイプ）
//! - プログラムと引数の語（順序、引用の状態、語の断片、置換の本体）
//! - リダイレクトの種類（読み、書き、追記、切り詰め、読み書き、複製）と対象の語とファイル記述子
//! - 複合構文（条件、繰り返し、グループ、サブシェル、関数）の本体

/// 1 つのスクリプト。コマンド列を順に持つ。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Script {
    pub items: Vec<Item>,
}

impl Script {
    /// コマンド列が空か。
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// コマンド列の 1 項目。`first` と `rest` が `&&` / `||` でつながり、
/// 項目の終わりが `separator`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub first: Pipeline,
    pub rest: Vec<(AndOrOp, Pipeline)>,
    pub separator: Separator,
}

/// `&&` と `||`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndOrOp {
    And,
    Or,
}

/// 項目の区切り（";"・改行・"&"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Separator {
    Sequence,
    Async,
}

/// `|` でつながったコマンドの並び。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pipeline {
    pub commands: Vec<Command>,
}

/// 1 つのコマンド。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Simple(SimpleCommand),
    /// 複合構文と、そのコマンドに付いたリダイレクト。
    Compound {
        compound: Compound,
        redirects: Vec<Redirect>,
    },
    Function(Function),
    /// `[[ ... ]]`。
    Test(TestCommand),
}

/// 単純コマンド。代入の語も含めて、語が現れた順に並ぶ。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SimpleCommand {
    pub words: Vec<Word>,
    pub redirects: Vec<Redirect>,
    /// `<( ... )` と `>( ... )` の本体。
    pub process_substitutions: Vec<ProcessSubstitution>,
}

/// 関数定義。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: Word,
    pub body: Box<Compound>,
    pub redirects: Vec<Redirect>,
}

/// `[[ ... ]]`。式の中の語を現れた順に持つ。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TestCommand {
    pub words: Vec<Word>,
    pub redirects: Vec<Redirect>,
}

/// 複合構文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Compound {
    /// `if` と `elif` / `else`。
    If {
        condition: Script,
        then: Script,
        elses: Vec<Else>,
    },
    /// `while` と `until`。`until` が true のとき `until`。
    While {
        condition: Script,
        body: Script,
        until: bool,
    },
    /// `for`。
    For {
        var: String,
        values: Vec<Word>,
        body: Script,
    },
    /// `for (( ... ))`。式は読んだ断片として持つ。
    ArithmeticFor {
        initializer: Option<Word>,
        condition: Option<Word>,
        updater: Option<Word>,
        body: Script,
    },
    /// `case`。
    Case { value: Word, arms: Vec<CaseArm> },
    /// `{ ...; }`。
    BraceGroup(Script),
    /// `( ... )`。
    Subshell(Script),
    /// `(( ... ))`。式は読んだ断片として持つ。
    Arithmetic(Word),
    /// `coproc`。
    Coprocess {
        name: Option<Word>,
        body: Box<Command>,
    },
}

/// `if` の `elif` / `else`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Else {
    pub condition: Option<Script>,
    pub body: Script,
}

/// `case` の 1 枝。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseArm {
    pub patterns: Vec<Word>,
    pub body: Option<Script>,
}

/// 1 つの語。引用の状態と、語を構成する断片を持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub parts: Vec<Part>,
    /// 引用を外した見かけの文字列。展開は解かない。
    pub text: String,
    /// 引用されていない glob 文字を含むか。
    pub has_glob: bool,
}

impl Word {
    /// 語の断片から組み立てる。`text` と `has_glob` は断片から決まる。
    pub fn from_parts(parts: Vec<Part>) -> Word {
        let mut text = String::new();
        let mut has_glob = false;
        for part in &parts {
            part.render(&mut text);
            if matches!(part, Part::Glob(_)) {
                has_glob = true;
            }
        }
        Word {
            parts,
            text,
            has_glob,
        }
    }

    /// 展開も glob も無いリテラルか。
    pub fn literal(&self) -> bool {
        !self.has_glob
            && self
                .parts
                .iter()
                .all(|p| matches!(p, Part::Literal(_) | Part::Quoted(_)))
    }

    /// リテラルならその値。
    pub fn literal_value(&self) -> Option<String> {
        if self.literal() {
            Some(self.text.clone())
        } else {
            None
        }
    }
}

/// 語を構成する部分。引用の内側か外側かで展開の意味が変わる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// 引用の外のリテラル。
    Literal(String),
    /// 引用の内のリテラル。
    Quoted(String),
    /// `$NAME` または `${NAME}`。解決は照合時に行う。
    Var(String),
    /// `$(...)` またはバッククォート。本体は既に読んである。
    Substitution(Substitution),
    /// コマンドとして読まない展開。原文の綴りを持つ。
    Opaque(String),
    /// 引用されていない glob 文字。
    Glob(String),
}

impl Part {
    fn render(&self, out: &mut String) {
        match self {
            Part::Literal(s) | Part::Quoted(s) | Part::Opaque(s) | Part::Glob(s) => out.push_str(s),
            Part::Var(name) => {
                out.push('$');
                out.push_str(name);
            }
            Part::Substitution(sub) => {
                out.push_str("$(");
                out.push_str(&sub.body_text);
                out.push(')');
            }
        }
    }
}

/// コマンド置換の本体。読めた本体と、読めなかったときの原文を持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Substitution {
    /// 内側の本文（原文）。
    pub body_text: String,
    /// 読めた本文。読めなかったときは None（原因は失敗の一覧に入る）。
    pub body: Option<Box<Script>>,
}

/// 1 つのリダイレクト。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirect {
    pub kind: RedirectKind,
    /// ファイル記述子（`2>` の 2 など）。
    pub fd: Option<i32>,
    pub target: RedirectTarget,
}

/// リダイレクトの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectKind {
    /// 読み（`<`）。
    Read,
    /// 書き（`>`）。
    Write,
    /// 追記（`>>`）。
    Append,
    /// 読み書き（`<>`）。
    ReadWrite,
    /// 切り詰め（`>|`）。
    Clobber,
    /// 入力の複製（`<&`）。
    DuplicateInput,
    /// 出力の複製（`>&`）。
    DuplicateOutput,
    /// ヒアドキュメント（`<<`、`<<-`）。
    HereDocument,
    /// ヒアストリング（`<<<`）。
    HereString,
    /// 標準出力と標準エラーのまとめ書き（`&>`、`&>>`）。true は追記。
    OutputAndError(bool),
}

/// リダイレクトの対象。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedirectTarget {
    /// 対象の語。
    Word(Word),
    /// ファイル記述子（`2>&1` の 1 など）。
    Fd(i32),
    /// プロセス置換（`< <( ... )` など）。
    ProcessSubstitution(ProcessSubstitution),
    /// ヒアドキュメント。`expand` は本文を展開するか。
    HereDocument { end: Word, doc: Word, expand: bool },
}

/// プロセス置換。`write` は `>` の向き（`>( ... )`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSubstitution {
    pub write: bool,
    pub body: Script,
}
