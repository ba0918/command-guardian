//! コマンド文字列の字句解析と構文解析。成熟した OSS のシェルパーサ
//! （brush-parser、版を固定）を背後に隠し、正規化した構文木だけを公開する
//! （REQ-036）。
//!
//! - 解析は専用の大きなスタックのスレッドで行い、panic は ask の原因に変える
//!   （REQ-038・REQ-039）。
//! - サイズと深さは字句解析の前に測り、置換の再帰読みは累計で測る（REQ-039）。
//! - 解析に失敗した命令からは効果を返さない。読めている命令だけを構文木に残す
//!   （REQ-038）。

pub mod ast;
mod normalize;
mod words;

pub use ast::*;
pub use words::{
    basename, shell_c_index, shell_invocation, shell_kind, split_assignment,
    split_prefix_assignments, strip_wrapper, ShellInvocation, ShellKind,
};

use brush_parser::{ParserOptions, Token};
use normalize::{options, parse_program, parse_tokens, tokenize, unquoted_word, Normalizer};

/// コマンド文字列の上限（REQ-039）。置換の再帰読みでは累計で測る。
pub const LIMIT_BYTES: usize = 1024 * 1024;

/// 構文の入れ子と置換の再帰の上限（REQ-039）。
pub const LIMIT_DEPTH: usize = 128;

/// 解析に使うスレッドのスタック。深い入れ子でもプロセスを落とさないため。
const WORKER_STACK_BYTES: usize = 32 * 1024 * 1024;

/// 読み直しの試行の上限。壊れた入力を読み続けないための歯止め。
const MAX_RECOVERY_ATTEMPTS: usize = 64;

/// 解析の失敗の種類（REQ-038）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// 入力が上限（1 MiB）を超えた。
    TooLarge,
    /// 入れ子が上限（128 段）を超えた。
    TooDeep,
    /// 構文解析に失敗した。
    Syntax,
    /// 知らない形のノードに出会った。
    UnknownNode(String),
    /// 構文解析が panic した。
    Panic,
}

/// 解析の結果。読めた構文木と、読めなかった理由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub script: Script,
    pub failures: Vec<Failure>,
}

impl Outcome {
    fn failure(failure: Failure) -> Outcome {
        Outcome {
            script: Script::default(),
            failures: vec![failure],
        }
    }
}

/// コマンド文字列を構文解析し、正規化した構文木と読めなかった理由を返す。
pub fn parse(input: &str) -> Outcome {
    in_parser_thread(|| parse_inner(input, parse_program))
        .unwrap_or_else(|| Outcome::failure(Failure::Panic))
}

/// 引用を外したコマンド本文。設定の照合（見張りの例の分割とカスタムルールの
/// 本文の引用の除去）で使う（REQ-036）。
pub fn strip_quotes(input: &str) -> String {
    in_parser_thread(|| strip_quotes_inner(input)).unwrap_or_default()
}

/// 差し込み可能な生の構文解析。panic を境界で Failure に変える（REQ-038）。
type ProgramParser = fn(&str, &ParserOptions) -> Result<brush_parser::ast::Program, Failure>;

fn parse_inner(input: &str, parser: ProgramParser) -> Outcome {
    // サイズは構文解析の前に測る（REQ-039）。
    if input.len() > LIMIT_BYTES {
        return Outcome::failure(Failure::TooLarge);
    }
    // 深さも字句解析の前に測る。OSS パーサの再帰でプロセスを落とさないため。
    if depth_over_limit(input) {
        return Outcome::failure(Failure::TooDeep);
    }
    let options = options();
    let mut normalizer = Normalizer::new(LIMIT_BYTES - input.len());
    let parsed = match catch(|| parser(input, &options)) {
        Ok(parsed) => parsed,
        Err(()) => return Outcome::failure(Failure::Panic),
    };
    match parsed {
        Ok(program) => {
            let script = normalizer.program(&program, 0);
            Outcome {
                script,
                failures: normalizer.failures,
            }
        }
        Err(Failure::Panic) => Outcome::failure(Failure::Panic),
        Err(_) => {
            // 解析に失敗した命令からは効果を返さない。読めている命令だけを拾う
            // （REQ-038・A20）。
            let mut failures = Vec::new();
            let script = recover(input, &options, &mut normalizer, &mut failures);
            failures.extend(normalizer.failures);
            Outcome { script, failures }
        }
    }
}

