//! S2: 総走査の性質テスト（REQ-037）。
//!
//! 文法要素のコーパスに対して、OSS パーサの構文木に現れる語・リダイレクト・
//! 置換・複合構文を数え、正規化した構文木の対応する数を数えて比べる。

use brush_parser::{ast as raw, parse_tokens, uncached_tokenize_str, word, ParserOptions};
use guardian_parser::{parse, Command, Compound, Part, RedirectTarget, Script};

/// 文法要素のコーパス（REQ-037・A9）。
const CORPUS: &[&str] = &[
    // コマンド列・パイプ・and/or
    "echo a b c; pwd && ls || true; false & echo done",
    "find . -name '*.rs' -delete | xargs rm -f",
    "echo one\necho two\n",
    // 単純コマンド・代入・引用
    "a=1 b=2 cmd arg1 arg2",
    "IFS=, read a b <<< \"x\"",
    "echo \"quoted 'text'\" 'single \"text\"'",
    "echo foo\\ bar a\\*b",
    "echo ~; echo ~root; echo ~+/x",
    "echo {a,b,c}",
    // 複合構文
    "if true; then rm -rf /etc/x; elif false; then echo y; else echo z; fi",
    "while read -r line; do echo \"$line\"; done < input.txt",
    "until false; do echo x; done",
    "for i in a b c; do echo \"$i\"; done",
    "for ((i=0; i<3; i++)); do echo \"$i\"; done",
    "case \"$x\" in a|b) echo ab;; c) echo c;;& esac",
    "{ echo a; echo b; }",
    "( cd /tmp && rm -rf . )",
    "f() { echo \"$1\"; }; f x",
    "coproc NAME { echo hi; }",
    "time rm -rf /etc/x",
    "! false",
    // リダイレクト
    "cat < in.txt > out.txt >> log.txt 2>&1 <> rw.txt >| clobber.txt",
    "cat &> both.txt && cat &>> append.txt",
    "cat <<EOF\n$(rm -rf /etc/x)\nEOF",
    "cat <<-'EOF'\n\t$HOME\n\tEOF",
    "cat <<'EOF'\n$(rm -rf /etc/x)\nEOF",
    "cat <<< \"here $(echo x)\"",
    // 置換・算術・展開
    "echo $(date) `whoami` $((1+2)) ${HOME} $x",
    "echo \"pre $(rm -rf /etc/x) post\"",
    "echo <(ls) >(cat)",
    "[[ -n \"$(echo x)\" && -f /tmp/x ]]",
    "((1+2))",
    // 配列
    "a=(x y z); echo \"${a[0]}\"",
    // 走査漏れの事例として知られている位置
    "bash -c 'rm -rf /etc/x' extra args",
    "sudo -u \"$(rm -rf /etc/x)\" true",
    "sudo -u root bash -lc 'rm -rf /etc/x'",
    // 入れ子の置換
    "echo $(echo $(echo $(pwd)))",
    // 展開のオペランド・算術・配列の中の置換（走査漏れの位置）
    "echo ${X:-$(rm -rf /etc/x)}",
    "echo ${X:-${Y:-$(rm -rf /etc/x)}}",
    "echo $(( $(rm -rf /etc/x) + 1 ))",
    "(( $(rm -rf /etc/x) ))",
    "for ((i=0; i<$(rm -rf /etc/x); i++)); do :; done",
    "echo ${a[$(rm -rf /etc/x)]}",
    "a=($(rm -rf /etc/x))",
    // 二重引用の中の置換（同じ位置の別の綴り）
    "echo ${X:-\"$(rm -rf /etc/x)\"}",
    "echo ${X:=\"$(rm -rf /etc/x)\"}",
    "echo $(( \"$(rm -rf /etc/x)\" + 1 ))",
    "a=(\"$(rm -rf /etc/x)\")",
];

#[derive(Debug, Default, PartialEq, Eq)]
struct Shape {
    words: usize,
    redirects: usize,
    compounds: usize,
    substitutions: Vec<String>,
}

fn options() -> ParserOptions {
    ParserOptions::default()
}

