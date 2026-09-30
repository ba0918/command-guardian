//! OSS パーサの構文木を、正規化した構文木へ写す。すべての位置を 1 回ずつ訪問する。

use crate::ast::*;
use crate::{catch, Failure, LIMIT_DEPTH};
use brush_parser::ast as raw;
use brush_parser::{word, ParserOptions};

/// 字句解析と構文解析の設定。対象は bash と POSIX sh（REQ-035）。
pub(crate) fn options() -> ParserOptions {
    ParserOptions::default()
}

/// 生の字句解析。panic は Failure::Panic に変える。
pub(crate) fn tokenize(
    input: &str,
    options: &ParserOptions,
) -> Result<Vec<brush_parser::Token>, Failure> {
    match catch(|| brush_parser::uncached_tokenize_str(input, &options.tokenizer_options())) {
        Ok(Ok(tokens)) => Ok(tokens),
        Ok(Err(_)) => Err(Failure::Syntax),
        Err(()) => Err(Failure::Panic),
    }
}

/// 生の構文解析。panic は Failure::Panic に変える。
pub(crate) fn parse_tokens(
    tokens: &[brush_parser::Token],
    options: &ParserOptions,
) -> Result<raw::Program, Failure> {
    match catch(|| brush_parser::parse_tokens(tokens, options)) {
        Ok(Ok(program)) => Ok(program),
        Ok(Err(_)) => Err(Failure::Syntax),
        Err(()) => Err(Failure::Panic),
    }
}

/// 文字列をそのまま構文解析する。
pub(crate) fn parse_program(input: &str, options: &ParserOptions) -> Result<raw::Program, Failure> {
    let tokens = tokenize(input, options)?;
    parse_tokens(&tokens, options)
}

/// 正規化の途中で見つけた失敗を集めながら、構文木を写す。
pub(crate) struct Normalizer {
    options: ParserOptions,
    /// 置換の再帰読みを含む累計の残り（REQ-039）。
    pub(crate) budget: usize,
    pub(crate) failures: Vec<Failure>,
}

impl Normalizer {
    pub(crate) fn new(budget: usize) -> Normalizer {
        Normalizer {
            options: options(),
            budget,
            failures: Vec::new(),
        }
    }

    /// コマンド列を写す。読めなかった項目は飛ばし、読めた項目だけを残す（REQ-038）。
    pub(crate) fn script(&mut self, list: &raw::CompoundList, depth: usize) -> Script {
        let mut items = Vec::new();
        for item in &list.0 {
            match self.item(item, depth) {
                Ok(item) => items.push(item),
                Err(failure) => self.failures.push(failure),
            }
        }
        Script { items }
    }

    /// スクリプト全体を写す。
    pub(crate) fn program(&mut self, program: &raw::Program, depth: usize) -> Script {
        let mut script = Script::default();
        for complete in &program.complete_commands {
            script.items.extend(self.script(complete, depth).items);
        }
        script
    }

    fn item(&mut self, item: &raw::CompoundListItem, depth: usize) -> Result<Item, Failure> {
        if depth > LIMIT_DEPTH {
            return Err(Failure::TooDeep);
        }
        let first = self.pipeline(&item.0.first, depth)?;
        let mut rest = Vec::new();
        for and_or in &item.0.additional {
            match and_or {
                raw::AndOr::And(pipeline) => {
                    rest.push((AndOrOp::And, self.pipeline(pipeline, depth)?))
                }
                raw::AndOr::Or(pipeline) => {
                    rest.push((AndOrOp::Or, self.pipeline(pipeline, depth)?))
                }
            }
        }
        let separator = match item.1 {
            raw::SeparatorOperator::Async => Separator::Async,
            raw::SeparatorOperator::Sequence => Separator::Sequence,
        };
        Ok(Item {
            first,
            rest,
            separator,
        })
    }

    fn pipeline(&mut self, pipeline: &raw::Pipeline, depth: usize) -> Result<Pipeline, Failure> {
        if depth > LIMIT_DEPTH {
            return Err(Failure::TooDeep);
        }
        let mut commands = Vec::new();
        for command in &pipeline.seq {
            commands.push(self.command(command, depth)?);
        }
        Ok(Pipeline { commands })
    }