/// 解析に失敗した入力を、トップレベルの区切りで分けて読めるところだけ拾う。
fn recover(
    input: &str,
    options: &ParserOptions,
    normalizer: &mut Normalizer,
    failures: &mut Vec<Failure>,
) -> Script {
    let mut script = Script::default();
    let tokens = match tokenize(input, options) {
        Ok(tokens) => tokens,
        Err(failure) => {
            failures.push(failure);
            return script;
        }
    };
    let mut rest = tokens.as_slice();
    let mut attempts = 0usize;
    while !rest.is_empty() {
        match parse_tokens(rest, options) {
            Ok(program) => {
                script.items.extend(normalizer.program(&program, 0).items);
                return script;
            }
            Err(Failure::Panic) => {
                failures.push(Failure::Panic);
                return script;
            }
            Err(_) => {}
        }
        let candidates = top_level_separators(rest);
        let mut split = false;
        for &index in candidates.iter().rev() {
            if index == 0 {
                continue;
            }
            attempts += 1;
            if attempts > MAX_RECOVERY_ATTEMPTS {
                failures.push(Failure::Syntax);
                return script;
            }
            match parse_tokens(&rest[..index], options) {
                Ok(program) => {
                    script.items.extend(normalizer.program(&program, 0).items);
                    rest = &rest[index + 1..];
                    split = true;
                    break;
                }
                Err(Failure::Panic) => {
                    failures.push(Failure::Panic);
                    return script;
                }
                Err(_) => {}
            }
        }
        if !split {
            failures.push(Failure::Syntax);
            return script;
        }
    }
    script
}

/// 入れ子の外側にある区切りの位置。パイプは 1 つのまとまりとして扱う。
fn top_level_separators(tokens: &[Token]) -> Vec<usize> {
    let mut depth = 0usize;
    let mut out = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if let Token::Operator(operator, _) = token {
            match operator.as_str() {
                "(" | "{" => depth += 1,
                ")" | "}" => depth = depth.saturating_sub(1),
                ";" | "&&" | "||" | "&" | "\n" if depth == 0 => out.push(index),
                _ => {}
            }
        }
    }
    out
}

/// 引用を外したコマンド本文。
fn strip_quotes_inner(input: &str) -> String {
    let options = options();
    let tokens = match tokenize(input, &options) {
        Ok(tokens) => tokens,
        Err(_) => return String::new(),
    };
    let mut out: Vec<String> = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        match &tokens[index] {
            // ヒアドキュメントは区切り語までを残し、本文は落とす。
            Token::Operator(operator, _) if operator == "<<" || operator == "<<-" => {
                out.push(operator.clone());
                index += 1;
                if let Some(Token::Word(delimiter, _)) = tokens.get(index) {
                    out.push(unquoted_word(delimiter, &options));
                    index += 1;
                }
                if matches!(tokens.get(index), Some(Token::Word(_, _))) {
                    index += 1;
                }
                if let Some(Token::Word(_, span)) = tokens.get(index) {
                    if span.start.index == span.end.index {
                        index += 1;
                    }
                }
            }
            Token::Word(word, _) => {
                out.push(unquoted_word(word, &options));
                index += 1;
            }
            Token::Operator(operator, _) => {
                out.push(operator.clone());
                index += 1;
            }
        }
    }
    out.join(" ")
}

/// panic を捕まえて Err に変える境界（REQ-038）。
pub(crate) fn catch<T>(f: impl FnOnce() -> T) -> Result<T, ()> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|_| ())
}

/// 解析を専用のスタックを持つスレッドで行う。スレッドを作れないときは None。
fn in_parser_thread<T: Send>(f: impl FnOnce() -> T + Send) -> Option<T> {
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("guardian-parser".to_string())
            .stack_size(WORKER_STACK_BYTES)
            .spawn_scoped(scope, f)
            .ok()?;
        handle.join().ok()
    })
}

