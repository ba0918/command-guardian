//! 親と子のやり取りの符号化。OSS の型は運ばず、正規化した構文木だけを運ぶ。
//!
//! 枠の外側（nonce・種別・長さ）は worker が扱い、ここは本体だけを扱う。

use crate::ast::*;
use crate::Failure;

/// 要求の種別。
pub(crate) const MODE_PARSE: u8 = 0;
pub(crate) const MODE_STRIP_QUOTES: u8 = 1;

/// 子からの応答。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Response {
    /// 解析の結果。深さが上限を超えたときは script が None（失敗は TooDeep）。
    Parsed {
        failures: Vec<Failure>,
        script: Option<Script>,
    },
    /// 引用を外したコマンド本文。
    Stripped(String),
}

/// 応答の本体を符号化する。
pub(crate) fn encode_response(response: &Response) -> Vec<u8> {
    let mut w = Writer::new();
    match response {
        Response::Parsed { failures, script } => {
            w.u8(0);
            w.count(failures.len());
            for failure in failures {
                failure_write(&mut w, failure);
            }
            match script {
                Some(script) => {
                    w.bool(true);
                    script_write(&mut w, script);
                }
                None => w.bool(false),
            }
        }
        Response::Stripped(text) => {
            w.u8(1);
            w.str(text);
        }
    }
    w.out
}

/// 応答の本体を復号する。
pub(crate) fn decode_response(bytes: &[u8]) -> Result<Response, ()> {
    let mut r = Reader::new(bytes);
    match r.u8()? {
        0 => {
            let count = r.count(MAX_ITEMS)?;
            let mut failures = Vec::with_capacity(count.min(MAX_PREALLOC));
            for _ in 0..count {
                failures.push(failure_read(&mut r)?);
            }
            let script = if r.bool()? {
                Some(script_read(&mut r)?)
            } else {
                None
            };
            r.finish()?;
            Ok(Response::Parsed { failures, script })
        }
        1 => {
            let text = r.str()?;
            r.finish()?;
            Ok(Response::Stripped(text))
        }
        _ => Err(()),
    }
}

/// 要素数の上限。壊れた枠で巨大な領域を取らないための歯止め。
const MAX_ITEMS: usize = 64 * 1024 * 1024;
/// 復号の最初に確保する要素数。残りは伸ばしながら読む。
const MAX_PREALLOC: usize = 4096;
/// 文字列の上限。
const MAX_STRING: usize = 256 * 1024 * 1024;

struct Writer {
    out: Vec<u8>,
}

impl Writer {
    fn new() -> Writer {
        Writer { out: Vec::new() }
    }

    fn u8(&mut self, value: u8) {
        self.out.push(value);
    }

    fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    fn u32(&mut self, value: u32) {
        self.out.extend_from_slice(&value.to_le_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.out.extend_from_slice(&value.to_le_bytes());
    }

    fn count(&mut self, value: usize) {
        self.u32(value as u32);
    }

    fn str(&mut self, value: &str) {
        self.u32(value.len() as u32);
        self.out.extend_from_slice(value.as_bytes());
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Reader<'a> {
        Reader { bytes, at: 0 }
    }

    fn u8(&mut self) -> Result<u8, ()> {
        let value = *self.bytes.get(self.at).ok_or(())?;
        self.at += 1;
        Ok(value)
    }

    fn bool(&mut self) -> Result<bool, ()> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(()),
        }
    }

    fn u32(&mut self) -> Result<u32, ()> {
        let bytes = self.bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn i32(&mut self) -> Result<i32, ()> {
        let bytes = self.bytes(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn count(&mut self, max: usize) -> Result<usize, ()> {
        let value = self.u32()? as usize;
        if value > max {
            return Err(());
        }
        Ok(value)
    }

    fn str(&mut self) -> Result<String, ()> {
        let len = self.count(MAX_STRING)?;
        let bytes = self.bytes(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| ())
    }

    fn bytes(&mut self, len: usize) -> Result<&'a [u8], ()> {
        let end = self.at.checked_add(len).ok_or(())?;
        let slice = self.bytes.get(self.at..end).ok_or(())?;
        self.at = end;
        Ok(slice)
    }

    /// 末尾まで読み切ったか。
    fn finish(self) -> Result<(), ()> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(())
        }
    }
}

fn failure_write(w: &mut Writer, failure: &Failure) {
    match failure {
        Failure::TooLarge => w.u8(0),
        Failure::TooDeep => w.u8(1),
        Failure::Syntax => w.u8(2),
        Failure::UnknownNode(text) => {
            w.u8(3);
            w.str(text);
        }
        Failure::Panic => w.u8(4),
        Failure::Limit => w.u8(5),
        Failure::Internal => w.u8(6),
    }
}