fn brush_shape(input: &str) -> Shape {
    let options = options();
    let tokens = uncached_tokenize_str(input, &options.tokenizer_options()).expect("字句解析");
    let program = parse_tokens(&tokens, &options).expect("構文解析");
    let mut shape = Shape::default();
    for complete in &program.complete_commands {
        brush_list(complete, &mut shape);
    }
    shape
}

fn brush_list(list: &raw::CompoundList, shape: &mut Shape) {
    for item in &list.0 {
        brush_and_or(&item.0, shape);
    }
}

fn brush_and_or(list: &raw::AndOrList, shape: &mut Shape) {
    brush_pipeline(&list.first, shape);
    for additional in &list.additional {
        match additional {
            raw::AndOr::And(pipeline) | raw::AndOr::Or(pipeline) => brush_pipeline(pipeline, shape),
        }
    }
}

fn brush_pipeline(pipeline: &raw::Pipeline, shape: &mut Shape) {
    for command in &pipeline.seq {
        brush_command(command, shape);
    }
}

fn brush_command(command: &raw::Command, shape: &mut Shape) {
    match command {
        raw::Command::Simple(simple) => brush_simple(simple, shape),
        raw::Command::Compound(compound, redirects) => {
            brush_compound(compound, shape);
            brush_redirects(redirects.as_ref(), shape);
        }
        raw::Command::Function(function) => {
            shape.compounds += 1;
            brush_word(&function.fname, shape);
            brush_compound(&function.body.0, shape);
            brush_redirects(function.body.1.as_ref(), shape);
        }
        raw::Command::ExtendedTest(test, redirects) => {
            shape.compounds += 1;
            brush_test_expr(&test.expr, shape);
            brush_redirects(redirects.as_ref(), shape);
        }
    }
}

fn brush_simple(simple: &raw::SimpleCommand, shape: &mut Shape) {
    if let Some(prefix) = &simple.prefix {
        for item in &prefix.0 {
            brush_item(item, shape);
        }
    }
    if let Some(name) = &simple.word_or_name {
        brush_word(name, shape);
    }
    if let Some(suffix) = &simple.suffix {
        for item in &suffix.0 {
            brush_item(item, shape);
        }
    }
}

fn brush_item(item: &raw::CommandPrefixOrSuffixItem, shape: &mut Shape) {
    match item {
        raw::CommandPrefixOrSuffixItem::Word(word) => brush_word(word, shape),
        raw::CommandPrefixOrSuffixItem::AssignmentWord(_, word) => brush_word(word, shape),
        raw::CommandPrefixOrSuffixItem::IoRedirect(redirect) => brush_redirect(redirect, shape),
        raw::CommandPrefixOrSuffixItem::ProcessSubstitution(_, subshell) => {
            brush_list(&subshell.list, shape)
        }
    }
}

fn brush_redirects(redirects: Option<&raw::RedirectList>, shape: &mut Shape) {
    if let Some(list) = redirects {
        for redirect in &list.0 {
            brush_redirect(redirect, shape);
        }
    }
}

fn brush_redirect(redirect: &raw::IoRedirect, shape: &mut Shape) {
    shape.redirects += 1;
    match redirect {
        raw::IoRedirect::File(_, _, target) => match target {
            raw::IoFileRedirectTarget::Filename(word) => brush_word(word, shape),
            raw::IoFileRedirectTarget::Duplicate(word) => brush_word(word, shape),
            raw::IoFileRedirectTarget::Fd(_) => {}
            raw::IoFileRedirectTarget::ProcessSubstitution(_, subshell) => {
                brush_list(&subshell.list, shape)
            }
        },
        raw::IoRedirect::HereDocument(_, here) => {
            // 区切りの語と本文の語。本文は展開するときだけ置換を数える。
            shape.words += 2;
            if here.requires_expansion {
                brush_word_pieces(&here.doc.value, true, shape);
            }
        }
        raw::IoRedirect::HereString(_, word) => brush_word(word, shape),
        raw::IoRedirect::OutputAndError(word, _) => brush_word(word, shape),
    }
}

