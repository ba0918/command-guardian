//! シェル構文の最小の解析。コマンド列、パイプ、引用、ヒアドキュメント、
//! コマンド置換、ラッパー、`bash -c` と `eval` の内側を読むために必要な分だけ扱う。

/// 語を構成する部分。引用の内側か外側かで展開の意味が変わる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// 引用を外したリテラル。
    Literal(String),
    /// `$NAME` または `${NAME}`。解決は照合時に行う。
    Var(String),
    /// `$(...)` またはバッククォート。内側のスクリプト本文。
    Subst(String),
    /// `$((...))` のような、コマンドとして読まない展開。
    Opaque(String),
    /// 引用されていない glob 文字。
    Glob(String),
}

/// 1 つの語。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub parts: Vec<Part>,
    /// 引用を外した見かけの文字列。展開は解かない。
    pub text: String,
    /// 引用されていない glob 文字を含むか。
    pub has_glob: bool,
}

impl Word {
    pub fn literal(&self) -> bool {
        !self.has_glob && self.parts.iter().all(|p| matches!(p, Part::Literal(_)))
    }

    /// リテラルならその値。
    pub fn literal_value(&self) -> Option<String> {
        if self.literal() {
            Some(self.text.clone())
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpTok {
    Semi,
    And,
    Or,
    Pipe,
    Amp,
    Newline,
    Out,
    Append,
    Clobber,
    In,
    Heredoc,
    HeredocDash,
    LParen,
    RParen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokKind {
    Word(Word),
    Op(OpTok),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tok {
    pub kind: TokKind,
    /// 直前の語と空白を挟まずに続いているか（fd 番号や `2>&1` の判定に使う）。
    pub glued_left: bool,
}

/// リダイレクト。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Redirect {
    Out(Word),
    Append(Word),
    Clobber(Word),
    In(Word),
    Heredoc(Word),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SimpleCommand {
    pub words: Vec<Word>,
    pub redirects: Vec<Redirect>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pipeline {
    pub commands: Vec<SimpleCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Simple(Pipeline),
    For {
        var: String,
        words: Vec<Word>,
        body: Vec<Item>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedScript {
    pub items: Vec<Item>,
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_glob_char(c: char) -> bool {
    matches!(c, '*' | '?' | '[')
}

/// 隣り合うリテラルを 1 つにまとめる。
fn push_literal(parts: &mut Vec<Part>, s: &str) {
    if s.is_empty() {
        return;
    }
    if let Some(Part::Literal(last)) = parts.last_mut() {
        last.push_str(s);
        return;
    }
    parts.push(Part::Literal(s.to_string()));
}

struct Lexer {
    chars: Vec<char>,
    i: usize,
    toks: Vec<Tok>,
    /// 解析の失敗（閉じない引用など）。
    errors: Vec<String>,
    /// ヒアドキュメントの区切り待ち（本文スキップ用）。値は (区切り語, タブ除去)。
    pending_heredocs: Vec<(String, bool)>,
    /// 直前に語を出し終えた位置。`glued_left` の判定に使う。
    prev_word_end: Option<usize>,
    /// 語の先頭か（`#` のコメント判定に使う）。
    at_word_start: bool,
    /// 次の語をヒアドキュメントの区切りとして記録するか (タブ除去)。
    expect_heredoc: Option<bool>,
}

impl Lexer {
    fn new(input: &str) -> Self {
        Lexer {
            chars: input.chars().collect(),
            i: 0,
            toks: Vec::new(),
            errors: Vec::new(),
            pending_heredocs: Vec::new(),
            prev_word_end: None,
            at_word_start: true,
            expect_heredoc: None,
        }
    }

    fn peek(&self, off: usize) -> Option<char> {
        self.chars.get(self.i + off).copied()
    }

    fn push_op(&mut self, op: OpTok, len: usize) {
        let glued = self.prev_word_end == Some(self.i);
        self.toks.push(Tok {
            kind: TokKind::Op(op),
            glued_left: glued,
        });
        self.i += len;
        self.prev_word_end = None;
        self.at_word_start = true;
    }

    fn push_word(&mut self, word: Word) {
        if let Some(dash) = self.expect_heredoc.take() {
            let delim = word.text.trim_matches(|c: char| c == '"' || c == '\'');
            if !delim.is_empty() {
                self.pending_heredocs.push((delim.to_string(), dash));
            }
        }
        self.toks.push(Tok {
            kind: TokKind::Word(word),
            glued_left: false,
        });
        self.prev_word_end = Some(self.i);
        self.at_word_start = false;
    }

    /// 語を 1 つ読む。`self.i` は語の先頭にあること。
    fn read_word(&mut self) -> Word {
        let mut parts: Vec<Part> = Vec::new();
        let mut text = String::new();
        let mut has_glob = false;

        while let Some(c) = self.peek(0) {
            match c {
                ' ' | '\t' | '\n' => break,
                ';' | '&' | '|' | '>' | '<' | '(' | ')' => break,
                '\\' => {
                    self.i += 1;
                    match self.peek(0) {
                        Some('\n') => {
                            self.i += 1;
                        }
                        Some(next) => {
                            self.i += 1;
                            text.push(next);
                            push_literal(&mut parts, &next.to_string());
                        }
                        None => {}
                    }
                }
                '\'' => {
                    self.i += 1;
                    let mut s = String::new();
                    let mut closed = false;
                    while let Some(ch) = self.peek(0) {
                        self.i += 1;
                        if ch == '\'' {
                            closed = true;
                            break;
                        }
                        s.push(ch);
                    }
                    if !closed {
                        self.errors.push("閉じない単引用符".to_string());
                    }
                    text.push_str(&s);
                    push_literal(&mut parts, &s);
                }
                '"' => {
                    self.i += 1;
                    self.read_double_quoted(&mut parts, &mut text);
                }
                '`' => {
                    self.i += 1;
                    let s = self.read_backtick();
                    text.push_str("$(");
                    text.push_str(&s);
                    text.push(')');
                    parts.push(Part::Subst(s));
                }
                '$' => {
                    self.read_dollar(&mut parts, &mut text);
                }
                _ if is_glob_char(c) => {
                    self.i += 1;
                    has_glob = true;
                    text.push(c);
                    parts.push(Part::Glob(c.to_string()));
                }
                _ => {
                    self.i += 1;
                    text.push(c);
                    push_literal(&mut parts, &c.to_string());
                }
            }
        }

        Word {
            parts,
            text,
            has_glob,
        }
    }

    fn read_double_quoted(&mut self, parts: &mut Vec<Part>, text: &mut String) {
        let mut closed = false;
        while let Some(c) = self.peek(0) {
            match c {
                '"' => {
                    self.i += 1;
                    closed = true;
                    break;
                }
                '\\' => {
                    self.i += 1;
                    match self.peek(0) {
                        Some('\n') => {
                            self.i += 1;
                        }
                        Some(next @ ('$' | '`' | '"' | '\\')) => {
                            self.i += 1;
                            text.push(next);
                            push_literal(parts, &next.to_string());
                        }
                        Some(next) => {
                            self.i += 1;
                            text.push('\\');
                            text.push(next);
                            push_literal(parts, &format!("\\{next}"));
                        }
                        None => {}
                    }
                }
                '$' => self.read_dollar(parts, text),
                '`' => {
                    self.i += 1;
                    let s = self.read_backtick();
                    text.push_str("$(");
                    text.push_str(&s);
                    text.push(')');
                    parts.push(Part::Subst(s));
                }
                _ => {
                    self.i += 1;
                    text.push(c);
                    push_literal(parts, &c.to_string());
                }
            }
        }
        if !closed {
            self.errors.push("閉じない二重引用符".to_string());
        }
    }

    /// バッククォートの内側を読む。`self.i` は開きの直後。
    fn read_backtick(&mut self) -> String {
        let mut s = String::new();
        let mut closed = false;
        while let Some(ch) = self.peek(0) {
            if ch == '`' {
                self.i += 1;
                closed = true;
                break;
            }
            if ch == '\\' {
                self.i += 1;
                if let Some(next) = self.peek(0) {
                    self.i += 1;
                    s.push(next);
                }
                continue;
            }
            self.i += 1;
            s.push(ch);
        }
        if !closed {
            self.errors.push("閉じないバッククォート".to_string());
        }
        s
    }

    fn read_dollar(&mut self, parts: &mut Vec<Part>, text: &mut String) {
        // `$` の位置。
        self.i += 1;
        match self.peek(0) {
            Some('(') => {
                if self.peek(1) == Some('(') {
                    // 算術展開。コマンドとしては読まない。
                    self.i += 2;
                    let mut depth = 1usize;
                    let mut s = String::new();
                    while let Some(ch) = self.peek(0) {
                        if ch == '(' {
                            depth += 1;
                        } else if ch == ')' {
                            depth -= 1;
                            if depth == 0 {
                                self.i += 1;
                                break;
                            }
                        }
                        self.i += 1;
                        s.push(ch);
                    }
                    if depth != 0 {
                        self.errors.push("閉じない算術展開".to_string());
                    }
                    text.push_str("$((");
                    text.push_str(&s);
                    text.push_str("))");
                    parts.push(Part::Opaque(s));
                } else {
                    self.i += 1;
                    let mut depth = 1usize;
                    let mut s = String::new();
                    while let Some(ch) = self.peek(0) {
                        match ch {
                            '(' => depth += 1,
                            ')' => {
                                depth -= 1;
                                if depth == 0 {
                                    self.i += 1;
                                    break;
                                }
                            }
                            _ => {}
                        }
                        self.i += 1;
                        s.push(ch);
                    }
                    if depth != 0 {
                        self.errors.push("閉じないコマンド置換".to_string());
                    }
                    text.push_str("$(");
                    text.push_str(&s);
                    text.push(')');
                    parts.push(Part::Subst(s));
                }
            }
            Some('{') => {
                self.i += 1;
                let mut s = String::new();
                let mut closed = false;
                while let Some(ch) = self.peek(0) {
                    self.i += 1;
                    if ch == '}' {
                        closed = true;
                        break;
                    }
                    s.push(ch);
                }
                if !closed {
                    self.errors.push("閉じない変数展開".to_string());
                }
                text.push_str("${");
                text.push_str(&s);
                text.push('}');
                parts.push(Part::Var(s));
            }
            Some(c) if is_name_start(c) => {
                let mut s = String::new();
                while let Some(ch) = self.peek(0) {
                    if is_name_char(ch) {
                        self.i += 1;
                        s.push(ch);
                    } else {
                        break;
                    }
                }
                text.push('$');
                text.push_str(&s);
                parts.push(Part::Var(s));
            }
            Some(c) => {
                // 特殊パラメータは解決しない。
                self.i += 1;
                text.push('$');
                text.push(c);
                parts.push(Part::Opaque(c.to_string()));
            }
            None => {
                text.push('$');
                push_literal(parts, "$");
            }
        }
    }

    /// ヒアドキュメントの本文を、区切りの行まで読み飛ばす。
    fn skip_heredoc_body(&mut self, delim: &str, strip_tabs: bool) {
        loop {
            // 1 行読む。
            let start = self.i;
            while let Some(c) = self.peek(0) {
                if c == '\n' {
                    break;
                }
                self.i += 1;
            }
            let line: String = self.chars[start..self.i].iter().collect();
            let line = if strip_tabs {
                line.trim_start_matches('\t').to_string()
            } else {
                line
            };
            if self.peek(0) == Some('\n') {
                self.i += 1;
            }
            if line == delim {
                break;
            }
            if self.i >= self.chars.len() {
                break;
            }
        }
    }

    fn skip_to_line_end(&mut self) {
        while let Some(c) = self.peek(0) {
            if c == '\n' {
                break;
            }
            self.i += 1;
        }
    }

    fn run(mut self) -> (Vec<Tok>, Vec<String>) {
        while let Some(c) = self.peek(0) {
            match c {
                ' ' | '\t' => {
                    self.i += 1;
                    self.prev_word_end = None;
                    self.at_word_start = true;
                }
                '\n' => {
                    self.push_op(OpTok::Newline, 1);
                    let pend = std::mem::take(&mut self.pending_heredocs);
                    for (delim, dash) in pend {
                        self.skip_heredoc_body(&delim, dash);
                    }
                }
                '#' if self.at_word_start => {
                    self.skip_to_line_end();
                }
                '\\' if self.peek(1) == Some('\n') => {
                    self.i += 2;
                }
                ';' => self.push_op(OpTok::Semi, 1),
                '&' => {
                    if self.peek(1) == Some('&') {
                        self.push_op(OpTok::And, 2);
                    } else {
                        self.push_op(OpTok::Amp, 1);
                    }
                }
                '|' => {
                    if self.peek(1) == Some('|') {
                        self.push_op(OpTok::Or, 2);
                    } else {
                        self.push_op(OpTok::Pipe, 1);
                    }
                }
                '>' => {
                    if self.peek(1) == Some('>') {
                        self.push_op(OpTok::Append, 2);
                    } else if self.peek(1) == Some('|') {
                        self.push_op(OpTok::Clobber, 2);
                    } else {
                        self.push_op(OpTok::Out, 1);
                    }
                }
                '<' => {
                    if self.peek(1) == Some('<') {
                        if self.peek(2) == Some('-') {
                            self.push_op(OpTok::HeredocDash, 3);
                            self.expect_heredoc = Some(true);
                        } else {
                            self.push_op(OpTok::Heredoc, 2);
                            self.expect_heredoc = Some(false);
                        }
                    } else {
                        self.push_op(OpTok::In, 1);
                    }
                }
                '(' => self.push_op(OpTok::LParen, 1),
                ')' => self.push_op(OpTok::RParen, 1),
                _ => {
                    let word = self.read_word();
                    self.push_word(word);
                }
            }
        }
        if !self.pending_heredocs.is_empty() {
            self.errors.push("閉じないヒアドキュメント".to_string());
        }
        (self.toks, self.errors)
    }
}

pub fn tokenize(input: &str) -> Vec<Tok> {
    tokenize_with_errors(input).0
}

/// 語と、解析の失敗を返す。
pub fn tokenize_with_errors(input: &str) -> (Vec<Tok>, Vec<String>) {
    Lexer::new(input).run()
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn word_text(&self) -> Option<&str> {
        match self.peek().map(|t| &t.kind) {
            Some(TokKind::Word(w)) => Some(&w.text),
            _ => None,
        }
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn skip_separators(&mut self) {
        while let Some(Tok {
            kind: TokKind::Op(op),
            ..
        }) = self.peek()
        {
            if matches!(
                op,
                OpTok::Semi | OpTok::Amp | OpTok::Newline | OpTok::And | OpTok::Or
            ) {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn at_stop(&self, stop: Option<&str>) -> bool {
        match (stop, self.word_text()) {
            (Some(s), Some(w)) => s == w,
            _ => false,
        }
    }

    fn parse_items(&mut self, stop: Option<&str>) -> Vec<Item> {
        let mut items = Vec::new();
        if self.depth > 32 {
            return items;
        }
        loop {
            self.skip_separators();
            if self.pos >= self.toks.len() {
                break;
            }
            if self.at_stop(stop) {
                break;
            }
            if self.word_text() == Some("for") {
                self.depth += 1;
                let item = self.parse_for();
                self.depth -= 1;
                items.push(item);
                continue;
            }
            if matches!(
                self.peek().map(|t| &t.kind),
                Some(TokKind::Op(OpTok::LParen))
            ) || matches!(
                self.peek().map(|t| &t.kind),
                Some(TokKind::Op(OpTok::RParen))
            ) {
                // グループ化は M1 では読まない。括弧は読み飛ばす。
                self.advance();
                continue;
            }
            if let Some(item) = self.parse_simple(stop) {
                items.push(item);
            }
        }
        items
    }

    fn parse_for(&mut self) -> Item {
        self.advance(); // for
        let var = self.word_text().unwrap_or("").to_string();
        if self.word_text().is_some() {
            self.advance();
        }
        let mut words = Vec::new();
        if self.word_text() == Some("in") {
            self.advance();
            loop {
                match self.peek().map(|t| &t.kind) {
                    Some(TokKind::Word(w))
                        if w.text != "do" && w.text != "done" && w.text != "for" =>
                    {
                        words.push(w.clone());
                        self.advance();
                    }
                    _ => break,
                }
            }
        }
        // `;` や改行を挟んで `do` を探す。
        while let Some(Tok {
            kind: TokKind::Op(op),
            ..
        }) = self.peek()
        {
            if matches!(op, OpTok::Semi | OpTok::Newline) {
                self.advance();
            } else {
                break;
            }
        }
        if self.word_text() == Some("do") {
            self.advance();
        }
        let body = self.parse_items(Some("done"));
        if self.word_text() == Some("done") {
            self.advance();
        }
        Item::For { var, words, body }
    }

    /// 1 つの単純コマンドと、それに続くパイプを読む。
    fn parse_simple(&mut self, stop: Option<&str>) -> Option<Item> {
        let mut pipeline = Pipeline::default();
        loop {
            let cmd = self.parse_command(stop);
            pipeline.commands.push(cmd);
            match self.peek().map(|t| &t.kind) {
                Some(TokKind::Op(OpTok::Pipe)) => {
                    self.advance();
                }
                _ => break,
            }
        }
        Some(Item::Simple(pipeline))
    }

    fn parse_command(&mut self, stop: Option<&str>) -> SimpleCommand {
        let mut cmd = SimpleCommand::default();
        loop {
            if self.pos >= self.toks.len() {
                break;
            }
            if self.at_stop(stop) {
                break;
            }
            match self.peek().map(|t| t.kind.clone()) {
                Some(TokKind::Word(w)) => {
                    // fd 番号 `2>` の判定。
                    let all_digits =
                        !w.text.is_empty() && w.text.chars().all(|c| c.is_ascii_digit());
                    let next_is_redirect = matches!(
                        self.toks.get(self.pos + 1).map(|t| (&t.kind, t.glued_left)),
                        Some((
                            TokKind::Op(OpTok::Out | OpTok::Append | OpTok::Clobber | OpTok::In),
                            true
                        ))
                    );
                    if all_digits && next_is_redirect {
                        self.advance();
                        continue;
                    }
                    cmd.words.push(w);
                    self.advance();
                }
                Some(TokKind::Op(OpTok::Out | OpTok::Append | OpTok::Clobber)) => {
                    let op = match self.peek().map(|t| &t.kind) {
                        Some(TokKind::Op(o)) => *o,
                        _ => unreachable!(),
                    };
                    self.advance();
                    // `2>&1` のような複製。
                    if matches!(self.peek().map(|t| &t.kind), Some(TokKind::Op(OpTok::Amp))) {
                        self.advance();
                        if matches!(self.peek().map(|t| &t.kind), Some(TokKind::Word(_))) {
                            self.advance();
                        }
                        continue;
                    }
                    if let Some(TokKind::Word(w)) = self.peek().map(|t| t.kind.clone()) {
                        self.advance();
                        let r = match op {
                            OpTok::Out => Redirect::Out(w),
                            OpTok::Append => Redirect::Append(w),
                            _ => Redirect::Clobber(w),
                        };
                        cmd.redirects.push(r);
                    }
                }
                Some(TokKind::Op(OpTok::In | OpTok::Heredoc | OpTok::HeredocDash)) => {
                    let op = match self.peek().map(|t| &t.kind) {
                        Some(TokKind::Op(o)) => *o,
                        _ => unreachable!(),
                    };
                    self.advance();
                    if let Some(TokKind::Word(w)) = self.peek().map(|t| t.kind.clone()) {
                        self.advance();
                        cmd.redirects.push(match op {
                            OpTok::In => Redirect::In(w),
                            _ => Redirect::Heredoc(w),
                        });
                    }
                }
                Some(TokKind::Op(_)) => break,
                None => break,
            }
        }
        cmd
    }
}

/// スクリプトを解析する。
pub fn parse_script(input: &str) -> ParsedScript {
    parse_script_with_errors(input).0
}

/// スクリプトと、解析の失敗を返す。
pub fn parse_script_with_errors(input: &str) -> (ParsedScript, Vec<String>) {
    let (toks, errors) = tokenize_with_errors(input);
    let mut parser = Parser {
        toks,
        pos: 0,
        depth: 0,
    };
    (
        ParsedScript {
            items: parser.parse_items(None),
        },
        errors,
    )
}

/// 引用とヒアドキュメントを外したコマンド本文。カスタムルールの照合に使う。
pub fn strip_quotes_and_heredocs(input: &str) -> String {
    let toks = tokenize(input);
    let mut out = String::new();
    for tok in &toks {
        let s: String = match &tok.kind {
            TokKind::Word(w) => w.text.clone(),
            TokKind::Op(op) => match op {
                OpTok::Semi => ";".to_string(),
                OpTok::And => "&&".to_string(),
                OpTok::Or => "||".to_string(),
                OpTok::Pipe => "|".to_string(),
                OpTok::Amp => "&".to_string(),
                OpTok::Newline => "\n".to_string(),
                OpTok::Out => ">".to_string(),
                OpTok::Append => ">>".to_string(),
                OpTok::Clobber => ">|".to_string(),
                OpTok::In => "<".to_string(),
                OpTok::Heredoc => "<<".to_string(),
                OpTok::HeredocDash => "<<-".to_string(),
                OpTok::LParen => "(".to_string(),
                OpTok::RParen => ")".to_string(),
            },
        };
        if out.is_empty() {
            out.push_str(&s);
        } else {
            out.push(' ');
            out.push_str(&s);
        }
    }
    out
}
