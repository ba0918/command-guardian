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
/// 引用・コメント・エスケープを読み飛ばし、括弧・波括弧・バッククォートと
/// 制御構造の語を数える。数え漏らすと OSS パーサが再帰でプロセスを落とすため、
/// 少なく見積もらない側に倒す。
fn depth_over_limit(input: &str) -> bool {
    const NORMAL: char = 'n';
    const DOUBLE: char = 'd';
    const SINGLE: char = 's';
    const BACKTICK: char = 'b';

    let chars: Vec<char> = input.chars().collect();
    let mut states = vec![NORMAL];
    let mut sub_marks: Vec<usize> = Vec::new();
    let mut depth = 0usize;
    let mut keywords = 0usize;
    let mut peak = 0usize;
    let mut at_word_start = true;
    let mut index = 0;
    let bump = |depth: usize, keywords: usize, peak: &mut usize| {
        let value = depth + keywords;
        if value > *peak {
            *peak = value;
        }
        value > LIMIT_DEPTH
    };
    while index < chars.len() {
        let c = chars[index];
        let state = *states.last().unwrap_or(&NORMAL);
        match state {
            SINGLE => {
                if c == '\'' {
                    states.pop();
                }
                index += 1;
            }
            BACKTICK => {
                if c == '\\' {
                    index += 2;
                    continue;
                }
                if c == '`' {
                    states.pop();
                    index += 1;
                    continue;
                }
                if c == '$' && chars.get(index + 1) == Some(&'(') {
                    depth += 1;
                    sub_marks.push(depth);
                    states.push(NORMAL);
                    index += 2;
                    continue;
                }
                if is_word_char(c) {
                    let word = word_at(&chars, &mut index);
                    count_keyword(&word, &mut keywords);
                    if bump(depth, keywords, &mut peak) {
                        return true;
                    }
                    continue;
                }
                index += 1;
            }
            DOUBLE => match c {
                '\\' => index += 2,
                '"' => {
                    states.pop();
                    index += 1;
                }
                '$' if chars.get(index + 1) == Some(&'(') => {
                    depth += 1;
                    sub_marks.push(depth);
                    states.push(NORMAL);
                    index += 2;
                }
                '`' => {
                    states.push(BACKTICK);
                    index += 1;
                }
                _ => index += 1,
            },
            _ => match c {
                '\\' => {
                    index += 2;
                    at_word_start = false;
                }
                '\'' => {
                    states.push(SINGLE);
                    index += 1;
                    at_word_start = false;
                }
                '"' => {
                    states.push(DOUBLE);
                    index += 1;
                    at_word_start = false;
                }
                '`' => {
                    states.push(BACKTICK);
                    index += 1;
                    at_word_start = false;
                }
                '#' if at_word_start => {
                    while index < chars.len() && chars[index] != '\n' {
                        index += 1;
                    }
                }
                '(' | '{' => {
                    depth += 1;
                    if bump(depth, keywords, &mut peak) {
                        return true;
                    }
                    index += 1;
                    at_word_start = true;
                }
                ')' => {
                    if sub_marks.last() == Some(&depth) {
                        sub_marks.pop();
                        states.pop();
                    }
                    depth = depth.saturating_sub(1);
                    index += 1;
                    at_word_start = true;
                }
                '}' => {
                    depth = depth.saturating_sub(1);
                    index += 1;
                    at_word_start = true;
                }
                _ if is_word_char(c) => {
                    let word = word_at(&chars, &mut index);
                    count_keyword(&word, &mut keywords);
                    if bump(depth, keywords, &mut peak) {
                        return true;
                    }
                    at_word_start = false;
                }
                _ => {
                    at_word_start = c.is_whitespace() || matches!(c, ';' | '&' | '|');
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

fn count_keyword(word: &str, keywords: &mut usize) {
    match word {
        "if" | "while" | "until" | "for" | "case" => *keywords += 1,
        "fi" | "done" | "esac" => *keywords = keywords.saturating_sub(1),
        _ => {}
    }
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