fn brush_compound(compound: &raw::CompoundCommand, shape: &mut Shape) {
    shape.compounds += 1;
    match compound {
        raw::CompoundCommand::Arithmetic(arithmetic) => {
            brush_arithmetic(&arithmetic.expr.value, shape)
        }
        raw::CompoundCommand::ArithmeticForClause(clause) => {
            for expr in [&clause.initializer, &clause.condition, &clause.updater]
                .into_iter()
                .flatten()
            {
                brush_arithmetic(&expr.value, shape);
            }
            brush_list(&clause.body.list, shape)
        }
        raw::CompoundCommand::BraceGroup(group) => brush_list(&group.list, shape),
        raw::CompoundCommand::Subshell(subshell) => brush_list(&subshell.list, shape),
        raw::CompoundCommand::ForClause(clause) => {
            if let Some(values) = &clause.values {
                for value in values {
                    brush_word(value, shape);
                }
            }
            brush_list(&clause.body.list, shape);
        }
        raw::CompoundCommand::CaseClause(clause) => {
            brush_word(&clause.value, shape);
            for item in &clause.cases {
                for pattern in &item.patterns {
                    brush_word(pattern, shape);
                }
                if let Some(list) = &item.cmd {
                    brush_list(list, shape);
                }
            }
        }
        raw::CompoundCommand::IfClause(clause) => {
            brush_list(&clause.condition, shape);
            brush_list(&clause.then, shape);
            if let Some(elses) = &clause.elses {
                for clause in elses {
                    if let Some(condition) = &clause.condition {
                        brush_list(condition, shape);
                    }
                    brush_list(&clause.body, shape);
                }
            }
        }
        raw::CompoundCommand::WhileClause(clause) | raw::CompoundCommand::UntilClause(clause) => {
            brush_list(&clause.0, shape);
            brush_list(&clause.1.list, shape);
        }
        raw::CompoundCommand::Coprocess(coprocess) => {
            if let Some(name) = &coprocess.name {
                brush_word(name, shape);
            }
            brush_command(&coprocess.body, shape);
        }
    }
}

fn brush_test_expr(expr: &raw::ExtendedTestExpr, shape: &mut Shape) {
    match expr {
        raw::ExtendedTestExpr::And(left, right) | raw::ExtendedTestExpr::Or(left, right) => {
            brush_test_expr(left, shape);
            brush_test_expr(right, shape);
        }
        raw::ExtendedTestExpr::Not(inner) | raw::ExtendedTestExpr::Parenthesized(inner) => {
            brush_test_expr(inner, shape)
        }
        raw::ExtendedTestExpr::UnaryTest(_, word) => brush_word(word, shape),
        raw::ExtendedTestExpr::BinaryTest(_, left, right) => {
            brush_word(left, shape);
            brush_word(right, shape);
        }
    }
}

fn brush_word(word: &raw::Word, shape: &mut Shape) {
    shape.words += 1;
    brush_word_pieces(&word.value, false, shape);
}

fn brush_word_pieces(text: &str, heredoc: bool, shape: &mut Shape) {
    let options = options();
    let pieces = if heredoc {
        word::parse_heredoc(text, &options)
    } else {
        word::parse(text, &options)
    };
    let Ok(pieces) = pieces else { return };
    brush_pieces(&pieces, shape);
}

fn brush_pieces(pieces: &[word::WordPieceWithSource], shape: &mut Shape) {
    for piece in pieces {
        match &piece.piece {
            word::WordPiece::DoubleQuotedSequence(inner)
            | word::WordPiece::GettextDoubleQuotedSequence(inner) => brush_pieces(inner, shape),
            word::WordPiece::CommandSubstitution(text)
            | word::WordPiece::BackquotedCommandSubstitution(text) => {
                shape.substitutions.push(text.clone());
                brush_body(text, shape);
            }
            word::WordPiece::ParameterExpansion(expression) => {
                for operand in brush_operands(expression) {
                    brush_word_pieces(&operand, false, shape);
                }
            }
            word::WordPiece::ArithmeticExpression(expression) => {
                brush_arithmetic(&expression.value, shape);
            }
            _ => {}
        }
    }
}

/// 算術式の中を語の断片として読み、中の置換を数える。
fn brush_arithmetic(value: &str, shape: &mut Shape) {
    brush_word_pieces(value, false, shape);
}