    fn command(&mut self, command: &raw::Command, depth: usize) -> Result<Command, Failure> {
        if depth > LIMIT_DEPTH {
            return Err(Failure::TooDeep);
        }
        match command {
            raw::Command::Simple(simple) => Ok(Command::Simple(self.simple(simple, depth)?)),
            raw::Command::Compound(compound, redirects) => {
                let redirects = self.redirects(redirects.as_ref(), depth)?;
                let compound = self.compound(compound, depth)?;
                Ok(Command::Compound {
                    compound,
                    redirects,
                })
            }
            raw::Command::Function(function) => {
                let name = self.word(&function.fname, depth)?;
                let body = self.compound(&function.body.0, depth)?;
                let redirects = self.redirects(function.body.1.as_ref(), depth)?;
                Ok(Command::Function(Function {
                    name,
                    body: Box::new(body),
                    redirects,
                }))
            }
            raw::Command::ExtendedTest(test, redirects) => {
                let mut words = Vec::new();
                self.test_expr(&test.expr, depth, &mut words)?;
                let redirects = self.redirects(redirects.as_ref(), depth)?;
                Ok(Command::Test(TestCommand { words, redirects }))
            }
        }
    }

    fn simple(
        &mut self,
        simple: &raw::SimpleCommand,
        depth: usize,
    ) -> Result<SimpleCommand, Failure> {
        if depth > LIMIT_DEPTH {
            return Err(Failure::TooDeep);
        }
        let mut out = SimpleCommand::default();
        if let Some(prefix) = &simple.prefix {
            for item in &prefix.0 {
                self.simple_item(item, depth, &mut out)?;
            }
        }
        if let Some(name) = &simple.word_or_name {
            out.words.push(self.word(name, depth)?);
        }
        if let Some(suffix) = &simple.suffix {
            for item in &suffix.0 {
                self.simple_item(item, depth, &mut out)?;
            }
        }
        Ok(out)
    }

    fn simple_item(
        &mut self,
        item: &raw::CommandPrefixOrSuffixItem,
        depth: usize,
        out: &mut SimpleCommand,
    ) -> Result<(), Failure> {
        match item {
            raw::CommandPrefixOrSuffixItem::Word(word) => out.words.push(self.word(word, depth)?),
            raw::CommandPrefixOrSuffixItem::AssignmentWord(assignment, word) => out
                .words
                .push(self.assignment_word(assignment, word, depth)?),
            raw::CommandPrefixOrSuffixItem::IoRedirect(redirect) => {
                out.redirects.push(self.redirect(redirect, depth)?)
            }
            raw::CommandPrefixOrSuffixItem::ProcessSubstitution(kind, subshell) => {
                let write = matches!(kind, raw::ProcessSubstitutionKind::Write);
                let body = self.script(&subshell.list, depth + 1);
                out.process_substitutions
                    .push(ProcessSubstitution { write, body });
            }
        }
        Ok(())
    }

    /// 代入の語。配列の代入は値の形を取れないため、不透明な語として持つ。
    fn assignment_word(
        &mut self,
        assignment: &raw::Assignment,
        word: &raw::Word,
        depth: usize,
    ) -> Result<Word, Failure> {
        if matches!(assignment.value, raw::AssignmentValue::Array(_)) {
            return Ok(Word::from_parts(vec![Part::Opaque(word.value.clone())]));
        }
        self.word(word, depth)
    }

