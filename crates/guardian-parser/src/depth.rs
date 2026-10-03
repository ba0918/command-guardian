//! 正規化した構文木の深さの走査（REQ-039）。
//!
//! 構文の入れ子と置換の再帰の段数を数え、128 段を超えた時点で打ち切る。
//! 字句解析はしない。判定に使う構文木そのものが持つ入れ子だけを見る。

use crate::LIMIT_DEPTH;
use crate::ast::*;

/// 構文の入れ子と置換の再帰が上限を超えるか。超えた時点で打ち切る。
pub(crate) fn exceeds(script: &Script) -> bool {
    script_over(script, 0)
}

fn script_over(script: &Script, depth: usize) -> bool {
    script.items.iter().any(|item| item_over(item, depth))
}

fn item_over(item: &Item, depth: usize) -> bool {
    if depth > LIMIT_DEPTH {
        return true;
    }
    pipeline_over(&item.first, depth)
        || item
            .rest
            .iter()
            .any(|(_, pipeline)| pipeline_over(pipeline, depth))
}

fn pipeline_over(pipeline: &Pipeline, depth: usize) -> bool {
    if depth > LIMIT_DEPTH {
        return true;
    }
    pipeline
        .commands
        .iter()
        .any(|command| command_over(command, depth))
}

fn command_over(command: &Command, depth: usize) -> bool {
    if depth > LIMIT_DEPTH {
        return true;
    }
    match command {
        Command::Simple(simple) => {
            simple.words.iter().any(|word| word_over(word, depth))
                || simple.redirects.iter().any(|r| redirect_over(r, depth))
                || simple
                    .process_substitutions
                    .iter()
                    .any(|p| script_over(&p.body, depth + 1))
        }
        Command::Compound {
            compound,
            redirects,
        } => redirects.iter().any(|r| redirect_over(r, depth)) || compound_over(compound, depth),
        Command::Function(function) => {
            word_over(&function.name, depth)
                || compound_over(&function.body, depth)
                || function.redirects.iter().any(|r| redirect_over(r, depth))
        }
        Command::Test(test) => {
            test.words.iter().any(|word| word_over(word, depth))
                || test.redirects.iter().any(|r| redirect_over(r, depth))
        }
    }
}

fn compound_over(compound: &Compound, depth: usize) -> bool {
    if depth > LIMIT_DEPTH {
        return true;
    }
    let inner = depth + 1;
    match compound {
        Compound::If {
            condition,
            then,
            elses,
        } => {
            script_over(condition, inner)
                || script_over(then, inner)
                || elses.iter().any(|clause| {
                    clause
                        .condition
                        .as_ref()
                        .is_some_and(|condition| script_over(condition, inner))
                        || script_over(&clause.body, inner)
                })
        }
        Compound::While {
            condition, body, ..
        } => script_over(condition, inner) || script_over(body, inner),
        Compound::For { values, body, .. } => {
            values.iter().any(|word| word_over(word, inner)) || script_over(body, inner)
        }
        Compound::ArithmeticFor {
            initializer,
            condition,
            updater,
            body,
        } => {
            [initializer, condition, updater]
                .into_iter()
                .flatten()
                .any(|word| word_over(word, inner))
                || script_over(body, inner)
        }
        Compound::Case { value, arms } => {
            word_over(value, inner)
                || arms.iter().any(|arm| {
                    arm.patterns.iter().any(|word| word_over(word, inner))
                        || arm
                            .body
                            .as_ref()
                            .is_some_and(|body| script_over(body, inner))
                })
        }
        Compound::BraceGroup(script) | Compound::Subshell(script) => script_over(script, inner),
        Compound::Arithmetic(word) => word_over(word, inner),
        Compound::Coprocess { name, body } => {
            name.as_ref().is_some_and(|word| word_over(word, inner)) || command_over(body, inner)
        }
    }
}

fn word_over(word: &Word, depth: usize) -> bool {
    if depth > LIMIT_DEPTH {
        return true;
    }
    word.parts.iter().any(|part| match part {
        Part::Substitution(substitution) => {
            depth + 1 > LIMIT_DEPTH
                || substitution
                    .body
                    .as_ref()
                    .is_some_and(|body| script_over(body, depth + 1))
        }
        _ => false,
    })
}

fn redirect_over(redirect: &Redirect, depth: usize) -> bool {
    match &redirect.target {
        RedirectTarget::Word(word) => word_over(word, depth),
        RedirectTarget::Fd(_) => false,
        RedirectTarget::ProcessSubstitution(substitution) => {
            script_over(&substitution.body, depth + 1)
        }
        RedirectTarget::HereDocument { end, doc, .. } => {
            word_over(end, depth) || word_over(doc, depth)
        }
    }
}
