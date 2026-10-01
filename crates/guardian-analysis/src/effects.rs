//! コマンド文字列から破壊的効果を取り出す。パスの解決もここで行う。
//!
//! 正規化した構文木だけを見て、すべての語・置換・リダイレクト・複合構文を
//! 1 回ずつ訪問する（REQ-037）。読めなかった命令からは効果を取り出さない
//! （REQ-038）。

use crate::command::{basename, split_assignment, strip_wrapper};
use guardian_core::{Ask, CommandFacts, Effect, Env, Invocation, Op, Target};
use guardian_parser::{
    Command, Compound, Part, Pipeline, Redirect, RedirectKind, RedirectTarget, Script,
    SimpleCommand, Substitution, Word,
};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

/// パスの解決の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Resolved {
    Path(PathBuf),
    Glob(PathBuf),
    Mktemp,
    UnknownSource,
    Unresolved(String),
}

impl Resolved {
    fn into_target(self, word: &Word) -> Target {
        match self {
            Resolved::Path(path) => Target::Path {
                path,
                dereference: word.text.ends_with('/'),
            },
            Resolved::Glob(base) => Target::GlobBase(base),
            Resolved::Mktemp => Target::Mktemp,
            Resolved::UnknownSource => Target::UnknownSource,
            Resolved::Unresolved(text) => Target::Unresolved(text),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    Path(PathBuf),
    Mktemp,
    UnknownSource,
    Unresolved(String),
}

#[derive(Clone)]
struct Context<'a> {
    home: Option<PathBuf>,
    tmpdir: Option<PathBuf>,
    cwd: Option<PathBuf>,
    vars: HashMap<String, Value>,
    parser: SharedParser<'a>,
    facts: std::rc::Rc<std::cell::RefCell<WalkFacts>>,
    collect_invocations: bool,
}
#[derive(Default)]
struct WalkFacts {
    invocations: Vec<Invocation>,
    // 同じ本文でも別の構文位置は別要求。木と保存したOutcomeは走査中ずっと生存する。
    inner: HashMap<usize, std::rc::Rc<guardian_parser::Outcome>>,
}
type SharedParser<'a> =
    std::rc::Rc<std::cell::RefCell<&'a mut dyn FnMut(&str) -> guardian_parser::Outcome>>;

impl<'a> Context<'a> {
    fn new(env: &Env, parser: &'a mut dyn FnMut(&str) -> guardian_parser::Outcome) -> Context<'a> {
        Context {
            home: env.home.clone(),
            tmpdir: env.tmpdir.clone(),
            cwd: env.cwd.clone(),
            vars: HashMap::new(),
            parser: std::rc::Rc::new(std::cell::RefCell::new(parser)),
            facts: Default::default(),
            collect_invocations: true,
        }
    }
}

/// コマンド文字列から効果を取り出す。
pub fn extract_effects(
    outcome: guardian_parser::Outcome,
    env: &Env,
    parser: &mut dyn FnMut(&str) -> guardian_parser::Outcome,
) -> Vec<Effect> {
    analyze(outcome, env, parser).effects
}

/// 判定の材料。効果と、判定できない理由。
pub use guardian_core::CommandFacts as Analysis;

/// コマンド文字列を解析して効果を取り出す。判定できない理由も返す。
pub fn analyze(
    outcome: guardian_parser::Outcome,
    env: &Env,
    parser: &mut dyn FnMut(&str) -> guardian_parser::Outcome,
) -> Analysis {
    let mut ctx = Context::new(env, parser);
    let mut effects = Vec::new();
    let mut asks: Vec<Ask> = outcome.failures.into_iter().map(Ask::Parse).collect();
    extract_script(&outcome.script, &mut ctx, &mut effects, &mut asks, 0);
    dedupe(&mut asks);
    let invocations = std::mem::take(&mut ctx.facts.borrow_mut().invocations);
    CommandFacts {
        effects,
        invocations,
        diagnostics: asks,
    }
}

fn dedupe(asks: &mut Vec<Ask>) {
    let mut out: Vec<Ask> = Vec::new();
    for ask in asks.drain(..) {
        if !out.contains(&ask) {
            out.push(ask);
        }
    }
    *asks = out;
}

fn too_deep(asks: &mut Vec<Ask>) {
    asks.push(Ask::Parse(guardian_parser::Failure::TooDeep));
}

fn extract_script(
    script: &Script,
    ctx: &mut Context,
    out: &mut Vec<Effect>,
    asks: &mut Vec<Ask>,
    depth: usize,
) {
    if depth > guardian_parser::LIMIT_DEPTH {
        too_deep(asks);
        return;
    }
    for item in &script.items {
        extract_pipeline(&item.first, ctx, out, asks, depth);
        for (_, pipeline) in &item.rest {
            extract_pipeline(pipeline, ctx, out, asks, depth);
        }
    }
}

fn extract_pipeline(
    pipeline: &Pipeline,
    ctx: &mut Context,
    out: &mut Vec<Effect>,
    asks: &mut Vec<Ask>,
    depth: usize,
) {
    let mut children_sources: Vec<(PathBuf, bool)> = Vec::new();
    for command in &pipeline.commands {
        children_sources = extract_command(command, ctx, out, asks, depth, children_sources);
    }
}

/// 1 つのコマンドを読み、効果を足す。次段の供給元（find の起点と
/// 末尾スラッシュの有無）を返す。
fn extract_command(
    command: &Command,
    ctx: &mut Context,
    out: &mut Vec<Effect>,
    asks: &mut Vec<Ask>,
    depth: usize,
    children_sources: Vec<(PathBuf, bool)>,
) -> Vec<(PathBuf, bool)> {
    match command {
        Command::Simple(simple) => extract_simple(simple, ctx, out, asks, depth, children_sources),
        Command::Compound {
            compound,
            redirects,
        } => {
            extract_redirects(redirects, ctx, out, asks, depth);
            extract_compound(compound, ctx, out, asks, depth);
            Vec::new()
        }
        Command::Function(function) => {
            extract_redirects(&function.redirects, ctx, out, asks, depth);
            scan_word_substitutions(&function.name, ctx, out, asks, depth);
            extract_compound(&function.body, ctx, out, asks, depth);
            Vec::new()
        }
        Command::Test(test) => {
            extract_redirects(&test.redirects, ctx, out, asks, depth);
            for word in &test.words {
                scan_word_substitutions(word, ctx, out, asks, depth);
            }
            Vec::new()
        }
    }
}

fn extract_compound(
    compound: &Compound,
    ctx: &mut Context,
    out: &mut Vec<Effect>,
    asks: &mut Vec<Ask>,
    depth: usize,
) {
    if depth > guardian_parser::LIMIT_DEPTH {
        too_deep(asks);
        return;
    }
    match compound {
        Compound::If {
            condition,
            then,
            elses,
        } => {
            extract_script(condition, ctx, out, asks, depth);
            extract_script(then, ctx, out, asks, depth);
            for clause in elses {
                if let Some(condition) = &clause.condition {
                    extract_script(condition, ctx, out, asks, depth);
                }
                extract_script(&clause.body, ctx, out, asks, depth);
            }
        }
        Compound::While {
            condition, body, ..
        } => {
            extract_script(condition, ctx, out, asks, depth);
            extract_script(body, ctx, out, asks, depth);
        }
        Compound::For { var, values, body } => {
            extract_for(var, values, body, ctx, out, asks, depth);
        }
        Compound::ArithmeticFor {
            initializer,
            condition,
            updater,
            body,
        } => {
            for word in [initializer, condition, updater].into_iter().flatten() {
                scan_word_substitutions(word, ctx, out, asks, depth);
            }
            extract_script(body, ctx, out, asks, depth);
        }
        Compound::Case { value, arms } => {
            scan_word_substitutions(value, ctx, out, asks, depth);
            for arm in arms {
                for pattern in &arm.patterns {
                    scan_word_substitutions(pattern, ctx, out, asks, depth);
                }
                if let Some(body) = &arm.body {
                    extract_script(body, ctx, out, asks, depth);
                }
            }
        }
        Compound::BraceGroup(script) | Compound::Subshell(script) => {
            extract_script(script, ctx, out, asks, depth);
        }
        Compound::Arithmetic(word) => {
            scan_word_substitutions(word, ctx, out, asks, depth);
        }
        Compound::Coprocess { name, body } => {
            if let Some(name) = name {
                scan_word_substitutions(name, ctx, out, asks, depth);
            }
            extract_command(body, ctx, out, asks, depth, Vec::new());
        }
    }
}

fn extract_for(
    var: &str,
    values: &[Word],
    body: &Script,
    ctx: &mut Context,
    out: &mut Vec<Effect>,
    asks: &mut Vec<Ask>,
    depth: usize,
) {
    let mut resolved = Vec::new();
    let mut all_literal = !values.is_empty();
    for word in values {
        scan_word_substitutions(word, ctx, out, asks, depth);
        match resolve_word(word, ctx) {
            Resolved::Path(path) | Resolved::Glob(path) => resolved.push(Value::Path(path)),
            Resolved::Mktemp => resolved.push(Value::Mktemp),
            _ => all_literal = false,
        }
    }
    if all_literal {
        for (index, value) in resolved.into_iter().enumerate() {
            let mut child = ctx.clone();
            child.collect_invocations = ctx.collect_invocations && index == 0;
            child.vars.insert(var.to_string(), value);
            extract_script(body, &mut child, out, asks, depth + 1);
        }
    } else {
        let mut child = ctx.clone();
        child.vars.insert(var.to_string(), Value::UnknownSource);
        extract_script(body, &mut child, out, asks, depth + 1);
    }
}

/// 1 つの単純コマンドを読み、効果を足す。次段の供給元（find の起点と
/// 末尾スラッシュの有無）を返す。
fn extract_simple(
    simple: &SimpleCommand,
    ctx: &mut Context,
    out: &mut Vec<Effect>,
    asks: &mut Vec<Ask>,
    depth: usize,
    children_sources: Vec<(PathBuf, bool)>,
) -> Vec<(PathBuf, bool)> {
    let mut index = 0;
    let mut assignments = Vec::new();
    while index < simple.words.len() {
        match split_assignment(&simple.words[index]) {
            Some(pair) => {
                assignments.push(pair);
                index += 1;
            }
            None => break,
        }
    }
    if index == simple.words.len() {
        for (position, (name, value)) in assignments.into_iter().enumerate() {
            scan_word_substitutions(&simple.words[position], ctx, out, asks, depth);
            let value = resolve_value(&value, ctx);
            ctx.vars.insert(name, value);
        }
        extract_redirects(&simple.redirects, ctx, out, asks, depth);
        extract_process_substitutions(simple, ctx, out, asks, depth);
        return Vec::new();
    }

    for word in &simple.words[..index] {
        scan_word_substitutions(word, ctx, out, asks, depth);
    }
    extract_redirects(&simple.redirects, ctx, out, asks, depth);
    extract_process_substitutions(simple, ctx, out, asks, depth);

    let rest = &simple.words[index..];
    // ラッパー（sudo / doas）を外す。外した語（値付きオプションの値など）の
    // 置換も読む（EX-047）。
    let (wrapper_names, stripped) = strip_wrapper(rest);
    for word in rest {
        scan_word_substitutions(word, ctx, out, asks, depth);
    }
    let Some(program) = stripped.first() else {
        return Vec::new();
    };
    let name = basename(&program.text);
    let args = &stripped[1..];
    let shell_body = crate::command::shell_kind(name) == crate::command::ShellKind::BashLike
        && crate::command::shell_c_index(args).is_some();
    if ctx.collect_invocations && name != "eval" && !shell_body {
        let mut env_names: Vec<String> = assignments.iter().map(|(name, _)| name.clone()).collect();
        env_names.extend(wrapper_names);
        ctx.facts.borrow_mut().invocations.push(Invocation {
            program: name.to_string(),
            words: args.iter().map(|w| w.text.clone()).collect(),
            env_names,
        });
    }
    if program.literal_value().is_none() {
        asks.push(Ask::UnreadableProgram(program.text.clone()));
        return Vec::new();
    }

    // シェルの起動（REQ-035）。
    match crate::command::shell_kind(name) {
        crate::command::ShellKind::NonPosix => {
            asks.push(Ask::UnsupportedShell(name.to_string()));
            return Vec::new();
        }
        crate::command::ShellKind::BashLike => {
            extract_shell(
                args,
                simple as *const SimpleCommand as usize,
                ctx,
                out,
                asks,
                depth,
            );
            return Vec::new();
        }
        crate::command::ShellKind::Other => {}
    }

    match name {
        "rm" | "rmdir" | "unlink" => {
            let (targets, _) = option_targets(args, &[]);
            for target in targets {
                let res = resolve_word(target, ctx);
                out.push(Effect {
                    op: Op::Delete,
                    target: res.into_target(target),
                });
            }
            Vec::new()
        }
        "shred" => {
            let (targets, _) = option_targets(args, SHRED_VALUED);
            for target in targets {
                let res = resolve_word(target, ctx);
                out.push(Effect {
                    op: Op::Delete,
                    target: res.into_target(target),
                });
            }
            Vec::new()
        }
        "truncate" => {
            let (targets, _) = option_targets(args, TRUNCATE_VALUED);
            for target in targets {
                let res = resolve_word(target, ctx);
                out.push(Effect {
                    op: Op::Truncate,
                    target: res.into_target(target),
                });
            }
            Vec::new()
        }
        "dd" => {
            for word in args {
                let Some(value_word) = strip_prefix_word(word, "of=") else {
                    // of= 以外の語（if= など）の中のコマンド置換も読む。
                    continue;
                };
                if value_word.text.is_empty() || value_word.text == "-" {
                    continue;
                }
                let res = resolve_word(&value_word, ctx);
                let op = match &res {
                    Resolved::Path(path) if path.starts_with("/dev") => Op::Format,
                    _ => Op::Truncate,
                };
                out.push(Effect {
                    op,
                    target: res.into_target(&value_word),
                });
            }
            Vec::new()
        }
        "mkfs" | "wipefs" => {
            let valued: &[&str] = if name == "mkfs" {
                &["-t", "--type", "-L", "--label"]
            } else {
                &["-o", "-O", "-t", "--offset", "--types", "--output"]
            };
            let (targets, _) = option_targets(args, valued);
            for target in targets {
                let res = resolve_word(target, ctx);
                out.push(Effect {
                    op: Op::Format,
                    target: res.into_target(target),
                });
            }
            Vec::new()
        }
        _ if name.starts_with("mkfs") => {
            let (targets, _) = option_targets(args, &["-t", "--type", "-L", "--label"]);
            for target in targets {
                let res = resolve_word(target, ctx);
                out.push(Effect {
                    op: Op::Format,
                    target: res.into_target(target),
                });
            }
            Vec::new()
        }
        "find" => find_effects(args, ctx, out),
        "xargs" => {
            xargs_effects(args, &children_sources, out);
            Vec::new()
        }
        "eval" => {
            let mut pieces = Vec::new();
            let mut all_literal = true;
            for word in args {
                match word.literal_value() {
                    Some(value) => pieces.push(value),
                    None => {
                        all_literal = false;
                        break;
                    }
                }
            }
            if all_literal {
                if !pieces.is_empty() {
                    let inner = pieces.join(" ");
                    extract_inner(
                        &inner,
                        simple as *const SimpleCommand as usize,
                        ctx,
                        out,
                        asks,
                        depth,
                    );
                }
            } else {
                asks.push(Ask::UnreadableEval);
            }
            Vec::new()
        }
        "cd" => {
            update_cwd(args, ctx);
            Vec::new()
        }
        "export" | "local" | "declare" | "readonly" | "typeset" => {
            for word in args {
                if let Some((name, value)) = split_assignment(word) {
                    let value = resolve_value(&value, ctx);
                    ctx.vars.insert(name, value);
                }
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// bash 系のシェル起動を読む（REQ-035）。`-c` の本体だけを読み直す。
fn extract_shell(
    args: &[Word],
    position: usize,
    ctx: &mut Context,
    out: &mut Vec<Effect>,
    asks: &mut Vec<Ask>,
    depth: usize,
) {
    if let Some(body) = crate::command::shell_c_index(args).and_then(|index| args.get(index + 1)) {
        match body.literal_value() {
            Some(inner) => {
                extract_inner(&inner, position, ctx, out, asks, depth);
            }
            None => {
                asks.push(Ask::UnreadableShellBody(body.text.clone()));
            }
        }
    }
}

/// 内側のコマンド文字列を読み直して効果を足す。
fn extract_inner(
    inner: &str,
    position: usize,
    ctx: &mut Context,
    out: &mut Vec<Effect>,
    asks: &mut Vec<Ask>,
    depth: usize,
) {
    let cached = ctx.facts.borrow().inner.get(&position).cloned();
    let outcome = cached.unwrap_or_else(|| {
        let outcome = std::rc::Rc::new((ctx.parser.borrow_mut())(inner));
        ctx.facts
            .borrow_mut()
            .inner
            .insert(position, outcome.clone());
        outcome
    });
    for failure in &outcome.failures {
        asks.push(Ask::Parse(failure.clone()));
    }
    let mut child = ctx.clone();
    extract_script(&outcome.script, &mut child, out, asks, depth + 1);
}

/// 1 つの語のコマンド置換の内側だけを読む。
fn scan_word_substitutions(
    word: &Word,
    ctx: &Context,
    out: &mut Vec<Effect>,
    asks: &mut Vec<Ask>,
    depth: usize,
) {
    scan_parts_substitutions(&word.parts, ctx, out, asks, depth);
}

/// 断片の並びの中のコマンド置換の内側だけを読む。
fn scan_parts_substitutions(
    parts: &[Part],
    ctx: &Context,
    out: &mut Vec<Effect>,
    asks: &mut Vec<Ask>,
    depth: usize,
) {
    for part in parts {
        if let Part::Substitution(substitution) = part {
            if let Some(script) = &substitution.body {
                let mut child = ctx.clone();
                extract_script(script, &mut child, out, asks, depth + 1);
            }
        }
    }
}

/// プロセス置換の本体を読む。
fn extract_process_substitutions(
    simple: &SimpleCommand,
    ctx: &Context,
    out: &mut Vec<Effect>,
    asks: &mut Vec<Ask>,
    depth: usize,
) {
    for substitution in &simple.process_substitutions {
        let mut child = ctx.clone();
        extract_script(&substitution.body, &mut child, out, asks, depth + 1);
    }
}

/// `of=...` のような前置きを外した語を作る。引用やエスケープで断片に
/// 分かれた綴りでも、引用を外した見かけの前置きで判定する（REQ-001）。
fn strip_prefix_word(word: &Word, prefix: &str) -> Option<Word> {
    if !word.text.starts_with(prefix) {
        return None;
    }
    let mut remaining = prefix.len();
    let mut parts = Vec::new();
    for part in &word.parts {
        if remaining == 0 {
            parts.push(part.clone());
            continue;
        }
        let rendered = rendered_len(part);
        if rendered <= remaining {
            remaining -= rendered;
            continue;
        }
        let rest = match part {
            Part::Literal(text) => Part::Literal(text[remaining..].to_string()),
            Part::Quoted(text) => Part::Quoted(text[remaining..].to_string()),
            Part::Opaque(text) => Part::Opaque(text[remaining..].to_string()),
            Part::Glob(text) => Part::Glob(text[remaining..].to_string()),
            // 前置き（ASCII の綴り）が展開の断片の途中で切れることはない。
            Part::Var(_) | Part::Substitution(_) => return None,
        };
        parts.push(rest);
        remaining = 0;
    }
    Some(Word::from_parts(parts))
}

/// 断片が語の見かけの文字列に占める長さ。
fn rendered_len(part: &Part) -> usize {
    match part {
        Part::Literal(text) | Part::Quoted(text) | Part::Opaque(text) | Part::Glob(text) => {
            text.len()
        }
        Part::Var(name) => name.len() + 1,
        Part::Substitution(substitution) => substitution.body_text.len() + 3,
    }
}

const TRUNCATE_VALUED: &[&str] = &["-s", "--size", "-r", "--reference"];

/// shred の値付きオプション。値は削除の対象ではない。
const SHRED_VALUED: &[&str] = &["-n", "--iterations", "-s", "--size", "--random-source"];

/// 効果の対象を、`-` で始まる語を読み飛ばして集める。対象から外した語も返す
/// （値付きオプションの値など。対象にはしないが、コマンド置換は読む）。
fn option_targets<'a>(args: &'a [Word], valued: &[&str]) -> (Vec<&'a Word>, Vec<&'a Word>) {
    let mut targets = Vec::new();
    let mut skipped = Vec::new();
    let mut after_ddash = false;
    let mut skip_next = false;
    for word in args {
        if skip_next {
            skip_next = false;
            skipped.push(word);
            continue;
        }
        if after_ddash {
            targets.push(word);
            continue;
        }
        if word.text == "--" {
            after_ddash = true;
            continue;
        }
        if word.text.starts_with('-') && word.text != "-" {
            let head = word.text.split('=').next().unwrap_or(&word.text);
            if valued.contains(&head) && !word.text.contains('=') {
                skip_next = true;
            }
            skipped.push(word);
            continue;
        }
        targets.push(word);
    }
    (targets, skipped)
}

/// find の起点の読み取り結果。
enum FindStarts<'a> {
    /// 起点の語。述語の前ならいくつでも並ぶ。
    Words(Vec<&'a Word>),
    /// `-D help`。find はデバッグ用の一覧を出すだけで探索しない。
    DebugHelp,
}

/// 述語（式）の先頭になる語か。`-x`、`(`、`!` のいずれか。`-` だけは起点。
fn is_expression_word(text: &str) -> bool {
    text == "(" || text == "!" || (text.starts_with('-') && text != "-")
}

/// find の起点。先頭の全体オプションだけを読み飛ばし、述語が先に来るときは
/// 起点なし（cwd）とする。`--` は全体オプションの読み取りの終わりだけを告げる
/// ので、その次の語も起点と述語の判定に掛ける。
fn find_starts(args: &[Word]) -> FindStarts<'_> {
    let mut starts = Vec::new();
    let mut after_ddash = false;
    let mut index = 0;
    while index < args.len() {
        let text = &args[index].text;
        if !after_ddash {
            if text == "-L" || text == "-H" || text == "-P" {
                index += 1;
                continue;
            }
            if text == "-D" {
                if args.get(index + 1).is_some_and(|w| w.text == "help") {
                    return FindStarts::DebugHelp;
                }
                index += 2;
                continue;
            }
            // -O の水準は -O3 のように続けて書く。次の語は消費しない。
            if text.starts_with("-O") {
                index += 1;
                continue;
            }
            if text == "--" {
                after_ddash = true;
                index += 1;
                continue;
            }
        }
        if is_expression_word(text) {
            // 述語が先に来るときは起点なし（cwd）。
            break;
        }
        starts.push(&args[index]);
        index += 1;
    }
    FindStarts::Words(starts)
}

fn find_effects(args: &[Word], ctx: &mut Context, out: &mut Vec<Effect>) -> Vec<(PathBuf, bool)> {
    let starts = find_starts(args);
    let starts = match starts {
        FindStarts::Words(words) => words,
        // 探索しないので、削除の効果も次段への供給元もない。
        FindStarts::DebugHelp => return Vec::new(),
    };
    let mut has_delete = false;
    for (index, word) in args.iter().enumerate() {
        if word.text == "-delete" {
            has_delete = true;
        } else if word.text == "-exec" || word.text == "-execdir" {
            if let Some(next) = args.get(index + 1) {
                if basename(&next.text) == "rm" {
                    has_delete = true;
                }
            }
        }
    }
    let mut sources = Vec::new();
    if starts.is_empty() {
        let base = resolve_text(".", false, ctx);
        if let Some(source) = find_source(base, false, has_delete, out) {
            sources.push(source);
        }
    } else {
        for word in starts {
            let dereference = word.text.ends_with('/');
            let base = resolve_word(word, ctx);
            if let Some(source) = find_source(base, dereference, has_delete, out) {
                sources.push(source);
            }
        }
    }
    sources
}

/// 起点の子の削除の効果を足し、次段への供給元（起点と末尾スラッシュの有無）を返す。
fn find_source(
    res: Resolved,
    dereference: bool,
    has_delete: bool,
    out: &mut Vec<Effect>,
) -> Option<(PathBuf, bool)> {
    if has_delete {
        let target = match &res {
            Resolved::Path(path) | Resolved::Glob(path) => Target::Children {
                base: path.clone(),
                dereference,
            },
            Resolved::Mktemp => Target::Mktemp,
            Resolved::UnknownSource => Target::UnknownSource,
            Resolved::Unresolved(text) => Target::Unresolved(text.clone()),
        };
        out.push(Effect {
            op: Op::Delete,
            target,
        });
    }
    match res {
        Resolved::Path(path) | Resolved::Glob(path) => Some((path, dereference)),
        _ => None,
    }
}

const XARGS_VALUED: &[&str] = &[
    "-I",
    "-i",
    "-L",
    "-l",
    "-n",
    "-s",
    "-a",
    "-d",
    "-E",
    "-P",
    "--replace",
    "--max-lines",
    "--max-args",
    "--max-chars",
    "--arg-file",
    "--delimiter",
    "--eof",
    "--max-procs",
];

fn xargs_effects(args: &[Word], children_sources: &[(PathBuf, bool)], out: &mut Vec<Effect>) {
    let mut index = 0;
    let mut utility: Option<&Word> = None;
    while index < args.len() {
        let text = &args[index].text;
        if text == "--" {
            utility = args.get(index + 1);
            break;
        }
        if text.starts_with('-') && text != "-" {
            let head = text.split('=').next().unwrap_or(text);
            if XARGS_VALUED.contains(&head) && !text.contains('=') {
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        utility = Some(&args[index]);
        break;
    }
    let Some(utility) = utility else { return };
    if basename(&utility.text) != "rm" {
        return;
    }
    // 引数中の対象は供給元から来る。供給元の子の集合ごとに効果を出す。
    if children_sources.is_empty() {
        out.push(Effect {
            op: Op::Delete,
            target: Target::UnknownSource,
        });
        return;
    }
    for (base, dereference) in children_sources {
        out.push(Effect {
            op: Op::Delete,
            target: Target::Children {
                base: base.clone(),
                dereference: *dereference,
            },
        });
    }
}

fn update_cwd(args: &[Word], ctx: &mut Context) {
    let chosen = args
        .iter()
        .position(|word| !word.text.starts_with('-') || word.text == "-");
    match chosen {
        None => {
            // `cd` はホームへ。
            ctx.cwd = ctx.home.clone();
        }
        Some(index) => {
            // 値付きオプション（`-P` など）は読み飛ばさない。最初の非オプションの語。
            let word = &args[index];
            if word.text == "-" {
                // `cd -` は OLDPWD へ移る。解決できるパスではない。
                ctx.cwd = None;
            } else {
                match resolve_word(word, ctx) {
                    Resolved::Path(path) => ctx.cwd = Some(path),
                    _ => ctx.cwd = None,
                }
            }
        }
    }
}

fn extract_redirects(
    redirects: &[Redirect],
    ctx: &mut Context,
    out: &mut Vec<Effect>,
    asks: &mut Vec<Ask>,
    depth: usize,
) {
    for redirect in redirects {
        if let RedirectTarget::Word(word) = &redirect.target {
            scan_word_substitutions(word, ctx, out, asks, depth);
        }
        match &redirect.target {
            RedirectTarget::Word(word) => match redirect.kind {
                RedirectKind::Write
                | RedirectKind::Clobber
                | RedirectKind::OutputAndError(false) => {
                    let res = resolve_word(word, ctx);
                    out.push(Effect {
                        op: Op::Truncate,
                        target: res.into_target(word),
                    });
                }
                // `>& file` は `&> file` の別の綴りで、ファイルを切り詰める。
                // 語が数字か "-" のときだけファイル記述子への複製になる。
                RedirectKind::DuplicateOutput
                    if redirect.fd.is_none() && !is_fd_duplication_target(word) =>
                {
                    let res = resolve_word(word, ctx);
                    out.push(Effect {
                        op: Op::Truncate,
                        target: res.into_target(word),
                    });
                }
                _ => {}
            },
            RedirectTarget::Fd(_) => {}
            RedirectTarget::ProcessSubstitution(substitution) => {
                let mut child = ctx.clone();
                extract_script(&substitution.body, &mut child, out, asks, depth + 1);
            }
            RedirectTarget::HereDocument { doc, .. } => {
                scan_word_substitutions(doc, ctx, out, asks, depth);
            }
        }
    }
}

/// `>&word` の語がファイル記述子への複製か。数字か "-" だけが複製で、
/// それ以外の語はファイル名として切り詰められる（REQ-001）。
fn is_fd_duplication_target(word: &Word) -> bool {
    word.text == "-" || (!word.text.is_empty() && word.text.chars().all(|c| c.is_ascii_digit()))
}

/// 語から値だけを解決する。子の訪問はwalkerが担当する。
fn resolve_word(word: &Word, ctx: &Context) -> Resolved {
    if word.parts.len() == 1 {
        match &word.parts[0] {
            Part::Substitution(substitution) => {
                return substitution_target(substitution, word);
            }
            Part::Var(name) => {
                if let Some(value) = lookup_var(name, ctx) {
                    return match value {
                        Value::Path(path) => Resolved::Path(path),
                        Value::Mktemp => Resolved::Mktemp,
                        Value::UnknownSource => Resolved::UnknownSource,
                        Value::Unresolved(text) => Resolved::Unresolved(text),
                    };
                }
                return Resolved::Unresolved(word.text.clone());
            }
            Part::Opaque(_) => return Resolved::Unresolved(word.text.clone()),
            Part::Glob(_) => {}
            Part::Literal(text) | Part::Quoted(text) => return resolve_text(text, false, ctx),
        }
    }

    let mut text = String::new();
    let mut has_glob = word.has_glob;
    let mut mktemp_prefix = false;
    for (index, part) in word.parts.iter().enumerate() {
        match part {
            Part::Literal(value) | Part::Quoted(value) => text.push_str(value),
            Part::Var(name) => match lookup_var(name, ctx) {
                Some(Value::Path(path)) => text.push_str(&path.to_string_lossy()),
                // 先頭が mktemp の作ったパスなら、続く部分はその配下。
                Some(Value::Mktemp) if index == 0 => mktemp_prefix = true,
                _ => return Resolved::Unresolved(word.text.clone()),
            },
            Part::Substitution(substitution) => {
                if matches!(substitution_target(substitution, word), Resolved::Mktemp) && index == 0
                {
                    mktemp_prefix = true;
                } else {
                    return Resolved::Unresolved(word.text.clone());
                }
            }
            Part::Opaque(_) => return Resolved::Unresolved(word.text.clone()),
            Part::Glob(glob) => {
                has_glob = true;
                text.push_str(glob);
            }
        }
    }
    if mktemp_prefix {
        // `..` で mktemp の外へ出る綴りは解決しない。
        if Path::new(&text)
            .components()
            .any(|c| c == Component::ParentDir)
        {
            return Resolved::Unresolved(word.text.clone());
        }
        return Resolved::Mktemp;
    }
    resolve_text(&text, has_glob, ctx)
}

/// 置換そのものの解決結果。効果の抽出はしない（断片の語では先に読む）。
fn substitution_target(substitution: &Substitution, word: &Word) -> Resolved {
    match &substitution.body {
        Some(script) if is_mktemp_script(script) => Resolved::Mktemp,
        _ => Resolved::Unresolved(word.text.clone()),
    }
}

fn is_mktemp_script(script: &Script) -> bool {
    let [item] = script.items.as_slice() else {
        return false;
    };
    if !item.rest.is_empty() {
        return false;
    }
    let [command] = item.first.commands.as_slice() else {
        return false;
    };
    let Command::Simple(simple) = command else {
        return false;
    };
    let Some(first) = simple.words.first() else {
        return false;
    };
    basename(&first.text) == "mktemp"
}

fn lookup_var(name: &str, ctx: &Context) -> Option<Value> {
    if let Some(value) = ctx.vars.get(name) {
        return Some(value.clone());
    }
    match name {
        "PWD" => ctx.cwd.clone().map(Value::Path),
        "HOME" => ctx.home.clone().map(Value::Path),
        "TMPDIR" => ctx.tmpdir.clone().map(Value::Path),
        _ => None,
    }
}

/// 文字列をパスとして解決する。
fn resolve_text(text: &str, has_glob: bool, ctx: &Context) -> Resolved {
    let expanded = if text == "~" {
        match &ctx.home {
            Some(home) => home.to_string_lossy().to_string(),
            None => return Resolved::Unresolved(text.to_string()),
        }
    } else if let Some(rest) = text.strip_prefix("~/") {
        match &ctx.home {
            Some(home) => home.join(rest).to_string_lossy().to_string(),
            None => return Resolved::Unresolved(text.to_string()),
        }
    } else if text.starts_with('~') {
        // "~user" や "~+" は解決しない。cwd 相対にはしない。
        return Resolved::Unresolved(text.to_string());
    } else {
        text.to_string()
    };

    if has_glob {
        let base = glob_base(&expanded);
        let path = if base.is_empty() {
            match &ctx.cwd {
                Some(cwd) => cwd.clone(),
                None => return Resolved::Unresolved(text.to_string()),
            }
        } else if Path::new(&base).is_absolute() {
            clean_path(Path::new(&base))
        } else {
            match &ctx.cwd {
                Some(cwd) => clean_path(&cwd.join(&base)),
                None => return Resolved::Unresolved(text.to_string()),
            }
        };
        return Resolved::Glob(path);
    }

    if expanded.is_empty() {
        return Resolved::Unresolved(text.to_string());
    }
    if Path::new(&expanded).is_absolute() {
        Resolved::Path(clean_path(Path::new(&expanded)))
    } else {
        match &ctx.cwd {
            Some(cwd) => Resolved::Path(clean_path(&cwd.join(&expanded))),
            None => Resolved::Unresolved(text.to_string()),
        }
    }
}

/// glob が広がり得る最も外側のディレクトリ。最初に glob 文字を含む要素より前。
fn glob_base(text: &str) -> String {
    let mut base = String::new();
    for comp in text.split('/') {
        if comp.contains(['*', '?', '[']) {
            break;
        }
        if !base.is_empty() || text.starts_with('/') {
            base.push('/');
        }
        base.push_str(comp);
    }
    if base.is_empty() {
        return String::new();
    }
    // 末尾の空要素（glob が最後）を除く。
    while base.len() > 1 && base.ends_with('/') {
        base.pop();
    }
    base
}

fn clean_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                // ルートより上へは行かない。相対パスの先頭の ".." は残す。
                if !out.pop() && !out.has_root() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

fn resolve_value(word: &Word, ctx: &Context) -> Value {
    match resolve_word(word, ctx) {
        Resolved::Path(path) => Value::Path(path),
        Resolved::Glob(path) => Value::Path(path),
        Resolved::Mktemp => Value::Mktemp,
        Resolved::UnknownSource => Value::UnknownSource,
        Resolved::Unresolved(text) => Value::Unresolved(text),
    }
}