    fn compound(
        &mut self,
        compound: &raw::CompoundCommand,
        depth: usize,
    ) -> Result<Compound, Failure> {
        if depth > LIMIT_DEPTH {
            return Err(Failure::TooDeep);
        }
        let inner = depth + 1;
        match compound {
            raw::CompoundCommand::Arithmetic(arithmetic) => {
                Ok(Compound::Arithmetic(arithmetic.expr.value.clone()))
            }
            raw::CompoundCommand::ArithmeticForClause(clause) => Ok(Compound::ArithmeticFor {
                initializer: clause.initializer.as_ref().map(|e| e.value.clone()),
                condition: clause.condition.as_ref().map(|e| e.value.clone()),
                updater: clause.updater.as_ref().map(|e| e.value.clone()),
                body: self.script(&clause.body.list, inner),
            }),
            raw::CompoundCommand::BraceGroup(group) => {
                Ok(Compound::BraceGroup(self.script(&group.list, inner)))
            }
            raw::CompoundCommand::Subshell(subshell) => {
                Ok(Compound::Subshell(self.script(&subshell.list, inner)))
            }
            raw::CompoundCommand::ForClause(clause) => {
                let mut values = Vec::new();
                if let Some(words) = &clause.values {
                    for word in words {
                        values.push(self.word(word, inner)?);
                    }
                }
                Ok(Compound::For {
                    var: clause.variable_name.clone(),
                    values,
                    body: self.script(&clause.body.list, inner),
                })
            }
            raw::CompoundCommand::CaseClause(clause) => {
                let value = self.word(&clause.value, inner)?;
                let mut arms = Vec::new();
                for item in &clause.cases {
                    let mut patterns = Vec::new();
                    for pattern in &item.patterns {
                        patterns.push(self.word(pattern, inner)?);
                    }
                    let body = item.cmd.as_ref().map(|list| self.script(list, inner));
                    arms.push(CaseArm { patterns, body });
                }
                Ok(Compound::Case { value, arms })
            }
            raw::CompoundCommand::IfClause(clause) => {
                let condition = self.script(&clause.condition, inner);
                let then = self.script(&clause.then, inner);
                let mut elses = Vec::new();
                if let Some(list) = &clause.elses {
                    for clause in list {
                        let condition = clause
                            .condition
                            .as_ref()
                            .map(|list| self.script(list, inner));
                        elses.push(Else {
                            condition,
                            body: self.script(&clause.body, inner),
                        });
                    }
                }
                Ok(Compound::If {
                    condition,
                    then,
                    elses,
                })
            }
            raw::CompoundCommand::WhileClause(clause) => Ok(Compound::While {
                condition: self.script(&clause.0, inner),
                body: self.script(&clause.1.list, inner),
                until: false,
            }),
            raw::CompoundCommand::UntilClause(clause) => Ok(Compound::While {
                condition: self.script(&clause.0, inner),
                body: self.script(&clause.1.list, inner),
                until: true,
            }),
            raw::CompoundCommand::Coprocess(coprocess) => {
                let name = match &coprocess.name {
                    Some(name) => Some(self.word(name, inner)?),
                    None => None,
                };
                let body = self.command(&coprocess.body, inner)?;
                Ok(Compound::Coprocess {
                    name,
                    body: Box::new(body),
                })
            }
        }
    }

    fn test_expr(
        &mut self,
        expr: &raw::ExtendedTestExpr,
        depth: usize,
        out: &mut Vec<Word>,
    ) -> Result<(), Failure> {
        match expr {
            raw::ExtendedTestExpr::And(left, right) | raw::ExtendedTestExpr::Or(left, right) => {
                self.test_expr(left, depth, out)?;
                self.test_expr(right, depth, out)
            }
            raw::ExtendedTestExpr::Not(inner) | raw::ExtendedTestExpr::Parenthesized(inner) => {
                self.test_expr(inner, depth, out)
            }
            raw::ExtendedTestExpr::UnaryTest(_, word) => {
                out.push(self.word(word, depth)?);
                Ok(())
            }
            raw::ExtendedTestExpr::BinaryTest(_, left, right) => {
                out.push(self.word(left, depth)?);
                out.push(self.word(right, depth)?);
                Ok(())
            }
        }
    }

    fn redirects(
        &mut self,
        redirects: Option<&raw::RedirectList>,
        depth: usize,
    ) -> Result<Vec<Redirect>, Failure> {
        let mut out = Vec::new();
        if let Some(list) = redirects {
            for redirect in &list.0 {
                out.push(self.redirect(redirect, depth)?);
            }
        }
        Ok(out)
    }

