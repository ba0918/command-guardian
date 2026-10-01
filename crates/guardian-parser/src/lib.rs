//! コマンド文字列の字句解析と構文解析。成熟した OSS のシェルパーサ
//! （brush-parser、版を固定）を背後に隠し、正規化した構文木だけを公開する
//! （REQ-036）。
//!
//! - 入力由来の解析は隔離した子プロセス（同じ実行ファイル、スタックの上限つき）で
//!   行う。引用の除去も同じ隔離の内側（スタックの上限つきのスレッド）で行う。
//!   子の死は原因で分け（REQ-039・A23）、入力に帰せる死（スタック
//!   オーバーフロー、時間の上限の超過）と上限の超過は Failure::Limit（block）、
//!   自分に帰せる死（panic、起動とプロトコルの失敗、帰せない死）は
//!   Failure::Panic・Failure::Internal（ask）に落とし、判定は必ず返る。
//! - サイズは構文解析の前に測る。置換の再帰読みは累計で測る（REQ-039）。
//! - 深さは、正規化した構文木が保つ入れ子（複合構文と置換の再帰）を成功後の走査で
//!   測る。構文木では平坦な断片になる再帰読み（パラメータ展開と算術式）と `[[ ]]`
//!   の括弧は、その入れ子を正規化の中で過大評価で数える（断片のテキストの括弧を
//!   数え落とさない）。字句解析による事前の測定はしない（REQ-039）。
//! - 解析に失敗した命令からは効果を返さない。読めている命令だけを構文木に残す
//!   （REQ-038）。

pub mod ast;
mod budget;
mod depth;
mod normalize;
mod wire;
mod worker;

pub use ast::*;
pub use worker::run_if_child;

use brush_parser::{ParserOptions, Token};
use normalize::{options, parse_program, parse_tokens, tokenize, unquoted_word, Normalizer};

/// コマンド文字列の上限（REQ-039）。置換の再帰読みでは累計で測る。
pub const LIMIT_BYTES: usize = 1024 * 1024;

/// 構文の入れ子と置換の再帰の上限（REQ-039）。
pub const LIMIT_DEPTH: usize = 128;

/// 1 回の判定で行う構文解析の回数の上限（REQ-039）。固定値。
pub const LIMIT_EXCHANGES: usize = 1000;

/// 1 回の判定の時間の上限（REQ-039）。固定値。
pub const LIMIT_JUDGMENT_TIME: std::time::Duration = std::time::Duration::from_secs(5);

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
    worker::run_if_child();
    // サイズは構文解析の前に測る（REQ-039）。子を起こす前の近道。
    if input.len() > LIMIT_BYTES {
        return Outcome::failure(Failure::TooLarge);
    }
    // 子の死は原因で分ける（REQ-039・A23）。入力に帰せる死（スタック
    // オーバーフロー、時間の上限の超過）と上限の超過は Limit（block）、
    // 自分に帰せる失敗（起動・プロトコルの失敗、panic、帰せない死）は
    // Internal（ask）に落とす。
    match worker::request_parse(input) {
        Ok(outcome) => outcome,
        Err(failure) => Outcome::failure(failure),
    }
}

/// 1 回の判定の予算を数え直す（REQ-039）。判定の入口が呼ぶ。
pub fn begin_judgment() {
    budget::begin();
}

/// この判定で予算を使い切ったか（REQ-039）。判定の終わりに確かめる。
pub fn judgment_over_budget() -> bool {
    budget::exceeded()
}

/// 引用を外したコマンド本文。失敗は原因を返す（REQ-039・A23）。設定の照合
/// （見張りの例の分割とカスタムルールの本文の引用の除去）で使う（REQ-036）。
pub fn strip_quotes(input: &str) -> Result<String, Failure> {
    worker::run_if_child();
    worker::request_strip_quotes(input)
}

/// 差し込み可能な生の構文解析。panic はスレッドの境界で Failure に変える（REQ-038）。
pub(crate) type ProgramParser =
    fn(&str, &ParserOptions) -> Result<brush_parser::ast::Program, Failure>;