fn failure_read(r: &mut Reader) -> Result<Failure, ()> {
    match r.u8()? {
        0 => Ok(Failure::TooLarge),
        1 => Ok(Failure::TooDeep),
        2 => Ok(Failure::Syntax),
        3 => Ok(Failure::UnknownNode(r.str()?)),
        4 => Ok(Failure::Panic),
        5 => Ok(Failure::Limit),
        6 => Ok(Failure::Internal),
        _ => Err(()),
    }
}

fn script_write(w: &mut Writer, script: &Script) {
    w.count(script.items.len());
    for item in &script.items {
        item_write(w, item);
    }
}

fn script_read(r: &mut Reader) -> Result<Script, ()> {
    let count = r.count(MAX_ITEMS)?;
    let mut items = Vec::with_capacity(count.min(MAX_PREALLOC));
    for _ in 0..count {
        items.push(item_read(r)?);
    }
    Ok(Script { items })
}

fn item_write(w: &mut Writer, item: &Item) {
    pipeline_write(w, &item.first);
    w.count(item.rest.len());
    for (op, pipeline) in &item.rest {
        w.u8(match op {
            AndOrOp::And => 0,
            AndOrOp::Or => 1,
        });
        pipeline_write(w, pipeline);
    }
    w.u8(match item.separator {
        Separator::Sequence => 0,
        Separator::Async => 1,
    });
}

fn item_read(r: &mut Reader) -> Result<Item, ()> {
    let first = pipeline_read(r)?;
    let count = r.count(MAX_ITEMS)?;
    let mut rest = Vec::with_capacity(count.min(MAX_PREALLOC));
    for _ in 0..count {
        let op = match r.u8()? {
            0 => AndOrOp::And,
            1 => AndOrOp::Or,
            _ => return Err(()),
        };
        rest.push((op, pipeline_read(r)?));
    }
    let separator = match r.u8()? {
        0 => Separator::Sequence,
        1 => Separator::Async,
        _ => return Err(()),
    };
    Ok(Item {
        first,
        rest,
        separator,
    })
}

fn pipeline_write(w: &mut Writer, pipeline: &Pipeline) {
    w.count(pipeline.commands.len());
    for command in &pipeline.commands {
        command_write(w, command);
    }
}

fn pipeline_read(r: &mut Reader) -> Result<Pipeline, ()> {
    let count = r.count(MAX_ITEMS)?;
    let mut commands = Vec::with_capacity(count.min(MAX_PREALLOC));
    for _ in 0..count {
        commands.push(command_read(r)?);
    }
    Ok(Pipeline { commands })
}

fn command_write(w: &mut Writer, command: &Command) {
    match command {
        Command::Simple(simple) => {
            w.u8(0);
            simple_write(w, simple);
        }
        Command::Compound {
            compound,
            redirects,
        } => {
            w.u8(1);
            compound_write(w, compound);
            redirects_write(w, redirects);
        }
        Command::Function(function) => {
            w.u8(2);
            word_write(w, &function.name);
            compound_write(w, &function.body);
            redirects_write(w, &function.redirects);
        }
        Command::Test(test) => {
            w.u8(3);
            words_write(w, &test.words);
            redirects_write(w, &test.redirects);
        }
    }
}

fn command_read(r: &mut Reader) -> Result<Command, ()> {
    match r.u8()? {
        0 => Ok(Command::Simple(simple_read(r)?)),
        1 => {
            let compound = compound_read(r)?;
            let redirects = redirects_read(r)?;
            Ok(Command::Compound {
                compound,
                redirects,
            })
        }
        2 => {
            let name = word_read(r)?;
            let body = Box::new(compound_read(r)?);
            let redirects = redirects_read(r)?;
            Ok(Command::Function(Function {
                name,
                body,
                redirects,
            }))
        }
        3 => {
            let words = words_read(r)?;
            let redirects = redirects_read(r)?;
            Ok(Command::Test(TestCommand { words, redirects }))
        }
        _ => Err(()),
    }
}

fn simple_write(w: &mut Writer, simple: &SimpleCommand) {
    words_write(w, &simple.words);
    redirects_write(w, &simple.redirects);
    w.count(simple.process_substitutions.len());
    for substitution in &simple.process_substitutions {
        process_substitution_write(w, substitution);
    }
}

