//! コマンドの見張り（REQ-027〜REQ-034）。規則の形と語の照合を扱う。

use guardian_core::parse::{self, Item, Part, Redirect, SimpleCommand, Word};
use guardian_core::Verdict;
use regex::Regex;

/// 1 つの語の照合。`/.../` は語全体への正規表現、それ以外は完全一致。
#[derive(Debug, Clone)]
pub struct WordMatch {
    source: String,
    regex: Option<Regex>,
}

impl PartialEq for WordMatch {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl Eq for WordMatch {}

impl WordMatch {
    fn new(source: &str) -> Result<WordMatch, regex::Error> {
        if source.len() >= 2 && source.starts_with('/') && source.ends_with('/') {
            let inner = &source[1..source.len() - 1];
            let regex = Regex::new(&format!("^(?:{inner})$"))?;
            Ok(WordMatch {
                source: source.to_string(),
                regex: Some(regex),
            })
        } else {
            Ok(WordMatch {
                source: source.to_string(),
                regex: None,
            })
        }
    }

    pub fn is_match(&self, word: &str) -> bool {
        match &self.regex {
            Some(re) => re.is_match(word),
            None => self.source == word,
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }
}

/// 1 つの位置の代替の語の一覧。
pub type Alternatives = Vec<WordMatch>;
/// 語の並び。位置ごとに 1 つの語か代替の一覧。
pub type WordSeq = Vec<Alternatives>;

/// 見張りの規則（REQ-027）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardRule {
    pub program: String,
    pub reason: String,
    pub verdict: Verdict,
    pub options_with_value: Vec<String>,
    pub for_: Vec<WordSeq>,
    pub deny: Vec<WordSeq>,
    pub deny_flags: Vec<String>,
    pub deny_option_values: Vec<(String, Vec<WordMatch>)>,
    pub deny_env: Vec<WordMatch>,
    pub only: Vec<WordSeq>,
    pub examples_deny: Vec<String>,
    pub examples_allow: Vec<String>,
}

/// 1 つの起動。program とその引数、先頭の代入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub program: String,
    pub words: Vec<String>,
    pub env_names: Vec<String>,
}

/// 設定の `commands.guard` を読む（REQ-027）。
pub fn parse_guards(root: &toml::Value, warnings: &mut Vec<String>) -> Vec<GuardRule> {
    let Some(value) = root.get("commands").and_then(|c| c.get("guard")) else {
        return Vec::new();
    };
    let Some(items) = value.as_array() else {
        warnings.push("commands.guard は規則の一覧ではないため無視します".to_string());
        return Vec::new();
    };
    let mut rules = Vec::new();
    for item in items {
        let name = item
            .get("program")
            .and_then(|v| v.as_str())
            .unwrap_or("<program なし>")
            .to_string();
        match parse_guard(item) {
            Ok(rule) => rules.push(rule),
            Err(e) => warnings.push(format!("見張りの規則を無効にします（{name}）: {e}")),
        }
    }
    rules
}

/// テスト用に TOML の文書から読む。
pub fn parse_guard_rules_document(
    text: &str,
) -> Result<(Vec<GuardRule>, Vec<String>), toml::de::Error> {
    let value: toml::Value = toml::from_str(text)?;
    let mut warnings = Vec::new();
    let rules = parse_guards(&value, &mut warnings);
    Ok((rules, warnings))
}

fn parse_guard(item: &toml::Value) -> Result<GuardRule, String> {
    let program = item
        .get("program")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("program が無い")?
        .to_string();
    let reason = item
        .get("reason")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("reason が空")?
        .to_string();
    let verdict = match item.get("verdict").and_then(|v| v.as_str()) {
        None | Some("ask") => Verdict::Ask,
        Some("block") => Verdict::Block,
        Some(other) => return Err(format!("verdict が不正（{other}）")),
    };
    let options_with_value = string_list(item.get("options-with-value"), "options-with-value")?;
    let for_ = word_seq_list(item.get("for"), "for")?;
    let deny = word_seq_list(item.get("deny"), "deny")?;
    let deny_flags = string_list(item.get("deny-flags"), "deny-flags")?;
    let deny_option_values = option_values(item.get("deny-option-values"))?;
    let deny_env = word_list(item.get("deny-env"), "deny-env")?;
    let only = word_seq_list(item.get("only"), "only")?;
    let (examples_deny, examples_allow) = examples(item.get("examples"))?;

    if deny.is_empty()
        && deny_flags.is_empty()
        && deny_option_values.is_empty()
        && deny_env.is_empty()
        && only.is_empty()
    {
        return Err(
            "deny・deny-flags・deny-option-values・deny-env・only のどれも無い".to_string(),
        );
    }

    Ok(GuardRule {
        program,
        reason,
        verdict,
        options_with_value,
        for_,
        deny,
        deny_flags,
        deny_option_values,
        deny_env,
        only,
        examples_deny,
        examples_allow,
    })
}