    fn redirect(&mut self, redirect: &raw::IoRedirect, depth: usize) -> Result<Redirect, Failure> {
        let (kind, fd, target) = match redirect {
            raw::IoRedirect::File(fd, kind, target) => {
                let kind = match kind {
                    raw::IoFileRedirectKind::Read => RedirectKind::Read,
                    raw::IoFileRedirectKind::Write => RedirectKind::Write,
                    raw::IoFileRedirectKind::Append => RedirectKind::Append,
                    raw::IoFileRedirectKind::ReadAndWrite => RedirectKind::ReadWrite,
                    raw::IoFileRedirectKind::Clobber => RedirectKind::Clobber,
                    raw::IoFileRedirectKind::DuplicateInput => RedirectKind::DuplicateInput,
                    raw::IoFileRedirectKind::DuplicateOutput => RedirectKind::DuplicateOutput,
                };
                let target = match target {
                    raw::IoFileRedirectTarget::Filename(word) => {
                        RedirectTarget::Word(self.word(word, depth)?)
                    }
                    raw::IoFileRedirectTarget::Fd(fd) => RedirectTarget::Fd(*fd),
                    raw::IoFileRedirectTarget::Duplicate(word) => {
                        RedirectTarget::Word(self.word(word, depth)?)
                    }
                    raw::IoFileRedirectTarget::ProcessSubstitution(kind, subshell) => {
                        let write = matches!(kind, raw::ProcessSubstitutionKind::Write);
                        RedirectTarget::ProcessSubstitution(ProcessSubstitution {
                            write,
                            body: self.script(&subshell.list, depth + 1),
                        })
                    }
                };
                (kind, *fd, target)
            }
            raw::IoRedirect::HereDocument(fd, here) => {
                // 区切りの語は展開しない。本文は展開するときだけ読む。
                let end = Word::from_parts(vec![Part::Quoted(here.here_end.value.clone())]);
                let doc = self.heredoc_word(&here.doc, here.requires_expansion, depth)?;
                (
                    RedirectKind::HereDocument,
                    *fd,
                    RedirectTarget::HereDocument {
                        end,
                        doc,
                        expand: here.requires_expansion,
                    },
                )
            }
            raw::IoRedirect::HereString(fd, word) => (
                RedirectKind::HereString,
                *fd,
                RedirectTarget::Word(self.word(word, depth)?),
            ),
            raw::IoRedirect::OutputAndError(word, append) => (
                RedirectKind::OutputAndError(*append),
                None,
                RedirectTarget::Word(self.word(word, depth)?),
            ),
        };
        Ok(Redirect { kind, fd, target })
    }

    /// ヒアドキュメントの本文。展開するときだけ置換を読む。
    fn heredoc_word(
        &mut self,
        doc: &raw::Word,
        expand: bool,
        depth: usize,
    ) -> Result<Word, Failure> {
        if !expand {
            return Ok(Word::from_parts(vec![Part::Quoted(doc.value.clone())]));
        }
        if depth > LIMIT_DEPTH {
            return Err(Failure::TooDeep);
        }
        let pieces = match catch(|| word::parse_heredoc(&doc.value, &self.options)) {
            Ok(Ok(pieces)) => pieces,
            Ok(Err(_)) => return Err(Failure::UnknownNode(doc.value.clone())),
            Err(()) => return Err(Failure::Panic),
        };
        let mut parts = Vec::new();
        for piece in &pieces {
            self.word_piece(piece, &doc.value, depth, &mut parts)?;
        }
        Ok(Word::from_parts(parts))
    }

    fn word(&mut self, word: &raw::Word, depth: usize) -> Result<Word, Failure> {
        if depth > LIMIT_DEPTH {
            return Err(Failure::TooDeep);
        }
        let pieces = match catch(|| word::parse(&word.value, &self.options)) {
            Ok(Ok(pieces)) => pieces,
            Ok(Err(_)) => return Err(Failure::UnknownNode(word.value.clone())),
            Err(()) => return Err(Failure::Panic),
        };
        let mut parts = Vec::new();
        for piece in &pieces {
            self.word_piece(piece, &word.value, depth, &mut parts)?;
        }
        Ok(Word::from_parts(parts))
    }

    fn word_piece(
        &mut self,
        piece: &word::WordPieceWithSource,
        source: &str,
        depth: usize,
        parts: &mut Vec<Part>,
    ) -> Result<(), Failure> {
        match &piece.piece {
            word::WordPiece::Text(text) => {
                push_text(parts, text, false);
                Ok(())
            }
            word::WordPiece::SingleQuotedText(text) | word::WordPiece::AnsiCQuotedText(text) => {
                parts.push(Part::Quoted(text.clone()));
                Ok(())
            }
            word::WordPiece::DoubleQuotedSequence(pieces)
            | word::WordPiece::GettextDoubleQuotedSequence(pieces) => {
                for inner in pieces {
                    self.double_quoted_piece(inner, source, depth, parts)?;
                }
                Ok(())
            }
            word::WordPiece::TildeExpansion(tilde) => {
                parts.push(Part::Literal(tilde_spelling(tilde)));
                Ok(())
            }
            word::WordPiece::ParameterExpansion(expression) => {
                parts.push(parameter_part(expression, raw_text(source, piece)));
                Ok(())
            }
            word::WordPiece::CommandSubstitution(_)
            | word::WordPiece::BackquotedCommandSubstitution(_) => {
                let substitution = self.substitution_part(&piece.piece, depth);
                parts.push(Part::Substitution(substitution));
                Ok(())
            }
            word::WordPiece::EscapeSequence(_) => {
                parts.push(Part::Quoted(decode_escape(raw_text(source, piece))));
                Ok(())
            }
            word::WordPiece::ArithmeticExpression(expression) => {
                // 算術はコマンドとして読まない展開（原文の綴りを保つ）。
                parts.push(Part::Opaque(format!("$(({}))", expression.value)));
                Ok(())
            }
        }
    }