fn simple_read(r: &mut Reader) -> Result<SimpleCommand, ()> {
    let words = words_read(r)?;
    let redirects = redirects_read(r)?;
    let count = r.count(MAX_ITEMS)?;
    let mut process_substitutions = Vec::with_capacity(count.min(MAX_PREALLOC));
    for _ in 0..count {
        process_substitutions.push(process_substitution_read(r)?);
    }
    Ok(SimpleCommand {
        words,
        redirects,
        process_substitutions,
    })
}

fn compound_write(w: &mut Writer, compound: &Compound) {
    match compound {
        Compound::If {
            condition,
            then,
            elses,
        } => {
            w.u8(0);
            script_write(w, condition);
            script_write(w, then);
            w.count(elses.len());
            for clause in elses {
                match &clause.condition {
                    Some(condition) => {
                        w.bool(true);
                        script_write(w, condition);
                    }
                    None => w.bool(false),
                }
                script_write(w, &clause.body);
            }
        }
        Compound::While {
            condition,
            body,
            until,
        } => {
            w.u8(1);
            script_write(w, condition);
            script_write(w, body);
            w.bool(*until);
        }
        Compound::For { var, values, body } => {
            w.u8(2);
            w.str(var);
            words_write(w, values);
            script_write(w, body);
        }
        Compound::ArithmeticFor {
            initializer,
            condition,
            updater,
            body,
        } => {
            w.u8(3);
            option_word_write(w, initializer.as_ref());
            option_word_write(w, condition.as_ref());
            option_word_write(w, updater.as_ref());
            script_write(w, body);
        }
        Compound::Case { value, arms } => {
            w.u8(4);
            word_write(w, value);
            w.count(arms.len());
            for arm in arms {
                words_write(w, &arm.patterns);
                match &arm.body {
                    Some(body) => {
                        w.bool(true);
                        script_write(w, body);
                    }
                    None => w.bool(false),
                }
            }
        }
        Compound::BraceGroup(script) => {
            w.u8(5);
            script_write(w, script);
        }
        Compound::Subshell(script) => {
            w.u8(6);
            script_write(w, script);
        }
        Compound::Arithmetic(word) => {
            w.u8(7);
            word_write(w, word);
        }
        Compound::Coprocess { name, body } => {
            w.u8(8);
            option_word_write(w, name.as_ref());
            command_write(w, body);
        }
    }
}

fn compound_read(r: &mut Reader) -> Result<Compound, ()> {
    match r.u8()? {
        0 => {
            let condition = script_read(r)?;
            let then = script_read(r)?;
            let count = r.count(MAX_ITEMS)?;
            let mut elses = Vec::with_capacity(count.min(MAX_PREALLOC));
            for _ in 0..count {
                let condition = if r.bool()? {
                    Some(script_read(r)?)
                } else {
                    None
                };
                let body = script_read(r)?;
                elses.push(Else { condition, body });
            }
            Ok(Compound::If {
                condition,
                then,
                elses,
            })
        }
        1 => {
            let condition = script_read(r)?;
            let body = script_read(r)?;
            let until = r.bool()?;
            Ok(Compound::While {
                condition,
                body,
                until,
            })
        }
        2 => {
            let var = r.str()?;
            let values = words_read(r)?;
            let body = script_read(r)?;
            Ok(Compound::For { var, values, body })
        }
        3 => {
            let initializer = option_word_read(r)?;
            let condition = option_word_read(r)?;
            let updater = option_word_read(r)?;
            let body = script_read(r)?;
            Ok(Compound::ArithmeticFor {
                initializer,
                condition,
                updater,
                body,
            })
        }
        4 => {
            let value = word_read(r)?;
            let count = r.count(MAX_ITEMS)?;
            let mut arms = Vec::with_capacity(count.min(MAX_PREALLOC));
            for _ in 0..count {
                let patterns = words_read(r)?;
                let body = if r.bool()? {
                    Some(script_read(r)?)
                } else {
                    None
                };
                arms.push(CaseArm { patterns, body });
            }
            Ok(Compound::Case { value, arms })
        }
        5 => Ok(Compound::BraceGroup(script_read(r)?)),
        6 => Ok(Compound::Subshell(script_read(r)?)),
        7 => Ok(Compound::Arithmetic(word_read(r)?)),
        8 => {
            let name = option_word_read(r)?;
            let body = Box::new(command_read(r)?);
            Ok(Compound::Coprocess { name, body })
        }
        _ => Err(()),
    }
}

fn words_write(w: &mut Writer, words: &[Word]) {
    w.count(words.len());
    for word in words {
        word_write(w, word);
    }
}