/// 字句解析の前に、構文と置換の入れ子の深さの上限を測る（REQ-039）。
///
/// 引用とエスケープは正確に読み、`#`・here-doc・`case` のように解釈の揺れる
/// 位置に出会ったら、そこから先は「開きだけを数える」側へ切り替える。
/// 少なく見積もると OSS パーサの再帰でプロセスが落ちる（スタックオーバー
/// フローは捕まえられない）が、多く見積もっても ask に倒れるだけで済む。
fn depth_over_limit(input: &str) -> bool {
    const NORMAL: u8 = 0;
    const DOUBLE: u8 = 1;
    const SINGLE: u8 = 2;
    const BACKTICK: u8 = 3;
    const ANSI_C: u8 = 4;

    /// 開いている入れ子の種類。
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Nest {
        /// `( ... )` のサブシェル。
        Subshell,
        /// `$( ... )` のコマンド置換。
        Substitution,
        /// `(( ... ))` の算術。
        Arithmetic,
        /// `$[ ... ]` の算術。
        ArithmeticBracket,
        /// `${ ... }` のパラメータ展開。
        Parameter,
        /// `{ ...; }` のグループ。
        Group,
        /// `{a,b}` の語。
        WordBrace,
        /// `[[ ... ]]`。
        Test,
        /// `<( ... )`・`>( ... )` と拡張 glob。
        Process,
        /// バッククォートの本体。
        Backquote,
        /// スクリプト全体。
        Root,
    }

    impl Nest {
        /// この入れ子の中の `#` がコメントになり得るか。
        fn allows_comment(self) -> bool {
            matches!(
                self,
                Nest::Root
                    | Nest::Subshell
                    | Nest::Substitution
                    | Nest::Group
                    | Nest::Process
                    | Nest::Backquote
            )
        }

        /// `)` が閉じる種類か。
        fn closed_by_paren(self) -> bool {
            matches!(
                self,
                Nest::Subshell | Nest::Substitution | Nest::Arithmetic | Nest::Process
            )
        }

        /// この入れ子を閉じた後ろにコマンドが続くか。語の一部
        /// （`$(...)`・`${...}`・`{a,b}`・`<( ... )`）では続かない。
        fn command_follows(self) -> bool {
            matches!(
                self,
                Nest::Subshell | Nest::Arithmetic | Nest::Group | Nest::Test
            )
        }
    }

    /// 深さを増やす予約語。
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Keyword {
        If,
        Loop,
        /// `case`。`patterns` は枝の位置（`in` の後ろ）か。`mark` は枝を
        /// 開いたときの入れ子の数で、枝の中の `)` が外を閉じない印。
        Case {
            patterns: bool,
            mark: usize,
        },
    }

    /// `<<` の後ろの 1 つの here-doc。
    struct HereDoc {
        delimiter: String,
        strip_tabs: bool,
        expand: bool,
    }

    /// `<<` の後ろの区切りの語を読む（区切り、タブを除くか、展開するか）。
    fn here_doc_delimiter(chars: &[char], index: &mut usize) -> Option<(String, bool, bool)> {
        let mut i = *index;
        let mut strip_tabs = false;
        if chars.get(i) == Some(&'-') {
            strip_tabs = true;
            i += 1;
        }
        while matches!(chars.get(i), Some(' ' | '\t')) {
            i += 1;
        }
        let mut delimiter = String::new();
        let mut quoted = false;
        let mut any = false;
        while i < chars.len() {
            match chars[i] {
                ' ' | '\t' | '\n' | ';' | '&' | '|' | '(' | ')' | '<' | '>' => break,
                '\\' => {
                    quoted = true;
                    i += 1;
                    if let Some(&next) = chars.get(i) {
                        delimiter.push(next);
                        i += 1;
                        any = true;
                    }
                }
                '\'' => {
                    quoted = true;
                    i += 1;
                    while i < chars.len() && chars[i] != '\'' {
                        delimiter.push(chars[i]);
                        i += 1;
                    }
                    i += 1;
                    any = true;
                }
                '"' => {
                    quoted = true;
                    i += 1;
                    while i < chars.len() && chars[i] != '"' {
                        if chars[i] == '\\' && i + 1 < chars.len() {
                            i += 1;
                        }
                        delimiter.push(chars[i]);
                        i += 1;
                    }
                    i += 1;
                    any = true;
                }
                c => {
                    delimiter.push(c);
                    i += 1;
                    any = true;
                }
            }
        }
        if !any {
            return None;
        }
        *index = i;
        Some((delimiter, strip_tabs, !quoted))
    }

    /// here-doc の本体を区切りの行まで読み飛ばし、本体の中の展開の開きの
    /// 数の最大を返す（本体では `$(`・`${`・`$[`・バッククォートだけが効く）。
    fn skip_here_docs(chars: &[char], index: &mut usize, docs: &mut Vec<HereDoc>) -> usize {
        let mut i = *index;
        let mut max = 0usize;
        for doc in docs.drain(..) {
            let mut count = 0usize;
            loop {
                let start = i;
                while i < chars.len() && chars[i] != '\n' {
                    if doc.expand
                        && (chars[i] == '`'
                            || (chars[i] == '$'
                                && matches!(chars.get(i + 1), Some('(' | '{' | '['))))
                    {
                        count += 1;
                    }
                    i += 1;
                }
                let end = i;
                let mut content = start;
                if doc.strip_tabs {
                    while content < end && chars[content] == '\t' {
                        content += 1;
                    }
                }
                let line: String = chars[content..end].iter().collect();
                if line == doc.delimiter {
                    if i < chars.len() {
                        i += 1;
                    }
                    break;
                }
                if i >= chars.len() {
                    break;
                }
                i += 1;
            }
            max = max.max(count);
        }
        *index = i;
        max
    }

    struct Frame {
        nest: Nest,
        /// この入れ子の中で開いた予約語。
        keywords: Vec<Keyword>,
        /// 二重引用の中から開いたときは、閉じるときに引用へ戻る。
        restore_quote: bool,
    }

    fn opener(word: &str) -> Option<Keyword> {
        match word {
            "if" => Some(Keyword::If),
            "while" | "until" | "for" | "select" => Some(Keyword::Loop),
            _ => None,
        }
    }

    fn closer(word: &str) -> Option<Keyword> {
        match word {
            "fi" => Some(Keyword::If),
            "done" => Some(Keyword::Loop),
            _ => None,
        }
    }

    let chars: Vec<char> = input.chars().collect();
    let mut modes: Vec<u8> = vec![NORMAL];
    let mut frames: Vec<Frame> = vec![Frame {
        nest: Nest::Root,
        keywords: Vec::new(),
        restore_quote: false,
    }];
    let mut keywords = 0usize;
    let mut peak = 0usize;
    let mut at_command_start = true;
    // 予約語の閉じは、`;` か改行の直後にだけ閉じとして認められる。
    let mut after_separator = true;
    let mut last: Option<char> = None;
    let mut last_non_ws: Option<char> = None;
    // 行の終わりから始まる here-doc の本体。
    let mut here_docs: Vec<HereDoc> = Vec::new();
    // バッククォートの本体で開いたものを、閉じるときに捨てるための印。
    let mut backquote_frames: Vec<usize> = Vec::new();
    let mut backquote_keywords: Vec<usize> = Vec::new();
    let mut index = 0;

    macro_rules! depth {
        () => {
            (frames.len() - 1) + keywords
        };
    }
    macro_rules! bump {
        () => {
            if depth!() > peak {
                peak = depth!();
            }
            if peak > LIMIT_DEPTH {
                return true;
            }
        };
    }
    macro_rules! pop_frame {
        () => {{
            let frame = frames.pop().unwrap();
            keywords -= frame.keywords.len();
            if frame.restore_quote {
                modes.pop();
            }
            frame.nest
        }};
    }
    /// ここから先は開きだけを数える。解釈の揺れる字面に出会ったときの逃げ道。
    macro_rules! count_openers_only {
        () => {{
            let mut count = depth!();
            while index < chars.len() {
                let c = chars[index];
                match c {
                    '(' | '{' | '`' => {
                        count += 1;
                        index += 1;
                    }
                    '$' if chars.get(index + 1) == Some(&'[') => {
                        count += 1;
                        index += 2;
                    }
                    _ if is_word_char(c) => {
                        let word = word_at(&chars, &mut index);
                        if opener(&word).is_some() {
                            count += 1;
                        }
                    }
                    _ => index += 1,
                }
                if count > LIMIT_DEPTH {
                    return true;
                }
            }
            return false;
        }};
    }

    while index < chars.len() {
        let c = chars[index];
        let prev = last;
        let prev_non_ws = last_non_ws;
        let prev_separator = after_separator;
        last = Some(c);
        if !c.is_whitespace() {
            last_non_ws = Some(c);
            after_separator = false;
        }
        match *modes.last().unwrap_or(&NORMAL) {
            SINGLE => {
                if c == '\'' {
                    modes.pop();
                    at_command_start = false;
                }
                index += 1;
            }
            ANSI_C => match c {
                '\\' => index += 2,
                '\'' => {
                    modes.pop();
                    index += 1;
                    at_command_start = false;
                }
                _ => index += 1,
            },
            BACKTICK => match c {
                '\\' => index += 2,
                '`' => {
                    modes.pop();
                    frames.truncate(backquote_frames.pop().unwrap_or(0));
                    if let Some(saved) = backquote_keywords.pop() {
                        keywords = saved;
                    }
                    index += 1;
                    at_command_start = false;
                }
                '(' | '{' => {
                    frames.push(Frame {
                        nest: Nest::Substitution,
                        keywords: Vec::new(),
                        restore_quote: false,
                    });
                    index += 1;
                    bump!();
                }
                '[' if prev == Some('$') => {
                    frames.push(Frame {
                        nest: Nest::ArithmeticBracket,
                        keywords: Vec::new(),
                        restore_quote: false,
                    });
                    index += 1;
                    bump!();
                }
                _ if is_word_char(c) => {
                    let word = word_at(&chars, &mut index);
                    last = chars.get(index - 1).copied();
                    if opener(&word).is_some() {
                        keywords += 1;
                        bump!();
                    }
                }
                _ => index += 1,
            },
            DOUBLE => match c {
                '\\' => index += 2,
                '"' => {
                    modes.pop();
                    index += 1;
                    at_command_start = false;
                }
                '(' if prev == Some('$') => {
                    frames.push(Frame {
                        nest: Nest::Substitution,
                        keywords: Vec::new(),
                        restore_quote: true,
                    });
                    modes.push(NORMAL);
                    index += 1;
                    bump!();
                }
                '{' if prev == Some('$') => {
                    frames.push(Frame {
                        nest: Nest::Parameter,
                        keywords: Vec::new(),
                        restore_quote: true,
                    });
                    modes.push(NORMAL);
                    index += 1;
                    bump!();
                }
                '[' if prev == Some('$') => {
                    frames.push(Frame {
                        nest: Nest::ArithmeticBracket,
                        keywords: Vec::new(),
                        restore_quote: true,
                    });
                    modes.push(NORMAL);
                    index += 1;
                    bump!();
                }
                '`' => {
                    frames.push(Frame {
                        nest: Nest::Backquote,
                        keywords: Vec::new(),
                        restore_quote: false,
                    });
                    backquote_frames.push(frames.len() - 1);
                    backquote_keywords.push(keywords);
                    modes.push(BACKTICK);
                    index += 1;
                    at_command_start = false;
                    bump!();
                }
                _ => index += 1,
            },
            _ => match c {
                '\\' => {
                    index += 2;
                    at_command_start = false;
                }
                '\'' => {
                    // `$'...'` の中だけ `\` が次の字を escape する。
                    modes.push(if prev == Some('$') { ANSI_C } else { SINGLE });
                    index += 1;
                    at_command_start = false;
                }
                '"' => {
                    modes.push(DOUBLE);
                    index += 1;
                    at_command_start = false;
                }
                '`' => {
                    frames.push(Frame {
                        nest: Nest::Backquote,
                        keywords: Vec::new(),
                        restore_quote: false,
                    });
                    backquote_frames.push(frames.len() - 1);
                    backquote_keywords.push(keywords);
                    modes.push(BACKTICK);
                    index += 1;
                    at_command_start = false;
                    bump!();
                }
                '#' => {
                    // 語の途中の `#` はコメントにならない。引用の外で語の
                    // 切れ目にだけ、確かめられる位置のコメントを読み飛ばす。
                    let candidate = match prev {
                        None => true,
                        Some(p) if p.is_whitespace() => {
                            !matches!(prev_non_ws, Some('<' | '>' | '&' | '|'))
                        }
                        Some(';') => true,
                        _ => false,
                    };
                    let code = match prev {
                        Some(p) if is_word_char(p) || p == '$' || p == '\'' || p == '"' => true,
                        _ => matches!(
                            frames.last().map(|f| f.nest),
                            Some(Nest::Parameter | Nest::Arithmetic | Nest::ArithmeticBracket)
                        ),
                    };
                    if code {
                        index += 1;
                        at_command_start = false;
                    } else if candidate && frames.last().is_some_and(|f| f.nest.allows_comment()) {
                        while index < chars.len() && chars[index] != '\n' {
                            index += 1;
                        }
                        at_command_start = true;
                    } else {
                        count_openers_only!();
                    }
                }
                '(' => {
                    let nest = if prev == Some('$') {
                        Nest::Substitution
                    } else if prev == Some('(') || chars.get(index + 1) == Some(&'(') {
                        Nest::Arithmetic
                    } else if matches!(prev, Some('<' | '>' | '!' | '@' | '?' | '*' | '+')) {
                        Nest::Process
                    } else {
                        Nest::Subshell
                    };
                    frames.push(Frame {
                        nest,
                        keywords: Vec::new(),
                        restore_quote: false,
                    });
                    index += 1;
                    at_command_start = true;
                    bump!();
                }
                ')' => {
                    // `case` の枝を閉じる `)` は、枝の外の入れ子を閉じない。
                    let case_pattern = frames.last().is_some_and(|f| {
                        f.keywords.iter().rev().any(|k| {
                            matches!(k, Keyword::Case { patterns: true, mark } if frames.len() <= *mark)
                        })
                    });
                    if case_pattern {
                        index += 1;
                    } else if frames.last().is_some_and(|f| f.nest.closed_by_paren()) {
                        at_command_start = pop_frame!().command_follows();
                        index += 1;
                        bump!();
                    } else {
                        // 対になる開きが無い `)` は、こちらの読み方が
                        // ずれている印。ここから開きだけを数える。
                        count_openers_only!();
                    }
                }
                '{' => {
                    let nest = if prev == Some('$') {
                        Nest::Parameter
                    } else if chars.get(index + 1).is_some_and(|c| c.is_whitespace()) {
                        Nest::Group
                    } else {
                        Nest::WordBrace
                    };
                    frames.push(Frame {
                        nest,
                        keywords: Vec::new(),
                        restore_quote: false,
                    });
                    index += 1;
                    at_command_start = nest == Nest::Group;
                    bump!();
                }
                '}' => {
                    if frames.last().is_some_and(|f| {
                        matches!(f.nest, Nest::Group | Nest::WordBrace | Nest::Parameter)
                    }) {
                        at_command_start = pop_frame!().command_follows();
                    } else {
                        count_openers_only!();
                    }
                    index += 1;
                    bump!();
                }
                '[' if prev == Some('$') => {
                    frames.push(Frame {
                        nest: Nest::ArithmeticBracket,
                        keywords: Vec::new(),
                        restore_quote: false,
                    });
                    index += 1;
                    at_command_start = false;
                    bump!();
                }
                '[' if chars.get(index + 1) == Some(&'[') => {
                    frames.push(Frame {
                        nest: Nest::Test,
                        keywords: Vec::new(),
                        restore_quote: false,
                    });
                    index += 2;
                    at_command_start = true;
                    bump!();
                }
                '[' => {
                    index += 1;
                    at_command_start = false;
                }
                ']' if chars.get(index + 1) == Some(&']')
                    && frames.last().is_some_and(|f| f.nest == Nest::Test) =>
                {
                    let _ = pop_frame!();
                    index += 2;
                    at_command_start = true;
                    bump!();
                }
                ']' => {
                    if frames
                        .last()
                        .is_some_and(|f| f.nest == Nest::ArithmeticBracket)
                    {
                        let _ = pop_frame!();
                    }
                    at_command_start = false;
                    index += 1;
                    bump!();
                }
                '<' => {
                    if chars.get(index + 1) == Some(&'<') {
                        if chars.get(index + 2) == Some(&'<') {
                            // here-string（`<<<`）。
                            index += 3;
                        } else if frames.last().is_some_and(|f| f.nest == Nest::Arithmetic) {
                            // 算術の中の左シフト。
                            index += 2;
                        } else {
                            let mut cursor = index + 2;
                            match here_doc_delimiter(&chars, &mut cursor) {
                                Some((delimiter, strip_tabs, expand)) => {
                                    here_docs.push(HereDoc {
                                        delimiter,
                                        strip_tabs,
                                        expand,
                                    });
                                    index = cursor;
                                }
                                None => count_openers_only!(),
                            }
                        }
                    } else {
                        index += 1;
                    }
                    at_command_start = false;
                }
                '>' => {
                    index += 1;
                    if chars.get(index) == Some(&'>') {
                        index += 1;
                    }
                    at_command_start = false;
                }
                '$' => {
                    index += 1;
                    at_command_start = false;
                }
                ';' | '\n' => {
                    index += 1;
                    at_command_start = true;
                    after_separator = true;
                    if c == '\n' && !here_docs.is_empty() {
                        let base = depth!();
                        let count = skip_here_docs(&chars, &mut index, &mut here_docs);
                        if base + count > LIMIT_DEPTH {
                            return true;
                        }
                        last = Some('\n');
                        last_non_ws = None;
                    }
                }
                '&' => {
                    index += 1;
                    if chars.get(index) == Some(&'&') {
                        index += 1;
                    }
                    at_command_start = true;
                }
                '|' => {
                    index += 1;
                    if chars.get(index) == Some(&'|') {
                        index += 1;
                    }
                    at_command_start = true;
                }
                '!' => {
                    index += 1;
                    at_command_start = true;
                }
                _ if is_word_char(c) => {
                    let word = word_at(&chars, &mut index);
                    last = chars.get(index - 1).copied();
                    let command_like = frames.last().is_some_and(|f| f.nest.allows_comment());
                    let in_case_value = command_like
                        && word == "in"
                        && frames.last().is_some_and(|f| {
                            matches!(
                                f.keywords.last(),
                                Some(Keyword::Case {
                                    patterns: false,
                                    ..
                                })
                            )
                        });
                    if in_case_value {
                        // `case` の値の後ろの `in` から枝が始まる。
                        let len = frames.len();
                        if let Some(Keyword::Case { patterns, mark }) =
                            frames.last_mut().unwrap().keywords.last_mut()
                        {
                            *patterns = true;
                            *mark = len;
                        }
                        at_command_start = false;
                    } else if command_like && at_command_start {
                        match word.as_str() {
                            "if" => {
                                frames.last_mut().unwrap().keywords.push(Keyword::If);
                                keywords += 1;
                                at_command_start = true;
                                bump!();
                            }
                            "while" | "until" | "for" | "select" => {
                                frames.last_mut().unwrap().keywords.push(Keyword::Loop);
                                keywords += 1;
                                at_command_start = !matches!(word.as_str(), "for" | "select");
                                bump!();
                            }
                            "fi" | "done" => {
                                let want = closer(&word).unwrap();
                                if !prev_separator {
                                    // 区切りの直後にだけ、閉じの予約語は
                                    // 閉じとして認められる。
                                    count_openers_only!();
                                }
                                let frame = frames.last_mut().unwrap();
                                if frame.keywords.last() == Some(&want) {
                                    frame.keywords.pop();
                                    keywords -= 1;
                                    at_command_start = true;
                                    bump!();
                                } else {
                                    // 対になる開きが無い予約語は読み方のずれ。
                                    count_openers_only!();
                                }
                            }
                            "case" => {
                                let mark = frames.len();
                                frames.last_mut().unwrap().keywords.push(Keyword::Case {
                                    patterns: false,
                                    mark,
                                });
                                keywords += 1;
                                at_command_start = false;
                                bump!();
                            }
                            "esac" => {
                                if !prev_separator {
                                    count_openers_only!();
                                }
                                let frame = frames.last_mut().unwrap();
                                if matches!(frame.keywords.last(), Some(Keyword::Case { .. })) {
                                    frame.keywords.pop();
                                    keywords -= 1;
                                    at_command_start = true;
                                    bump!();
                                } else {
                                    // 対になる `case` が無い。
                                    count_openers_only!();
                                }
                            }
                            "then" | "do" | "else" | "elif" => {
                                at_command_start = true;
                            }
                            _ => {
                                at_command_start = false;
                            }
                        }
                    } else {
                        at_command_start = false;
                    }
                }
                _ => {
                    if !c.is_whitespace() {
                        at_command_start = false;
                    }
                    index += 1;
                }
            },
        }
    }
    peak > LIMIT_DEPTH
}