    fn double_quoted_piece(
        &mut self,
        piece: &word::WordPieceWithSource,
        source: &str,
        depth: usize,
        parts: &mut Vec<Part>,
    ) -> Result<(), Failure> {
        match &piece.piece {
            word::WordPiece::Text(text) => {
                push_text(parts, text, true);
                Ok(())
            }
            word::WordPiece::EscapeSequence(_) => {
                parts.push(Part::Quoted(decode_escape_in_double(raw_text(
                    source, piece,
                ))));
                Ok(())
            }
            word::WordPiece::TildeExpansion(tilde) => {
                parts.push(Part::Quoted(tilde_spelling(tilde)));
                Ok(())
            }
            word::WordPiece::ParameterExpansion(expression) => {
                parts.push(parameter_part(expression, raw_text(source, piece)));
                Ok(())
            }
            word::WordPiece::CommandSubstitution(_)
            | word::WordPiece::BackquotedCommandSubstitution(_) => {
                let substitution = self.substitution_part(&piece.piece, depth);
                parts.push(Part::Substitution(substitution));
                Ok(())
            }
            word::WordPiece::ArithmeticExpression(expression) => {
                parts.push(Part::Opaque(format!("$(({}))", expression.value)));
                Ok(())
            }
            // 二重引用の内側に入れ子の引用は現れない。
            word::WordPiece::SingleQuotedText(text) | word::WordPiece::AnsiCQuotedText(text) => {
                parts.push(Part::Quoted(text.clone()));
                Ok(())
            }
            word::WordPiece::DoubleQuotedSequence(pieces)
            | word::WordPiece::GettextDoubleQuotedSequence(pieces) => {
                for inner in pieces {
                    self.double_quoted_piece(inner, source, depth, parts)?;
                }
                Ok(())
            }
        }
    }

    /// 置換の本体を再帰的に読み直す（REQ-037・REQ-039）。
    fn substitution(
        &mut self,
        piece: &word::WordPiece,
        depth: usize,
    ) -> Result<Substitution, Failure> {
        if depth + 1 > LIMIT_DEPTH {
            return Err(Failure::TooDeep);
        }
        let body_text = substitution_source(piece);
        if self.budget < body_text.len() {
            return Err(Failure::TooLarge);
        }
        self.budget -= body_text.len();
        let program = parse_program(&body_text, &self.options)?;
        let script = self.program(&program, depth + 1);
        Ok(Substitution {
            body_text,
            body: Some(Box::new(script)),
        })
    }

    /// 置換の断片。読めなかった本体は失敗に記録し、断片としては本文だけ残す。
    fn substitution_part(&mut self, piece: &word::WordPiece, depth: usize) -> Substitution {
        match self.substitution(piece, depth) {
            Ok(substitution) => substitution,
            Err(failure) => {
                self.failures.push(failure);
                Substitution {
                    body_text: substitution_source(piece),
                    body: None,
                }
            }
        }
    }
}

/// 引用の外の文字列を、リテラルと glob に分けて足す。
fn push_text(parts: &mut Vec<Part>, text: &str, quoted: bool) {
    if quoted {
        if !text.is_empty() {
            parts.push(Part::Quoted(text.to_string()));
        }
        return;
    }
    let mut literal = String::new();
    for c in text.chars() {
        if is_glob_char(c) {
            if !literal.is_empty() {
                parts.push(Part::Literal(std::mem::take(&mut literal)));
            }
            parts.push(Part::Glob(c.to_string()));
        } else {
            literal.push(c);
        }
    }
    if !literal.is_empty() {
        parts.push(Part::Literal(literal));
    }
}