fn string_list(value: Option<&toml::Value>, key: &str) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(items) = value.as_array() else {
        return Err(format!("{key} がリストではない"));
    };
    let mut out = Vec::new();
    for item in items {
        out.push(
            item.as_str()
                .ok_or(format!("{key} の要素が文字列ではない"))?
                .to_string(),
        );
    }
    Ok(out)
}

fn word_list(value: Option<&toml::Value>, key: &str) -> Result<Vec<WordMatch>, String> {
    let raw = string_list(value, key)?;
    raw.iter()
        .map(|s| WordMatch::new(s).map_err(|e| format!("{key} の正規表現が不正: {e}")))
        .collect()
}

fn word_seq_list(value: Option<&toml::Value>, key: &str) -> Result<Vec<WordSeq>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(items) = value.as_array() else {
        return Err(format!("{key} がリストではない"));
    };
    let mut out = Vec::new();
    for item in items {
        out.push(parse_word_seq(item, key)?);
    }
    Ok(out)
}

fn parse_word_seq(item: &toml::Value, key: &str) -> Result<WordSeq, String> {
    let Some(positions) = item.as_array() else {
        return Err(format!("{key} の項目が語の並びではない"));
    };
    let mut seq = WordSeq::new();
    for position in positions {
        let mut alternatives = Alternatives::new();
        match position {
            toml::Value::String(s) => alternatives
                .push(WordMatch::new(s).map_err(|e| format!("{key} の正規表現が不正: {e}"))?),
            toml::Value::Array(items) => {
                for a in items {
                    let s = a.as_str().ok_or(format!("{key} の代替に語以外がある"))?;
                    alternatives.push(
                        WordMatch::new(s).map_err(|e| format!("{key} の正規表現が不正: {e}"))?,
                    );
                }
            }
            _ => return Err(format!("{key} の位置が語でも代替の一覧でもない")),
        }
        if alternatives.is_empty() {
            return Err(format!("{key} の代替が空"));
        }
        seq.push(alternatives);
    }
    Ok(seq)
}

fn option_values(value: Option<&toml::Value>) -> Result<Vec<(String, Vec<WordMatch>)>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(table) = value.as_table() else {
        return Err("deny-option-values が表ではない".to_string());
    };
    let mut out = Vec::new();
    for (name, values) in table {
        let words = word_list(Some(values), "deny-option-values")?;
        out.push((name.clone(), words));
    }
    Ok(out)
}

fn examples(value: Option<&toml::Value>) -> Result<(Vec<String>, Vec<String>), String> {
    let Some(value) = value else {
        return Ok((Vec::new(), Vec::new()));
    };
    let deny = string_list(value.get("deny"), "examples.deny")?;
    let allow = string_list(value.get("allow"), "examples.allow")?;
    Ok((deny, allow))
}

/// `deny`・`for`・`only` の先頭一致（REQ-029）。
fn seq_matches(seq: &WordSeq, words: &[String]) -> bool {
    if seq.len() > words.len() {
        return false;
    }
    seq.iter()
        .zip(words.iter())
        .all(|(alts, word)| alts.iter().any(|m| m.is_match(word)))
}

/// プログラム名の直後のオプションを読み飛ばした残り（REQ-029）。
fn skip_options<'a>(words: &'a [String], options_with_value: &[String]) -> &'a [String] {
    let mut i = 0;
    while i < words.len() {
        let w = &words[i];
        if w == "--" {
            i += 1;
            break;
        }
        if !w.starts_with('-') || w == "-" {
            break;
        }
        let head = w.split('=').next().unwrap_or(w);
        if w.contains('=') {
            i += 1;
        } else if options_with_value.iter().any(|o| o == head) {
            i += 2;
        } else {
            i += 1;
        }
    }
    &words[i.min(words.len())..]
}

impl GuardRule {
    /// 1 つの起動に一致するか。
    pub fn matches(&self, inv: &Invocation) -> bool {
        if inv.program != self.program {
            return false;
        }
        let rest = skip_options(&inv.words, &self.options_with_value);
        let for_ok = self.for_.is_empty() || self.for_.iter().any(|s| seq_matches(s, rest));

        let deny_hit = self.deny.iter().any(|s| seq_matches(s, rest));
        let flags_hit = for_ok && self.deny_flags.iter().any(|f| flag_hit(f, &inv.words));
        let optval_hit = for_ok
            && self
                .deny_option_values
                .iter()
                .any(|(name, values)| option_value_hit(name, values, &inv.words));
        let env_hit = for_ok
            && self
                .deny_env
                .iter()
                .any(|m| inv.env_names.iter().any(|n| m.is_match(n)));

        if deny_hit || flags_hit || optval_hit || env_hit {
            return true;
        }
        if !self.only.is_empty() {
            return !self.only.iter().any(|s| seq_matches(s, rest));
        }
        false
    }
}

/// 最初の `--` より前の語を返す（REQ-030）。
fn before_ddash(words: &[String]) -> &[String] {
    match words.iter().position(|w| w == "--") {
        Some(i) => &words[..i],
        None => words,
    }
}