fn words_read(r: &mut Reader) -> Result<Vec<Word>, ()> {
    let count = r.count(MAX_ITEMS)?;
    let mut words = Vec::with_capacity(count.min(MAX_PREALLOC));
    for _ in 0..count {
        words.push(word_read(r)?);
    }
    Ok(words)
}

fn option_word_write(w: &mut Writer, word: Option<&Word>) {
    match word {
        Some(word) => {
            w.bool(true);
            word_write(w, word);
        }
        None => w.bool(false),
    }
}

fn option_word_read(r: &mut Reader) -> Result<Option<Word>, ()> {
    if r.bool()? {
        Ok(Some(word_read(r)?))
    } else {
        Ok(None)
    }
}

fn word_write(w: &mut Writer, word: &Word) {
    w.count(word.parts.len());
    for part in &word.parts {
        part_write(w, part);
    }
}

fn word_read(r: &mut Reader) -> Result<Word, ()> {
    let count = r.count(MAX_ITEMS)?;
    let mut parts = Vec::with_capacity(count.min(MAX_PREALLOC));
    for _ in 0..count {
        parts.push(part_read(r)?);
    }
    Ok(Word::from_parts(parts))
}

fn part_write(w: &mut Writer, part: &Part) {
    match part {
        Part::Literal(text) => {
            w.u8(0);
            w.str(text);
        }
        Part::Quoted(text) => {
            w.u8(1);
            w.str(text);
        }
        Part::Var(name) => {
            w.u8(2);
            w.str(name);
        }
        Part::Substitution(substitution) => {
            w.u8(3);
            substitution_write(w, substitution);
        }
        Part::Opaque(text) => {
            w.u8(4);
            w.str(text);
        }
        Part::Glob(text) => {
            w.u8(5);
            w.str(text);
        }
    }
}

fn part_read(r: &mut Reader) -> Result<Part, ()> {
    match r.u8()? {
        0 => Ok(Part::Literal(r.str()?)),
        1 => Ok(Part::Quoted(r.str()?)),
        2 => Ok(Part::Var(r.str()?)),
        3 => Ok(Part::Substitution(substitution_read(r)?)),
        4 => Ok(Part::Opaque(r.str()?)),
        5 => Ok(Part::Glob(r.str()?)),
        _ => Err(()),
    }
}

fn substitution_write(w: &mut Writer, substitution: &Substitution) {
    w.str(&substitution.body_text);
    match &substitution.body {
        Some(body) => {
            w.bool(true);
            script_write(w, body);
        }
        None => w.bool(false),
    }
}

fn substitution_read(r: &mut Reader) -> Result<Substitution, ()> {
    let body_text = r.str()?;
    let body = if r.bool()? {
        Some(Box::new(script_read(r)?))
    } else {
        None
    };
    Ok(Substitution { body_text, body })
}

fn redirects_write(w: &mut Writer, redirects: &[Redirect]) {
    w.count(redirects.len());
    for redirect in redirects {
        redirect_write(w, redirect);
    }
}

fn redirects_read(r: &mut Reader) -> Result<Vec<Redirect>, ()> {
    let count = r.count(MAX_ITEMS)?;
    let mut redirects = Vec::with_capacity(count.min(MAX_PREALLOC));
    for _ in 0..count {
        redirects.push(redirect_read(r)?);
    }
    Ok(redirects)
}

fn redirect_write(w: &mut Writer, redirect: &Redirect) {
    match redirect.kind {
        RedirectKind::OutputAndError(append) => {
            w.u8(9);
            w.bool(append);
        }
        other => {
            w.u8(match other {
                RedirectKind::Read => 0,
                RedirectKind::Write => 1,
                RedirectKind::Append => 2,
                RedirectKind::ReadWrite => 3,
                RedirectKind::Clobber => 4,
                RedirectKind::DuplicateInput => 5,
                RedirectKind::DuplicateOutput => 6,
                RedirectKind::HereDocument => 7,
                RedirectKind::HereString => 8,
                RedirectKind::OutputAndError(_) => unreachable!(),
            });
        }
    }
    match redirect.fd {
        Some(fd) => {
            w.bool(true);
            w.i32(fd);
        }
        None => w.bool(false),
    }
    match &redirect.target {
        RedirectTarget::Word(word) => {
            w.u8(0);
            word_write(w, word);
        }
        RedirectTarget::Fd(fd) => {
            w.u8(1);
            w.i32(*fd);
        }
        RedirectTarget::ProcessSubstitution(substitution) => {
            w.u8(2);
            process_substitution_write(w, substitution);
        }
        RedirectTarget::HereDocument { end, doc, expand } => {
            w.u8(3);
            word_write(w, end);
            word_write(w, doc);
            w.bool(*expand);
        }
    }
}

