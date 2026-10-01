//! コマンド文字列の字句解析と構文解析。成熟した OSS のシェルパーサ
//! （brush-parser、版を固定）を背後に隠し、正規化した構文木だけを公開する
//! （REQ-036）。
//!
//! - 純粋な構文解析と引用除去を提供する。本番の入力はguardian-appが同じ
//!   実行ファイルの子で隔離し、スタック・回数・待ち時間・回収を管理する。
//! - サイズは構文解析の前に測る。置換の再帰読みは累計で測る（REQ-039）。
//! - 深さは、正規化した構文木が保つ入れ子（複合構文と置換の再帰）を成功後の走査で
//!   測る。構文木では平坦な断片になる再帰読み（パラメータ展開と算術式）と `[[ ]]`
//!   の括弧は、その入れ子を正規化の中で過大評価で数える（断片のテキストの括弧を
//!   数え落とさない）。字句解析による事前の測定はしない（REQ-039）。
//! - 解析に失敗した命令からは効果を返さない。読めている命令だけを構文木に残す
//!   （REQ-038）。

pub mod ast;
mod depth;
mod normalize;

pub use ast::*;

use brush_parser::{ParserOptions, Token};
use normalize::{options, parse_program, parse_tokens, tokenize, unquoted_word, Normalizer};

/// コマンド文字列の上限（REQ-039）。置換の再帰読みでは累計で測る。
pub const LIMIT_BYTES: usize = 1024 * 1024;

/// 構文の入れ子と置換の再帰の上限（REQ-039）。
pub const LIMIT_DEPTH: usize = 128;

/// 読み直しの試行の上限。壊れた入力を読み続けないための歯止め。
const MAX_RECOVERY_ATTEMPTS: usize = 64;

pub use guardian_core::Failure;

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
    let outcome = parse_inner(input);
    if depth::exceeds(&outcome.script) {
        Outcome::failure(Failure::TooDeep)
    } else {
        outcome
    }
}

/// 引用を外したコマンド本文。失敗は原因を返す（REQ-039・A23）。設定の照合
/// （見張りの例の分割とカスタムルールの本文の引用の除去）で使う（REQ-036）。
pub fn strip_quotes(input: &str) -> Result<String, Failure> {
    strip_quotes_inner(input)
}

fn parse_inner(input: &str) -> Outcome {
    // サイズは構文解析の前に測る（REQ-039）。
    if input.len() > LIMIT_BYTES {
        return Outcome::failure(Failure::TooLarge);
    }
    let options = options();
    let mut normalizer = Normalizer::new(LIMIT_BYTES - input.len());
    match parse_program(input, &options) {
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

/// 引用を外したコマンド本文。読めなかったときは理由を返す（REQ-038）。
pub(crate) fn strip_quotes_inner(input: &str) -> Result<String, Failure> {
    let options = options();
    let tokens = tokenize(input, &options)?;
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
    Ok(out.join(" "))
}

/// panic を捕まえて Err に変える境界（REQ-038）。
pub(crate) fn catch<T>(f: impl FnOnce() -> T) -> Result<T, ()> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    // @kotowari[REQ-036]
    #[test]
    fn req_036_a_simple_command_is_parsed() {
        let outcome = parse("rm -rf /tmp/x");
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert_eq!(outcome.script.items.len(), 1);
    }

    // @kotowari[REQ-039]
    #[test]
    fn req_039_input_over_one_mebibyte_is_too_large() {
        let input = "a".repeat(LIMIT_BYTES + 1);
        assert_eq!(parse(&input).failures, vec![Failure::TooLarge]);
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
        assert_eq!(
            strip_quotes("git \"push\" origin").unwrap(),
            "git push origin"
        );
        assert_eq!(strip_quotes("echo a\\ b").unwrap(), "echo a b");
        assert!(strip_quotes("cat <<EOF\nbody\nEOF")
            .unwrap()
            .starts_with("cat << EOF"));
    }
}