/// パラメータ展開が持つオペランドの原文（既定値、パターン、算術式、添字など）。
fn brush_operands(expression: &word::ParameterExpr) -> Vec<String> {
    use word::ParameterExpr as Expr;
    let mut operands = match expression {
        Expr::Parameter { .. } | Expr::ParameterLength { .. } | Expr::Transform { .. } => {
            Vec::new()
        }
        Expr::UseDefaultValues { default_value, .. }
        | Expr::AssignDefaultValues { default_value, .. } => {
            default_value.iter().cloned().collect()
        }
        Expr::IndicateErrorIfNullOrUnset { error_message, .. } => {
            error_message.iter().cloned().collect()
        }
        Expr::UseAlternativeValue {
            alternative_value, ..
        } => alternative_value.iter().cloned().collect(),
        Expr::RemoveSmallestSuffixPattern { pattern, .. }
        | Expr::RemoveLargestSuffixPattern { pattern, .. }
        | Expr::RemoveSmallestPrefixPattern { pattern, .. }
        | Expr::RemoveLargestPrefixPattern { pattern, .. }
        | Expr::UppercaseFirstChar { pattern, .. }
        | Expr::UppercasePattern { pattern, .. }
        | Expr::LowercaseFirstChar { pattern, .. }
        | Expr::LowercasePattern { pattern, .. } => pattern.iter().cloned().collect(),
        Expr::Substring { offset, length, .. } => {
            let mut operands = vec![offset.value.clone()];
            if let Some(length) = length {
                operands.push(length.value.clone());
            }
            operands
        }
        Expr::ReplaceSubstring {
            pattern,
            replacement,
            ..
        } => {
            let mut operands = vec![pattern.clone()];
            if let Some(replacement) = replacement {
                operands.push(replacement.clone());
            }
            operands
        }
        Expr::VariableNames { prefix, .. } => vec![prefix.clone()],
        Expr::MemberKeys { variable_name, .. } => vec![variable_name.clone()],
    };
    if let Some(word::Parameter::NamedWithIndex { index, .. }) = brush_parameter(expression) {
        operands.push(index.clone());
    }
    operands
}

/// 展開が使うパラメータ。
fn brush_parameter(expression: &word::ParameterExpr) -> Option<&word::Parameter> {
    use word::ParameterExpr as Expr;
    match expression {
        Expr::Parameter { parameter, .. }
        | Expr::UseDefaultValues { parameter, .. }
        | Expr::AssignDefaultValues { parameter, .. }
        | Expr::IndicateErrorIfNullOrUnset { parameter, .. }
        | Expr::UseAlternativeValue { parameter, .. }
        | Expr::ParameterLength { parameter, .. }
        | Expr::RemoveSmallestSuffixPattern { parameter, .. }
        | Expr::RemoveLargestSuffixPattern { parameter, .. }
        | Expr::RemoveSmallestPrefixPattern { parameter, .. }
        | Expr::RemoveLargestPrefixPattern { parameter, .. }
        | Expr::Substring { parameter, .. }
        | Expr::Transform { parameter, .. }
        | Expr::UppercaseFirstChar { parameter, .. }
        | Expr::UppercasePattern { parameter, .. }
        | Expr::LowercaseFirstChar { parameter, .. }
        | Expr::LowercasePattern { parameter, .. }
        | Expr::ReplaceSubstring { parameter, .. } => Some(parameter),
        Expr::VariableNames { .. } | Expr::MemberKeys { .. } => None,
    }
}

fn brush_body(text: &str, shape: &mut Shape) {
    let options = options();
    let Ok(tokens) = uncached_tokenize_str(text, &options.tokenizer_options()) else {
        return;
    };
    let Ok(program) = parse_tokens(&tokens, &options) else {
        return;
    };
    for complete in &program.complete_commands {
        brush_list(complete, shape);
    }
}

fn normalized_shape(script: &Script) -> Shape {
    let mut shape = Shape::default();
    normalized_script(script, &mut shape);
    shape
}

fn normalized_script(script: &Script, shape: &mut Shape) {
    for item in &script.items {
        normalized_pipeline(&item.first, shape);
        for (_, pipeline) in &item.rest {
            normalized_pipeline(pipeline, shape);
        }
    }
}