fn parse_inner(input: &str, parser: ProgramParser) -> Outcome {
    // サイズは構文解析の前に測る（REQ-039）。
    if input.len() > LIMIT_BYTES {
        return Outcome::failure(Failure::TooLarge);
    }
    let options = options();
    let mut normalizer = Normalizer::new(LIMIT_BYTES - input.len());
    match parser(input, &options) {
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

/// 子プロセス側の引用の除去。解析と同じ隔離の内側（スタックの上限つきの
/// スレッド）で行う（REQ-039）。設定の照合が消えないよう、解析の死より深い
/// 入力まで本文を返す。スレッドを起こせないときと panic は Failure::Panic に
/// する。
pub(crate) fn strip_quotes_in_child(input: &str) -> Result<String, Failure> {
    match in_bounded_thread(worker::STRIP_STACK_BYTES, move || strip_quotes_inner(input)) {
        Some(result) => result,
        None => Err(Failure::Panic),
    }
}

/// panic を捕まえて Err に変える境界（REQ-038）。
pub(crate) fn catch<T>(f: impl FnOnce() -> T) -> Result<T, ()> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|_| ())
}

/// 子プロセス側の解析。上限つきのスタックのスレッドで行い、panic は
/// Failure::Panic、深さの上限超えは Failure::TooDeep にする（REQ-039）。
pub(crate) fn parse_in_child(input: &str) -> (Vec<Failure>, Option<Script>) {
    parse_in_child_with(input, parse_program)
}

/// 解析の関数を差し込める形。スレッドを起こせないときと panic は Panic。
pub(crate) fn parse_in_child_with(
    input: &str,
    parser: ProgramParser,
) -> (Vec<Failure>, Option<Script>) {
    match in_bounded_thread(worker::CHILD_STACK_BYTES, move || {
        parse_inner(input, parser)
    }) {
        Some(outcome) => finish(outcome),
        None => (vec![Failure::Panic], None),
    }
}

/// 深さは、成功した正規化の構文木の走査で測る（REQ-039）。超えたら構文木は
/// 返さない（親が深い木を読まないようにするため）。
fn finish(outcome: Outcome) -> (Vec<Failure>, Option<Script>) {
    if depth::exceeds(&outcome.script) {
        (vec![Failure::TooDeep], None)
    } else {
        (outcome.failures, Some(outcome.script))
    }
}

/// 処理をスタックの上限つきのスレッドで行う。スレッドを作れないときは None。
fn in_bounded_thread<T: Send>(stack: usize, f: impl FnOnce() -> T + Send) -> Option<T> {
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("guardian-parser".to_string())
            .stack_size(stack)
            .spawn_scoped(scope, f)
            .ok()?;
        handle.join().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// panic する解析関数を差し込む口（S1 の境界の確認に使う）。
    fn parse_with(input: &str, parser: ProgramParser) -> Outcome {
        let (failures, script) = parse_in_child_with(input, parser);
        Outcome {
            script: script.unwrap_or_default(),
            failures,
        }
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

    // @kotowari[REQ-039]
    #[test]
    fn req_039_parse_exchanges_over_the_budget_fail() {
        // 1 回の判定の構文解析は 1000 回まで。1001 回目は Limit になる。
        begin_judgment();
        for _ in 0..LIMIT_EXCHANGES {
            let outcome = parse("true");
            assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        }
        // 引用の除去は字句解析であり、構文解析の回数の予算を消費しない。
        assert_eq!(strip_quotes("true").unwrap(), "true");
        let outcome = parse("true");
        assert_eq!(outcome.failures, vec![Failure::Limit]);
        assert!(outcome.script.is_empty());
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

    // @kotowari[REQ-036]
    #[test]
    fn req_036_strip_quotes_survives_a_deep_input() {
        // 引用の除去も解析と同じ隔離の内側（スタックの上限つきのスレッド）で
        // 行う（REQ-039）。深い入力でも本文が返り、設定の照合が空にならない。
        let deep = format!(
            "echo $(true)#{}: {} ; git push origin main",
            "$(".repeat(2000),
            ")".repeat(2000)
        );
        let text = strip_quotes(&deep).expect("深い入力でも引用を外せる");
        assert!(text.contains("git push origin main"), "{text}");
    }
}