fn flag_hit(flag: &str, words: &[String]) -> bool {
    let single = flag.len() == 2 && flag.starts_with('-');
    for w in before_ddash(words) {
        let head = w.split('=').next().unwrap_or(w);
        if single {
            if head.starts_with('-') && !head.starts_with("--") {
                for c in head[1..].chars() {
                    if format!("-{c}") == flag {
                        return true;
                    }
                }
            }
        } else if head == flag {
            return true;
        }
    }
    false
}

fn option_value_hit(name: &str, values: &[WordMatch], words: &[String]) -> bool {
    let words = before_ddash(words);
    let mut i = 0;
    while i < words.len() {
        let w = &words[i];
        if w == name {
            if let Some(value) = words.get(i + 1) {
                if values.iter().any(|m| m.is_match(value)) {
                    return true;
                }
            }
        } else if let Some(rest) = w.strip_prefix(&format!("{name}=")) {
            if values.iter().any(|m| m.is_match(rest)) {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// コマンド文字列から起動を集める。ラッパーと `bash -c`・`eval` の内側、
/// コマンド置換の内側も展開する（REQ-044 の調査済みの範囲）。
pub fn invocations(command: &str) -> Vec<Invocation> {
    let script = parse::parse_script(command);
    let mut out = Vec::new();
    walk_items(&script.items, &mut out, 0);
    out
}

fn walk_items(items: &[Item], out: &mut Vec<Invocation>, depth: usize) {
    if depth > 16 {
        return;
    }
    for item in items {
        match item {
            Item::Simple(p) => {
                for cmd in &p.commands {
                    walk_command(cmd, out, depth);
                }
            }
            Item::For { words, body, .. } => {
                for w in words {
                    walk_word_subst(w, out, depth);
                }
                walk_items(body, out, depth + 1);
            }
        }
    }
}

fn walk_command(cmd: &SimpleCommand, out: &mut Vec<Invocation>, depth: usize) {
    for w in &cmd.words {
        walk_word_subst(w, out, depth);
    }
    for r in &cmd.redirects {
        let w = match r {
            Redirect::Out(w)
            | Redirect::Append(w)
            | Redirect::Clobber(w)
            | Redirect::In(w)
            | Redirect::Heredoc(w) => w,
        };
        walk_word_subst(w, out, depth);
    }

    let words: Vec<&Word> = cmd.words.iter().collect();
    let mut i = 0;
    let mut env_names = Vec::new();
    while i < words.len() {
        match assignment_name(words[i]) {
            Some((name, value_word)) => {
                env_names.push(name);
                walk_word_subst(value_word, out, depth);
                i += 1;
            }
            None => break,
        }
    }
    loop {
        let Some(first) = words.get(i) else { return };
        let name = basename(&first.text);
        if name == "sudo" || name == "doas" {
            i = strip_wrapper(&words, i, name);
        } else {
            break;
        }
    }
    let Some(first) = words.get(i) else { return };
    let program = basename(&first.text).to_string();
    let args: Vec<&Word> = words[i + 1..].to_vec();

    if matches!(program.as_str(), "bash" | "sh" | "zsh" | "dash" | "ksh") {
        if let Some(inner) = shell_c_string(&args) {
            walk_inner(&inner, out, depth);
        }
        return;
    }
    if program == "eval" {
        let mut pieces = Vec::new();
        for w in &args {
            match w.literal_value() {
                Some(s) => pieces.push(s),
                None => return,
            }
        }
        if !pieces.is_empty() {
            walk_inner(&pieces.join(" "), out, depth);
        }
        return;
    }
    out.push(Invocation {
        program,
        words: args.iter().map(|w| w.text.clone()).collect(),
        env_names,
    });
}

fn walk_inner(inner: &str, out: &mut Vec<Invocation>, depth: usize) {
    let script = parse::parse_script(inner);
    walk_items(&script.items, out, depth + 1);
}

fn walk_word_subst(w: &Word, out: &mut Vec<Invocation>, depth: usize) {
    for part in &w.parts {
        if let Part::Subst(inner) = part {
            walk_inner(inner, out, depth);
        }
    }
}

fn assignment_name(w: &Word) -> Option<(String, &Word)> {
    let first = match w.parts.first() {
        Some(Part::Literal(s)) => s,
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
    Some((name.to_string(), w))
}

const SUDO_VALUED: &[&str] = &[
    "-u", "-g", "-p", "-C", "-h", "-r", "-t", "-U", "-T", "-R", "-D",
];
const DOAS_VALUED: &[&str] = &["-u", "-C", "-a"];

fn strip_wrapper(words: &[&Word], mut i: usize, name: &str) -> usize {
    let valued = if name == "doas" {
        DOAS_VALUED
    } else {
        SUDO_VALUED
    };
    i += 1;
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
    while i < words.len() && assignment_name(words[i]).is_some() {
        i += 1;
    }
    i
}

fn shell_c_string(args: &[&Word]) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        let t = &args[i].text;
        let is_c =
            t == "-c" || (t.starts_with('-') && !t.starts_with("--") && t[1..].contains('c'));
        if is_c {
            return args.get(i + 1).and_then(|w| w.literal_value());
        }
        i += 1;
    }
    None
}

fn basename(text: &str) -> &str {
    text.rsplit('/').next().unwrap_or(text)
}
