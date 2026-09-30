//! コマンド文字列から破壊的効果を取り出す。パスの解決もここで行う。

use crate::parse::{self, Item, Part, Pipeline, Redirect, SimpleCommand, Word};
use crate::types::{Effect, Op, Target};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

/// 解決に使う環境。テストから差し替えられる。
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub home: Option<PathBuf>,
    pub tmpdir: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
}

impl Env {
    pub fn from_process() -> Env {
        Env {
            home: std::env::var_os("HOME").map(PathBuf::from),
            tmpdir: std::env::var_os("TMPDIR").map(PathBuf::from),
            cwd: std::env::current_dir().ok(),
        }
    }
}

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
struct Context {
    home: Option<PathBuf>,
    tmpdir: Option<PathBuf>,
    cwd: Option<PathBuf>,
    vars: HashMap<String, Value>,
}

impl Context {
    fn new(env: &Env) -> Context {
        Context {
            home: env.home.clone(),
            tmpdir: env.tmpdir.clone(),
            cwd: env.cwd.clone(),
            vars: HashMap::new(),
        }
    }
}

/// コマンド文字列から効果を取り出す。
pub fn extract_effects(command: &str, env: &Env) -> Vec<Effect> {
    analyze(command, env).effects
}

/// 判定の材料。効果と、解析の失敗。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Analysis {
    pub effects: Vec<Effect>,
    pub parse_errors: Vec<String>,
}

/// コマンド文字列を解析して効果を取り出す。解析の失敗も返す。
pub fn analyze(command: &str, env: &Env) -> Analysis {
    let (script, mut errors) = parse::parse_script_with_errors(command);
    collect_subst_errors(&script.items, &mut errors, 0);
    let mut ctx = Context::new(env);
    let mut out = Vec::new();
    extract_items(&script.items, &mut ctx, &mut out, 0);
    Analysis {
        effects: out,
        parse_errors: errors,
    }
}

/// コマンド置換の内側の解析の失敗を集める。
fn collect_subst_errors(items: &[Item], errors: &mut Vec<String>, depth: usize) {
    if depth > 16 {
        return;
    }
    let visit_word = |w: &Word, errors: &mut Vec<String>| {
        for part in &w.parts {
            if let Part::Subst(inner) = part {
                let (script, mut inner_errors) = parse::parse_script_with_errors(inner);
                errors.append(&mut inner_errors);
                collect_subst_errors(&script.items, errors, depth + 1);
            }
        }
    };
    for item in items {
        match item {
            Item::Simple(p) => {
                for cmd in &p.commands {
                    for w in &cmd.words {
                        visit_word(w, errors);
                    }
                    for r in &cmd.redirects {
                        match r {
                            Redirect::Out(w)
                            | Redirect::Append(w)
                            | Redirect::Clobber(w)
                            | Redirect::In(w)
                            | Redirect::Heredoc(w) => visit_word(w, errors),
                        }
                    }
                }
            }
            Item::For { words, body, .. } => {
                for w in words {
                    visit_word(w, errors);
                }
                collect_subst_errors(body, errors, depth + 1);
            }
        }
    }
}

fn extract_items(items: &[Item], ctx: &mut Context, out: &mut Vec<Effect>, depth: usize) {
    if depth > 16 {
        return;
    }
    for item in items {
        match item {
            Item::Simple(p) => extract_pipeline(p, ctx, out, depth),
            Item::For { var, words, body } => {
                let mut values = Vec::new();
                let mut all_literal = !words.is_empty();
                for w in words {
                    match resolve_word(w, ctx, out, depth) {
                        Resolved::Path(p) | Resolved::Glob(p) => values.push(Value::Path(p)),
                        Resolved::Mktemp => values.push(Value::Mktemp),
                        _ => all_literal = false,
                    }
                }
                if all_literal {
                    for v in values {
                        let mut child = ctx.clone();
                        child.vars.insert(var.clone(), v);
                        extract_items(body, &mut child, out, depth + 1);
                    }
                } else {
                    let mut child = ctx.clone();
                    child.vars.insert(var.clone(), Value::UnknownSource);
                    extract_items(body, &mut child, out, depth + 1);
                }
            }
        }
    }
}

