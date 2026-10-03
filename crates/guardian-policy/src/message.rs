//! 非 allow の文面。「何を・なぜ・代替」を 2〜4 行で組み立てる。

use guardian_core::Failure;
use guardian_core::{Ask, Class, Op, ProtectedKind, Target, Why};

/// 対象を人が読める形にする。
pub fn display_target(target: &Target) -> String {
    match target {
        Target::Path { path, dereference } => {
            let mut s = path.display().to_string();
            if *dereference {
                s.push('/');
            }
            s
        }
        Target::GlobBase(base) => format!("{}/…", base.display()),
        Target::Children { base, .. } => format!("children of {}", base.display()),
        Target::Mktemp => "path created by mktemp".to_string(),
        Target::UnknownSource => "targets from an unknown source".to_string(),
        Target::Unresolved(text) => format!("{text} (unresolved path)"),
    }
}

/// Keep the original data separate from its reversible, single-line display.
pub fn display_inline(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

pub fn op_label(op: Op) -> &'static str {
    match op {
        Op::Delete => "Delete",
        Op::Truncate => "Truncate",
        Op::Format => "Format",
    }
}

fn protected_label(kind: &ProtectedKind) -> &'static str {
    match kind {
        ProtectedKind::Root => "root directory",
        ProtectedKind::SystemArea => "system area",
        ProtectedKind::EphemeralRoot => "temporary directory root",
        ProtectedKind::Home => "home directory",
        ProtectedKind::Cwd => "working directory",
        ProtectedKind::RepoRoot => "repository root",
        ProtectedKind::DotGit => ".git",
        ProtectedKind::OtherHome => "another user's home",
        ProtectedKind::ConfiguredRoot => "configured protected root",
    }
}

fn reason_phrase(class: Class, why: &Why) -> String {
    let detail = match why {
        Why::Ephemeral => String::new(),
        Why::Mktemp => String::new(),
        Why::Vcs => String::new(),
        Why::Untracked => " (untracked)".to_string(),
        Why::Uncommitted => " (uncommitted changes)".to_string(),
        Why::Unmanaged => " (not managed by git)".to_string(),
        Why::Unresolved(_) => " (unresolved path)".to_string(),
        Why::UnknownSource => " (unknown targets)".to_string(),
        Why::GitFailed => " (git check failed)".to_string(),
        Why::Protected(kind) => format!(" ({})", protected_label(kind)),
    };
    format!("{class}{detail}")
}

fn loss_phrase(why: &Why) -> &'static str {
    match why {
        Why::Untracked => "this has not been committed and cannot be recovered after deletion",
        Why::Uncommitted => "uncommitted changes will be lost",
        Why::Unmanaged => "this is not managed by git and cannot be recovered after deletion",
        Why::Unresolved(_) => "the target cannot be resolved, so what would be lost is unknown",
        Why::UnknownSource => "the scope of deletion cannot be determined",
        Why::GitFailed => "the git tracking state could not be checked",
        Why::Protected(_) => "deleted data cannot be recovered",
        Why::Ephemeral | Why::Mktemp | Why::Vcs => "deleted data cannot be recovered",
    }
}

fn alternative_phrase(why: &Why) -> &'static str {
    match why {
        Why::Untracked => {
            "Move it to a temporary or working area before deleting, or commit it first."
        }
        Why::Uncommitted => "Commit the changes first.",
        Why::Unmanaged => {
            "Move it to a temporary or working area before deleting, or add an allowed root."
        }
        Why::Unresolved(_) => "Specify a literal path instead.",
        Why::UnknownSource => "Specify literal targets instead.",
        Why::GitFailed => "Check the tracking state with git status before deleting.",
        Why::Protected(_) => "No applicable alternative.",
        Why::Ephemeral | Why::Mktemp | Why::Vcs => "No applicable alternative.",
    }
}

/// 非 allow の理由を 1 行にする。
pub fn reason_line(class: Class, why: &Why) -> String {
    format!("{}; {}", reason_phrase(class, why), loss_phrase(why))
}

/// 非 allow の文面。`block` はエージェント向け、`ask` は利用者向けに書く。
pub fn non_allow_message(op: Op, target: &Target, class: Class, why: &Why) -> String {
    let lines = [
        operation_line(op, target),
        format!("Reason: {}", reason_line(class, why)),
        format!("Alternative: {}", alternative_phrase(why)),
    ];
    lines.join("\n")
}

fn operation_line(op: Op, target: &Target) -> String {
    format!(
        "{}: {}",
        op_label(op),
        display_inline(&display_target(target))
    )
}

pub fn advisor_explanation(
    risk: guardian_advisor::Risk,
    verdict: guardian_core::Verdict,
    mechanical_reason: &str,
    effects: &[crate::EffectReport],
) -> (String, String) {
    use guardian_advisor::Risk;
    use guardian_core::Verdict;
    let (detail, alternative) = match (risk, verdict) {
        (Risk::HarmfulIrreversible, _) => (
            "independent harmful irreversible effects require blocking, even with matching instructions",
            "No applicable safe alternative for this command. Revise it to remove the harmful effects.",
        ),
        (Risk::MajorDestructive, Verdict::Ask) => (
            "major destruction matches confirmed instructions but still requires your confirmation of irreversible loss",
            "Before approving, verify the target, operation and all effects, and that the loss is intended. Otherwise cancel or narrow the operation.",
        ),
        (Risk::MajorDestructive, _) => (
            "major destruction lacks sufficiently confirmed matching instructions",
            "Obtain concrete user instructions covering the target, operation and all effects, or narrow the operation before checking again.",
        ),
        _ => ("destructive effects require intervention", "No applicable alternative."),
    };
    let intervention = format!("Advisor classification {}: {detail}", risk.as_str());
    let reason = if mechanical_reason.is_empty() {
        intervention.clone()
    } else {
        format!("{mechanical_reason}; {intervention}")
    };
    let heading = match crate::evaluation::worst_effect(effects).or_else(|| effects.first()) {
        Some(effect) => operation_line(effect.op, &effect.target),
        None => format!("Advisor intervention: {}", risk.as_str()),
    };
    let mut message = format!("{heading}\nReason: {intervention}\nAlternative: {alternative}");
    if !mechanical_reason.is_empty() {
        message.push_str(&format!(
            "\nMechanical reason: {}",
            display_inline(mechanical_reason)
        ));
    }
    (reason, message)
}

