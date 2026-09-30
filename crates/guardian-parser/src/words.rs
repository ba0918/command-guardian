//! 語の意味づけ。プログラム名の取り出し、代入の分解、ラッパーの除去、
//! シェル起動の認識（REQ-035）を扱う。

use crate::ast::{Part, Word};

/// 語の basename。
pub fn basename(text: &str) -> &str {
    text.rsplit('/').next().unwrap_or(text)
}

/// `name=value` の語を分解する。分解できないときは None。
pub fn split_assignment(word: &Word) -> Option<(String, Word)> {
    let first = match word.parts.first() {
        Some(Part::Literal(text)) => text.as_str(),
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
    parts.extend(word.parts[1..].iter().cloned());
    Some((name.to_string(), Word::from_parts(parts)))
}

/// 先頭に並ぶ代入の語を分ける。
pub fn split_prefix_assignments(words: &[Word]) -> (Vec<(String, Word)>, &[Word]) {
    let mut index = 0;
    let mut assignments = Vec::new();
    while index < words.len() {
        match split_assignment(&words[index]) {
            Some(pair) => {
                assignments.push(pair);
                index += 1;
            }
            None => break,
        }
    }
    (assignments, &words[index.min(words.len())..])
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

/// 先頭の代入とラッパー（sudo・doas）を外した語の並び。
pub fn strip_wrapper(words: &[Word]) -> &[Word] {
    let mut index = 0;
    loop {
        // 先頭の代入。
        while index < words.len() && split_assignment(&words[index]).is_some() {
            index += 1;
        }
        let Some(first) = words.get(index) else {
            break;
        };
        let name = basename(&first.text);
        if name != "sudo" && name != "doas" {
            break;
        }
        let valued = if name == "doas" {
            DOAS_VALUED
        } else {
            SUDO_VALUED
        };
        index += 1;
        while index < words.len() {
            let text = &words[index].text;
            if text == "--" {
                index += 1;
                break;
            }
            if !text.starts_with('-') || text == "-" {
                break;
            }
            let head = text.split('=').next().unwrap_or(text);
            if valued.contains(&head) && !text.contains('=') {
                index += 2;
            } else {
                index += 1;
            }
        }
        // ラッパー越しの代入。
        while index < words.len() && split_assignment(&words[index]).is_some() {
            index += 1;
        }
    }
    &words[index.min(words.len())..]
}

/// 対象のシェルの種類（REQ-035）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    /// bash と POSIX sh の文法で読むシェル。
    BashLike,
    /// 知っているが読まないシェル。
    NonPosix,
    /// シェルではない（知らないプログラムも含む）。
    Other,
}

/// プログラムの語からシェルの種類を判別する。
pub fn shell_kind(program: &str) -> ShellKind {
    match basename(program) {
        "bash" | "sh" | "dash" | "zsh" | "ksh" | "mksh" => ShellKind::BashLike,
        "fish" | "csh" | "tcsh" | "elvish" | "xonsh" | "nu" | "pwsh" => ShellKind::NonPosix,
        _ => ShellKind::Other,
    }
}

/// `-c` の位置を探す。`-lc` のようなまとめ書きを含む。
pub fn shell_c_index(words: &[Word]) -> Option<usize> {
    words.iter().position(|word| {
        let text = &word.text;
        text == "-c"
            || (text.starts_with('-') && !text.starts_with("--") && text[1..].contains('c'))
    })
}

/// コマンドの起動の種類（REQ-035）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellInvocation {
    /// bash 系の `-c` 起動。本体の語を持つ（本体の語が無いときは None）。
    BashLike { body: Option<Word> },
    /// 知っている非 POSIX 系の起動。
    NonPosix,
    /// シェルの起動ではない。
    Other,
}

/// ラッパーを外した後のプログラムの語で起動を判別する。
pub fn shell_invocation(words: &[Word]) -> ShellInvocation {
    let rest = strip_wrapper(words);
    let Some(program) = rest.first() else {
        return ShellInvocation::Other;
    };
    if program.literal_value().is_none() {
        return ShellInvocation::Other;
    }
    match shell_kind(&program.text) {
        ShellKind::BashLike => {
            let args = &rest[1..];
            let body = shell_c_index(args).and_then(|index| args.get(index + 1).cloned());
            ShellInvocation::BashLike { body }
        }
        ShellKind::NonPosix => ShellInvocation::NonPosix,
        ShellKind::Other => ShellInvocation::Other,
    }
}