fn extract_pipeline(p: &Pipeline, ctx: &mut Context, out: &mut Vec<Effect>, depth: usize) {
    let mut children_source: Option<PathBuf> = None;
    for cmd in &p.commands {
        children_source = extract_command(cmd, ctx, out, depth, children_source);
    }
}

/// 1 つの単純コマンドを読み、効果を足す。次段の供給元（find の base）を返す。
fn extract_command(
    cmd: &SimpleCommand,
    ctx: &mut Context,
    out: &mut Vec<Effect>,
    depth: usize,
    children_source: Option<PathBuf>,
) -> Option<PathBuf> {
    let mut idx = 0;
    let mut assignments = Vec::new();
    while idx < cmd.words.len() {
        match split_assignment(&cmd.words[idx]) {
            Some(pair) => {
                assignments.push(pair);
                idx += 1;
            }
            None => break,
        }
    }
    if idx == cmd.words.len() {
        for (name, value) in assignments {
            let v = resolve_value(&value, ctx, out, depth);
            ctx.vars.insert(name, v);
        }
        extract_redirects(cmd, ctx, out, depth);
        return None;
    }

    let mut words: &[Word] = &cmd.words[idx..];
    extract_redirects(cmd, ctx, out, depth);

    // ラッパー（sudo / doas）と env の代入を外す。
    loop {
        let Some(first) = words.first() else { break };
        let name = basename(&first.text);
        if name == "sudo" || name == "doas" {
            words = strip_wrapper(words, name);
            continue;
        }
        break;
    }
    let Some(first) = words.first() else {
        return None;
    };
    let name = basename(&first.text);
    let args = &words[1..];

    match name {
        "rm" | "rmdir" | "unlink" | "shred" => {
            for target in option_targets(args, &[]) {
                let res = resolve_word(target, ctx, out, depth);
                out.push(Effect {
                    op: Op::Delete,
                    target: res.into_target(target),
                });
            }
            None
        }
        "truncate" => {
            for target in option_targets(args, TRUNCATE_VALUED) {
                let res = resolve_word(target, ctx, out, depth);
                out.push(Effect {
                    op: Op::Truncate,
                    target: res.into_target(target),
                });
            }
            None
        }
        "dd" => {
            for w in args {
                let Some(value_word) = strip_prefix_word(w, "of=") else {
                    continue;
                };
                if value_word.text.is_empty() || value_word.text == "-" {
                    continue;
                }
                let res = resolve_word(&value_word, ctx, out, depth);
                let op = match &res {
                    Resolved::Path(p) if p.starts_with("/dev") => Op::Format,
                    _ => Op::Truncate,
                };
                out.push(Effect {
                    op,
                    target: res.into_target(&value_word),
                });
            }
            None
        }
        "mkfs" | "wipefs" => {
            let valued: &[&str] = if name == "mkfs" {
                &["-t", "--type"]
            } else {
                &["-o", "-O", "-t", "--offset", "--types", "--output"]
            };
            for target in option_targets(args, valued) {
                let res = resolve_word(target, ctx, out, depth);
                out.push(Effect {
                    op: Op::Format,
                    target: res.into_target(target),
                });
            }
            None
        }
        _ if name.starts_with("mkfs") => {
            for target in option_targets(args, &["-t", "--type"]) {
                let res = resolve_word(target, ctx, out, depth);
                out.push(Effect {
                    op: Op::Format,
                    target: res.into_target(target),
                });
            }
            None
        }
        "find" => find_effects(args, ctx, out, depth),
        "xargs" => {
            xargs_effects(args, children_source.clone(), ctx, out, depth);
            scan_substitutions(args, ctx, out, depth);
            None
        }
        "bash" | "sh" | "zsh" | "dash" | "ksh" => {
            if let Some(inner) = shell_c_string(args) {
                let script = parse::parse_script(&inner);
                let mut child = ctx.clone();
                extract_items(&script.items, &mut child, out, depth + 1);
            } else {
                scan_substitutions(args, ctx, out, depth);
            }
            None
        }
        "eval" => {
            let mut pieces = Vec::new();
            for w in args {
                match w.literal_value() {
                    Some(s) => pieces.push(s),
                    None => return None,
                }
            }
            if !pieces.is_empty() {
                let script = parse::parse_script(&pieces.join(" "));
                let mut child = ctx.clone();
                extract_items(&script.items, &mut child, out, depth + 1);
            }
            None
        }
        "cd" => {
            update_cwd(args, ctx, out, depth);
            None
        }
        "export" | "local" | "declare" | "readonly" | "typeset" => {
            for w in args {
                if let Some((n, v)) = split_assignment(w) {
                    let value = resolve_value(&v, ctx, out, depth);
                    ctx.vars.insert(n, value);
                }
            }
            None
        }
        _ => {
            scan_substitutions(args, ctx, out, depth);
            None
        }
    }
}