/// 操作とパスが無い ask の種類（REQ-011）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AskKind {
    /// 構文を読めない。
    Syntax,
    /// 判定の内部で失敗した（自分に帰せる死。panic、起動とプロトコルの失敗）。
    Internal,
    /// 入力が大きすぎる。
    TooLarge,
    /// 入れ子が深すぎる。
    TooDeep,
    /// 判定の上限（回数・時間）を超えた。
    Limit,
    /// 読めないシェル。
    Shell,
}

fn ask_kind(ask: &Ask) -> AskKind {
    match ask {
        Ask::Parse(Failure::TooLarge) => AskKind::TooLarge,
        Ask::Parse(Failure::TooDeep) => AskKind::TooDeep,
        Ask::Parse(Failure::Limit) => AskKind::Limit,
        Ask::Parse(Failure::Panic | Failure::Internal) => AskKind::Internal,
        Ask::Parse(_) => AskKind::Syntax,
        Ask::UnreadableProgram(_) | Ask::UnreadableShellBody(_) | Ask::UnreadableEval => {
            AskKind::Syntax
        }
        Ask::UnsupportedShell(_) => AskKind::Shell,
    }
}

/// 上限を超えた block の理由を 1 行にする（REQ-011）。
pub fn limit_reason(asks: &[Ask]) -> String {
    match asks.iter().find(|ask| ask.is_limit()) {
        Some(Ask::Parse(Failure::TooLarge)) => "Judgment stopped: input is too large".to_string(),
        Some(Ask::Parse(Failure::TooDeep)) => {
            "Judgment stopped: input is nested too deeply".to_string()
        }
        _ => "Judgment stopped: a judgment limit was exceeded".to_string(),
    }
}

/// 上限を超えた block の文面（REQ-011）。操作とパスに代えて理由を示す。
pub fn limit_message(asks: &[Ask]) -> String {
    let (title, reason) = match asks.iter().find(|ask| ask.is_limit()) {
        Some(Ask::Parse(Failure::TooLarge)) => (
            "Judgment stopped: input is too large",
            "Reason: Input exceeds the parsing limit of 1 MiB.",
        ),
        Some(Ask::Parse(Failure::TooDeep)) => (
            "Judgment stopped: input is nested too deeply",
            "Reason: Syntax nesting exceeds the limit of 128 levels.",
        ),
        _ => (
            "Judgment stopped: a judgment limit was exceeded",
            "Reason: Parsing exceeded the judgment budget (1000 parses or 5 seconds), or the parser process terminated abnormally.",
        ),
    };
    let mut text = format!("{title}\n{reason}\nAlternative: No applicable alternative.");
    if asks.len() > 1 {
        text.push_str(&format!("\nAdditional findings: {}", asks.len() - 1));
    }
    text
}

/// 操作とパスが無い ask の理由を 1 行にする。上限を超えたものは block の
/// 理由にする（REQ-011）。
pub fn ask_reason(asks: &[Ask]) -> String {
    if asks.iter().any(Ask::is_limit) {
        return limit_reason(asks);
    }
    match asks.first() {
        None => String::new(),
        Some(ask) => match ask_kind(ask) {
            AskKind::Syntax => "Cannot judge: command syntax could not be read".to_string(),
            AskKind::Internal => "Cannot judge: an internal failure occurred".to_string(),
            AskKind::Shell => match ask {
                Ask::UnsupportedShell(name) => format!("Unsupported shell: {name}"),
                _ => "Unsupported shell".to_string(),
            },
            AskKind::TooLarge | AskKind::TooDeep | AskKind::Limit => limit_reason(asks),
        },
    }
}

/// 操作とパスが無い ask の文面。理由に応じて 2〜4 行で組み立てる。
pub fn ask_message(asks: &[Ask]) -> String {
    let Some(first) = asks.first() else {
        return String::new();
    };
    if asks.iter().any(Ask::is_limit) {
        return limit_message(asks);
    }
    let lines = match ask_kind(first) {
        AskKind::Syntax => vec![
            "Cannot judge: command syntax could not be read".to_string(),
            "Reason: The command syntax could not be parsed.".to_string(),
            "Alternative: No applicable alternative.".to_string(),
        ],
        AskKind::Internal => vec![
            "Cannot judge: an internal failure occurred".to_string(),
            "Reason: An internal judgment failure occurred.".to_string(),
            "Alternative: No applicable alternative.".to_string(),
        ],
        AskKind::TooLarge | AskKind::TooDeep | AskKind::Limit => return limit_message(asks),
        AskKind::Shell => {
            let name = match first {
                Ask::UnsupportedShell(name) => name.clone(),
                _ => "unknown shell".to_string(),
            };
            vec![
                format!("Unsupported shell: {}", display_inline(&name)),
                "Reason: Bodies of unsupported shells are not analyzed.".to_string(),
                "Alternative: No applicable alternative.".to_string(),
            ]
        }
    };
    let mut text = lines.join("\n");
    if asks.len() > 1 {
        text.push_str(&format!("\nAdditional findings: {}", asks.len() - 1));
    }
    text
}