fn normalized_pipeline(pipeline: &guardian_parser::Pipeline, shape: &mut Shape) {
    for command in &pipeline.commands {
        normalized_command(command, shape);
    }
}

fn normalized_command(command: &Command, shape: &mut Shape) {
    match command {
        Command::Simple(simple) => {
            for word in &simple.words {
                normalized_word(word, shape);
            }
            for redirect in &simple.redirects {
                normalized_redirect(redirect, shape);
            }
            for substitution in &simple.process_substitutions {
                normalized_script(&substitution.body, shape);
            }
        }
        Command::Compound {
            compound,
            redirects,
        } => {
            normalized_compound(compound, shape);
            for redirect in redirects {
                normalized_redirect(redirect, shape);
            }
        }
        Command::Function(function) => {
            shape.compounds += 1;
            normalized_word(&function.name, shape);
            normalized_compound(&function.body, shape);
            for redirect in &function.redirects {
                normalized_redirect(redirect, shape);
            }
        }
        Command::Test(test) => {
            shape.compounds += 1;
            for word in &test.words {
                normalized_word(word, shape);
            }
            for redirect in &test.redirects {
                normalized_redirect(redirect, shape);
            }
        }
    }
}

fn normalized_redirect(redirect: &guardian_parser::Redirect, shape: &mut Shape) {
    shape.redirects += 1;
    match &redirect.target {
        RedirectTarget::Word(word) => normalized_word(word, shape),
        RedirectTarget::Fd(_) => {}
        RedirectTarget::ProcessSubstitution(substitution) => {
            normalized_script(&substitution.body, shape)
        }
        RedirectTarget::HereDocument { end, doc, .. } => {
            normalized_word(end, shape);
            normalized_word(doc, shape);
        }
    }
}

fn normalized_word(word: &guardian_parser::Word, shape: &mut Shape) {
    shape.words += 1;
    normalized_fragment(word, shape);
}

/// 語を数えずに、断片の中の置換だけを数える。
fn normalized_fragment(word: &guardian_parser::Word, shape: &mut Shape) {
    for part in &word.parts {
        if let Part::Substitution(substitution) = part {
            shape.substitutions.push(substitution.body_text.clone());
            if let Some(body) = &substitution.body {
                normalized_script(body, shape);
            }
        }
    }
}

fn normalized_compound(compound: &Compound, shape: &mut Shape) {
    shape.compounds += 1;
    match compound {
        Compound::If {
            condition,
            then,
            elses,
        } => {
            normalized_script(condition, shape);
            normalized_script(then, shape);
            for clause in elses {
                if let Some(condition) = &clause.condition {
                    normalized_script(condition, shape);
                }
                normalized_script(&clause.body, shape);
            }
        }
        Compound::While {
            condition, body, ..
        } => {
            normalized_script(condition, shape);
            normalized_script(body, shape);
        }
        Compound::For { values, body, .. } => {
            for value in values {
                normalized_word(value, shape);
            }
            normalized_script(body, shape);
        }
        Compound::ArithmeticFor {
            initializer,
            condition,
            updater,
            body,
        } => {
            for word in [initializer, condition, updater].into_iter().flatten() {
                normalized_fragment(word, shape);
            }
            normalized_script(body, shape);
        }
        Compound::Case { value, arms } => {
            normalized_word(value, shape);
            for arm in arms {
                for pattern in &arm.patterns {
                    normalized_word(pattern, shape);
                }
                if let Some(body) = &arm.body {
                    normalized_script(body, shape);
                }
            }
        }
        Compound::BraceGroup(script) | Compound::Subshell(script) => {
            normalized_script(script, shape)
        }
        Compound::Arithmetic(word) => normalized_fragment(word, shape),
        Compound::Coprocess { name, body } => {
            if let Some(name) = name {
                normalized_word(name, shape);
            }
            normalized_command(body, shape);
        }
    }
}

// @kotowari[REQ-037]
#[test]
fn req_037_every_position_is_visited() {
    for input in CORPUS {
        let expected = brush_shape(input);
        let outcome = parse(input);
        assert!(
            outcome.failures.is_empty(),
            "{input}: {:?}",
            outcome.failures
        );
        let actual = normalized_shape(&outcome.script);
        assert_eq!(expected, actual, "{input}");
    }
}