/// コマンド置換の内側だけを読む。既知のコマンドとして扱わない語に使う。
fn scan_substitutions(args: &[Word], ctx: &Context, out: &mut Vec<Effect>, depth: usize) {
    for w in args {
        for part in &w.parts {
            if let Part::Subst(inner) = part {
                let script = parse::parse_script(inner);
                let mut child = ctx.clone();
                extract_items(&script.items, &mut child, out, depth + 1);
            }
        }
    }
}

/// `of=...` のような前置きを外した語を作る。
fn strip_prefix_word(w: &Word, prefix: &str) -> Option<Word> {
    let first = match w.parts.first() {
        Some(Part::Literal(s)) => s.as_str(),
        _ => return None,
    };
    let rest = first.strip_prefix(prefix)?;
    let mut parts = Vec::new();
    if !rest.is_empty() {
        parts.push(Part::Literal(rest.to_string()));
    }
    parts.extend(w.parts[1..].iter().cloned());
    let text: String = parts
        .iter()
        .map(|p| match p {
            Part::Literal(s) => s.clone(),
            Part::Var(n) => format!("${n}"),
            Part::Subst(s) => format!("$({s})"),
            Part::Opaque(s) => s.clone(),
            Part::Glob(g) => g.clone(),
        })
        .collect();
    Some(Word {
        parts,
        text,
        has_glob: w.has_glob,
    })
}

const TRUNCATE_VALUED: &[&str] = &["-s", "--size", "-r", "--reference"];

/// 効果の対象を、`-` で始まる語を読み飛ばして集める。
fn option_targets<'a>(args: &'a [Word], valued: &[&str]) -> Vec<&'a Word> {
    let mut targets = Vec::new();
    let mut after_ddash = false;
    let mut skip_next = false;
    for w in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if after_ddash {
            targets.push(w);
            continue;
        }
        if w.text == "--" {
            after_ddash = true;
            continue;
        }
        if w.text.starts_with('-') && w.text != "-" {
            let head = w.text.split('=').next().unwrap_or(&w.text);
            if valued.contains(&head) && !w.text.contains('=') {
                skip_next = true;
            }
            continue;
        }
        targets.push(w);
    }
    targets
}