fn is_glob_char(c: char) -> bool {
    matches!(c, '*' | '?' | '[')
}

/// 語の中で部品が占める原文。
fn raw_text<'a>(source: &'a str, piece: &word::WordPieceWithSource) -> &'a str {
    source.get(piece.start_index..piece.end_index).unwrap_or("")
}

/// パラメータ展開を、単純な変数だけ Var にし、それ以外は原文の綴りにする。
fn parameter_part(expression: &word::ParameterExpr, raw: &str) -> Part {
    if let word::ParameterExpr::Parameter {
        parameter,
        indirect: false,
    } = expression
    {
        match parameter {
            word::Parameter::Named(name) => return Part::Var(name.clone()),
            word::Parameter::NamedWithIndex { name, index } => {
                return Part::Var(format!("{name}[{index}]"))
            }
            _ => {}
        }
    }
    Part::Opaque(raw.to_string())
}

fn substitution_source(piece: &word::WordPiece) -> String {
    match piece {
        word::WordPiece::CommandSubstitution(text)
        | word::WordPiece::BackquotedCommandSubstitution(text) => text.clone(),
        _ => String::new(),
    }
}

/// 語の引用を外した見かけの文字列。設定の照合で使う（REQ-036・A17）。
pub(crate) fn unquoted_word(raw: &str, options: &ParserOptions) -> String {
    match catch(|| word::parse(raw, options)) {
        Ok(Ok(pieces)) => {
            let mut out = String::new();
            for piece in &pieces {
                render_plain(piece, raw, false, &mut out);
            }
            out
        }
        _ => raw.to_string(),
    }
}

fn render_plain(
    piece: &word::WordPieceWithSource,
    source: &str,
    in_double: bool,
    out: &mut String,
) {
    match &piece.piece {
        word::WordPiece::Text(text)
        | word::WordPiece::SingleQuotedText(text)
        | word::WordPiece::AnsiCQuotedText(text) => out.push_str(text),
        word::WordPiece::DoubleQuotedSequence(pieces)
        | word::WordPiece::GettextDoubleQuotedSequence(pieces) => {
            for piece in pieces {
                render_plain(piece, source, true, out);
            }
        }
        word::WordPiece::TildeExpansion(tilde) => out.push_str(&tilde_spelling(tilde)),
        word::WordPiece::ParameterExpansion(_) => out.push_str(raw_text(source, piece)),
        word::WordPiece::CommandSubstitution(text)
        | word::WordPiece::BackquotedCommandSubstitution(text) => {
            out.push_str("$(");
            out.push_str(text);
            out.push(')');
        }
        word::WordPiece::EscapeSequence(_) => {
            let raw = raw_text(source, piece);
            if in_double {
                out.push_str(&decode_escape_in_double(raw));
            } else {
                out.push_str(&decode_escape(raw));
            }
        }
        word::WordPiece::ArithmeticExpression(expression) => {
            out.push_str("$((");
            out.push_str(&expression.value);
            out.push_str("))");
        }
    }
}

fn tilde_spelling(tilde: &word::TildeExpr) -> String {
    match tilde {
        word::TildeExpr::Home => "~".to_string(),
        word::TildeExpr::UserHome(user) => format!("~{user}"),
        word::TildeExpr::WorkingDir => "~+".to_string(),
        word::TildeExpr::OldWorkingDir => "~-".to_string(),
        word::TildeExpr::NthDirFromTopOfDirStack { n, plus_used } => {
            if *plus_used {
                format!("~+{n}")
            } else {
                format!("~{n}")
            }
        }
        word::TildeExpr::NthDirFromBottomOfDirStack { n } => format!("~-{n}"),
    }
}

fn decode_escape(raw: &str) -> String {
    let mut chars = raw.chars();
    if chars.next() != Some('\\') {
        return raw.to_string();
    }
    match chars.next() {
        None => "\\".to_string(),
        Some('\n') => String::new(),
        Some(c) => c.to_string(),
    }
}

fn decode_escape_in_double(raw: &str) -> String {
    let mut chars = raw.chars();
    if chars.next() != Some('\\') {
        return raw.to_string();
    }
    match chars.next() {
        None => "\\".to_string(),
        Some('\n') => String::new(),
        Some(c @ ('$' | '`' | '"' | '\\')) => c.to_string(),
        Some(c) => format!("\\{c}"),
    }
}