fn redirect_read(r: &mut Reader) -> Result<Redirect, ()> {
    let kind = match r.u8()? {
        0 => RedirectKind::Read,
        1 => RedirectKind::Write,
        2 => RedirectKind::Append,
        3 => RedirectKind::ReadWrite,
        4 => RedirectKind::Clobber,
        5 => RedirectKind::DuplicateInput,
        6 => RedirectKind::DuplicateOutput,
        7 => RedirectKind::HereDocument,
        8 => RedirectKind::HereString,
        9 => RedirectKind::OutputAndError(r.bool()?),
        _ => return Err(()),
    };
    let fd = if r.bool()? { Some(r.i32()?) } else { None };
    let target = match r.u8()? {
        0 => RedirectTarget::Word(word_read(r)?),
        1 => RedirectTarget::Fd(r.i32()?),
        2 => RedirectTarget::ProcessSubstitution(process_substitution_read(r)?),
        3 => {
            let end = word_read(r)?;
            let doc = word_read(r)?;
            let expand = r.bool()?;
            RedirectTarget::HereDocument { end, doc, expand }
        }
        _ => return Err(()),
    };
    Ok(Redirect { kind, fd, target })
}

fn process_substitution_write(w: &mut Writer, substitution: &ProcessSubstitution) {
    w.bool(substitution.write);
    script_write(w, &substitution.body);
}

fn process_substitution_read(r: &mut Reader) -> Result<ProcessSubstitution, ()> {
    let write = r.bool()?;
    let body = script_read(r)?;
    Ok(ProcessSubstitution { write, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Response {
        Response::Parsed {
            failures: vec![
                Failure::TooLarge,
                Failure::TooDeep,
                Failure::Syntax,
                Failure::UnknownNode("x".to_string()),
                Failure::Panic,
                Failure::Internal,
                Failure::Limit,
            ],
            script: Some(Script {
                items: vec![Item {
                    first: Pipeline {
                        commands: vec![
                            Command::Simple(SimpleCommand {
                                words: vec![
                                    Word::from_parts(vec![Part::Literal("rm".to_string())]),
                                    Word::from_parts(vec![
                                        Part::Quoted("-rf".to_string()),
                                        Part::Glob("*".to_string()),
                                    ]),
                                    Word::from_parts(vec![Part::Var("HOME".to_string())]),
                                    Word::from_parts(vec![Part::Substitution(Substitution {
                                        body_text: "echo x".to_string(),
                                        body: Some(Box::new(Script::default())),
                                    })]),
                                ],
                                redirects: vec![Redirect {
                                    kind: RedirectKind::OutputAndError(true),
                                    fd: Some(2),
                                    target: RedirectTarget::Word(Word::from_parts(vec![
                                        Part::Literal("/dev/null".to_string()),
                                    ])),
                                }],
                                process_substitutions: vec![ProcessSubstitution {
                                    write: true,
                                    body: Script::default(),
                                }],
                            }),
                            Command::Compound {
                                compound: Compound::If {
                                    condition: Script::default(),
                                    then: Script::default(),
                                    elses: vec![Else {
                                        condition: Some(Script::default()),
                                        body: Script::default(),
                                    }],
                                },
                                redirects: Vec::new(),
                            },
                            Command::Function(Function {
                                name: Word::from_parts(vec![Part::Literal("f".to_string())]),
                                body: Box::new(Compound::Subshell(Script::default())),
                                redirects: Vec::new(),
                            }),
                            Command::Test(TestCommand {
                                words: vec![Word::from_parts(vec![Part::Opaque(
                                    "$((".to_string(),
                                )])],
                                redirects: Vec::new(),
                            }),
                        ],
                    },
                    rest: vec![(AndOrOp::Or, Pipeline::default())],
                    separator: Separator::Async,
                }],
            }),
        }
    }

    // @kotowari[REQ-039]
    #[test]
    fn req_039_a_response_roundtrips() {
        let response = sample();
        let bytes = encode_response(&response);
        assert_eq!(decode_response(&bytes), Ok(response));
    }

    // @kotowari[REQ-039]
    #[test]
    fn req_039_a_truncated_response_is_rejected() {
        let bytes = encode_response(&sample());
        for cut in 0..bytes.len() {
            assert!(decode_response(&bytes[..cut]).is_err(), "{cut}");
        }
    }
}