fn find_effects(
    args: &[Word],
    ctx: &mut Context,
    out: &mut Vec<Effect>,
    depth: usize,
) -> Option<PathBuf> {
    let base_word = args
        .iter()
        .find(|w| !w.text.starts_with('-') || w.text == "-");
    let base_res = match base_word {
        Some(w) => resolve_word(w, ctx, out, depth),
        None => resolve_text(".", false, ctx),
    };
    let mut has_delete = false;
    let mut i = 0;
    while i < args.len() {
        let text = &args[i].text;
        if text == "-delete" {
            has_delete = true;
        } else if text == "-exec" || text == "-execdir" {
            if let Some(next) = args.get(i + 1) {
                if basename(&next.text) == "rm" {
                    has_delete = true;
                }
            }
        }
        i += 1;
    }
    let base_path = match &base_res {
        Resolved::Path(p) | Resolved::Glob(p) => Some(p.clone()),
        _ => None,
    };
    if has_delete {
        let target = match base_res {
            Resolved::Path(p) | Resolved::Glob(p) => Target::Children(p),
            Resolved::Mktemp => Target::Mktemp,
            Resolved::UnknownSource => Target::UnknownSource,
            Resolved::Unresolved(t) => Target::Unresolved(t),
        };
        out.push(Effect {
            op: Op::Delete,
            target,
        });
    }
    base_path
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

fn xargs_effects(
    args: &[Word],
    children_source: Option<PathBuf>,
    ctx: &mut Context,
    out: &mut Vec<Effect>,
    depth: usize,
) {
    let mut i = 0;
    let mut utility: Option<&Word> = None;
    while i < args.len() {
        let text = &args[i].text;
        if text == "--" {
            utility = args.get(i + 1);
            break;
        }
        if text.starts_with('-') && text != "-" {
            let head = text.split('=').next().unwrap_or(text);
            if XARGS_VALUED.contains(&head) && !text.contains('=') {
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        utility = Some(&args[i]);
        break;
    }
    let Some(u) = utility else { return };
    if basename(&u.text) != "rm" {
        return;
    }
    // 引数中の対象は供給元から来る。供給元が分かればその子、分からなければ unknown。
    let target = match children_source {
        Some(p) => Target::Children(p),
        None => Target::UnknownSource,
    };
    let _ = (ctx, depth);
    out.push(Effect {
        op: Op::Delete,
        target,
    });
}

fn shell_c_string(args: &[Word]) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        let t = &args[i].text;
        let is_c =
            t == "-c" || (t.starts_with('-') && !t.starts_with("--") && t[1..].contains('c'));
        if is_c {
            if let Some(next) = args.get(i + 1) {
                return next.literal_value();
            }
            return None;
        }
        i += 1;
    }
    None
}

fn update_cwd(args: &[Word], ctx: &mut Context, out: &mut Vec<Effect>, depth: usize) {
    let arg = args
        .iter()
        .find(|w| !w.text.starts_with('-') || w.text == "-");
    match arg {
        None => {
            // `cd` はホームへ。
            if let Some(home) = &ctx.home {
                ctx.cwd = Some(home.clone());
            } else {
                ctx.cwd = None;
            }
        }
        Some(w) => match resolve_word(w, ctx, out, depth) {
            Resolved::Path(p) => ctx.cwd = Some(p),
            _ => ctx.cwd = None,
        },
    }
}

fn extract_redirects(cmd: &SimpleCommand, ctx: &mut Context, out: &mut Vec<Effect>, depth: usize) {
    for r in &cmd.redirects {
        match r {
            Redirect::Out(w) | Redirect::Clobber(w) => {
                let res = resolve_word(w, ctx, out, depth);
                out.push(Effect {
                    op: Op::Truncate,
                    target: res.into_target(w),
                });
            }
            Redirect::Append(_) | Redirect::In(_) | Redirect::Heredoc(_) => {}
        }
    }
}

/// 語を解決する。コマンド置換の内側の効果もここで取り出す。
fn resolve_word(word: &Word, ctx: &Context, out: &mut Vec<Effect>, depth: usize) -> Resolved {
    if word.parts.len() == 1 {
        match &word.parts[0] {
            Part::Subst(inner) => {
                return resolve_subst(inner, word, ctx, out, depth);
            }
            Part::Var(name) => {
                if let Some(v) = lookup_var(name, ctx) {
                    return match v {
                        Value::Path(p) => Resolved::Path(p),
                        Value::Mktemp => Resolved::Mktemp,
                        Value::UnknownSource => Resolved::UnknownSource,
                        Value::Unresolved(t) => Resolved::Unresolved(t),
                    };
                }
                return Resolved::Unresolved(word.text.clone());
            }
            Part::Opaque(_) => return Resolved::Unresolved(word.text.clone()),
            Part::Glob(_) => {}
            Part::Literal(s) => return resolve_text(s, false, ctx),
        }
    }

    let mut text = String::new();
    let mut has_glob = word.has_glob;
    for part in &word.parts {
        match part {
            Part::Literal(s) => text.push_str(s),
            Part::Var(name) => match lookup_var(name, ctx) {
                Some(Value::Path(p)) => text.push_str(&p.to_string_lossy()),
                _ => return Resolved::Unresolved(word.text.clone()),
            },
            Part::Subst(inner) => {
                resolve_subst(inner, word, ctx, out, depth);
                return Resolved::Unresolved(word.text.clone());
            }
            Part::Opaque(_) => return Resolved::Unresolved(word.text.clone()),
            Part::Glob(g) => {
                has_glob = true;
                text.push_str(g);
            }
        }
    }
    resolve_text(&text, has_glob, ctx)
}

fn resolve_subst(
    inner: &str,
    word: &Word,
    ctx: &Context,
    out: &mut Vec<Effect>,
    depth: usize,
) -> Resolved {
    let script = parse::parse_script(inner);
    let mut child = ctx.clone();
    extract_items(&script.items, &mut child, out, depth + 1);
    if is_mktemp_script(&script.items) {
        Resolved::Mktemp
    } else {
        Resolved::Unresolved(word.text.clone())
    }
}

fn is_mktemp_script(items: &[Item]) -> bool {
    let [Item::Simple(pipeline)] = items else {
        return false;
    };
    let [cmd] = pipeline.commands.as_slice() else {
        return false;
    };
    let Some(first) = cmd.words.first() else {
        return false;
    };
    basename(&first.text) == "mktemp"
}

fn lookup_var(name: &str, ctx: &Context) -> Option<Value> {
    if let Some(v) = ctx.vars.get(name) {
        return Some(v.clone());
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
            Some(h) => h.to_string_lossy().to_string(),
            None => return Resolved::Unresolved(text.to_string()),
        }
    } else if let Some(rest) = text.strip_prefix("~/") {
        match &ctx.home {
            Some(h) => h.join(rest).to_string_lossy().to_string(),
            None => return Resolved::Unresolved(text.to_string()),
        }
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

fn clean_path(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                let popped = out.pop();
                if !popped {
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

fn split_assignment(w: &Word) -> Option<(String, Word)> {
    let first = match w.parts.first() {
        Some(Part::Literal(s)) => s.clone(),
        _ => return None,
    };
    let eq = first.find('=')?;
    if eq == 0 {
        return None;
    }
    let name = &first[..eq];
    if !name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    let rest = &first[eq + 1..];
    let mut parts = Vec::new();
    if !rest.is_empty() {
        parts.push(Part::Literal(rest.to_string()));
    }
    parts.extend(w.parts[1..].iter().cloned());
    let text: String = parts
        .iter()
        .map(|p| match p {
            Part::Literal(s) => s.clone(),
            Part::Var(n) => format!("${n}"),
            Part::Subst(s) => format!("$({s})"),
            Part::Opaque(s) => s.clone(),
            Part::Glob(g) => g.clone(),
        })
        .collect();
    Some((
        name.to_string(),
        Word {
            parts,
            text,
            has_glob: w.has_glob,
        },
    ))
}

fn resolve_value(w: &Word, ctx: &Context, out: &mut Vec<Effect>, depth: usize) -> Value {
    match resolve_word(w, ctx, out, depth) {
        Resolved::Path(p) => Value::Path(p),
        Resolved::Glob(p) => Value::Path(p),
        Resolved::Mktemp => Value::Mktemp,
        Resolved::UnknownSource => Value::UnknownSource,
        Resolved::Unresolved(t) => Value::Unresolved(t),
    }
}

/// 語の basename。
pub fn basename(text: &str) -> &str {
    text.rsplit('/').next().unwrap_or(text)
}

/// ラッパーのオプションで値を取るもの。
const SUDO_VALUED: &[&str] = &[
    "-u",
    "-g",
    "-p",
    "-C",
    "-h",
    "-r",
    "-t",
    "-U",
    "-T",
    "-R",
    "-D",
    "--user",
    "--group",
    "--prompt",
    "--close-from",
    "--host",
    "--role",
    "--type",
    "--other-user",
    "--command-timeout",
    "--chdir",
    "--chroot",
];

const DOAS_VALUED: &[&str] = &["-u", "-C", "-a"];

fn strip_wrapper<'a>(words: &'a [Word], name: &str) -> &'a [Word] {
    let valued = if name == "doas" {
        DOAS_VALUED
    } else {
        SUDO_VALUED
    };
    let mut i = 1;
    while i < words.len() {
        let t = &words[i].text;
        if t == "--" {
            i += 1;
            break;
        }
        if !t.starts_with('-') || t == "-" {
            break;
        }
        let head = t.split('=').next().unwrap_or(t);
        if valued.contains(&head) && !t.contains('=') {
            i += 2;
        } else {
            i += 1;
        }
    }
    // ラッパー越しの代入。
    while i < words.len() && split_assignment(&words[i]).is_some() {
        i += 1;
    }
    &words[i..]
}