fn word_at(chars: &[char], index: &mut usize) -> String {
    let start = *index;
    while *index < chars.len() && is_word_char(chars[*index]) {
        *index += 1;
    }
    chars[start..*index].iter().collect()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    /// panic する解析関数を差し込む口（S1 の境界の確認に使う）。
    fn parse_with(input: &str, parser: ProgramParser) -> Outcome {
        parse_inner(input, parser)
    }

    // @kotowari[REQ-036]
    #[test]
    fn req_036_a_simple_command_is_parsed() {
        let outcome = parse("rm -rf /tmp/x");
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert_eq!(outcome.script.items.len(), 1);
    }

    // @kotowari[REQ-039]
    #[test]
    fn req_039_a_panicking_parser_becomes_a_failure() {
        fn boom(_: &str, _: &ParserOptions) -> Result<brush_parser::ast::Program, Failure> {
            panic!("panic in the injected parser")
        }
        let outcome = parse_with("rm -rf /tmp/x", boom);
        assert_eq!(outcome.failures, vec![Failure::Panic]);
        assert!(outcome.script.is_empty());
    }

    // @kotowari[REQ-039]
    #[test]
    fn req_039_input_over_one_mebibyte_is_too_large() {
        let input = "a".repeat(LIMIT_BYTES + 1);
        assert_eq!(parse(&input).failures, vec![Failure::TooLarge]);
    }

    // @kotowari[REQ-039]
    #[test]
    fn req_039_cumulative_substitution_reads_are_measured() {
        // 1 段ごとに約 2 万バイトの本体を持つ 100 段の入れ子。累計で 1 MiB を超える。
        let payload = "echo y; ".repeat(2_500);
        let mut input = String::from("echo ");
        for _ in 0..100 {
            input.push_str("$(");
        }
        input.push_str(&payload);
        for _ in 0..100 {
            input.push(')');
        }
        assert!(input.len() < LIMIT_BYTES);
        let outcome = parse(&input);
        assert!(
            outcome.failures.contains(&Failure::TooLarge),
            "{:?}",
            outcome.failures
        );
    }

    // @kotowari[REQ-039]
    #[test]
    fn req_039_depth_over_the_limit_is_too_deep() {
        let input = format!(
            "{}{}{}",
            "{ ".repeat(LIMIT_DEPTH + 1),
            "true",
            " ; }".repeat(LIMIT_DEPTH + 1)
        );
        let outcome = parse(&input);
        assert_eq!(outcome.failures, vec![Failure::TooDeep]);
        assert!(outcome.script.is_empty());
    }

    // @kotowari[REQ-039]
    #[test]
    fn req_039_depth_at_the_limit_is_read() {
        let mut input = String::from("echo ");
        for _ in 0..LIMIT_DEPTH {
            input.push_str("$(");
        }
        input.push_str("true");
        for _ in 0..LIMIT_DEPTH {
            input.push(')');
        }
        let outcome = parse(&input);
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    }

    // @kotowari[REQ-038]
    #[test]
    fn req_038_unreadable_syntax_is_a_failure() {
        let outcome = parse("if true; then");
        assert_eq!(outcome.failures, vec![Failure::Syntax]);
        assert!(outcome.script.is_empty());
    }

    // @kotowari[REQ-038]
    #[test]
    fn req_038_a_readable_prefix_survives() {
        let outcome = parse("rm -rf /etc/x; if true; then");
        assert!(
            outcome.failures.contains(&Failure::Syntax),
            "{:?}",
            outcome.failures
        );
        assert_eq!(outcome.script.items.len(), 1);
    }

    // @kotowari[REQ-036]
    #[test]
    fn req_036_strip_quotes_removes_quotes() {
        assert_eq!(strip_quotes("git \"push\" origin"), "git push origin");
        assert_eq!(strip_quotes("echo a\\ b"), "echo a b");
        assert!(strip_quotes("cat <<EOF\nbody\nEOF").starts_with("cat << EOF"));
    }
}
