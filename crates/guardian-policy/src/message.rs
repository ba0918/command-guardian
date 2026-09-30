//! 非 allow の文面。「何を・なぜ・代替」を 2〜4 行で組み立てる。

use guardian_core::{Class, Op, ProtectedKind, Target, Why};

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
        Target::Children { base, .. } => format!("{} の配下", base.display()),
        Target::Mktemp => "mktemp が作ったパス".to_string(),
        Target::UnknownSource => "供給元が分からない対象集合".to_string(),
        Target::Unresolved(text) => format!("{text}（解決できないパス）"),
    }
}

pub fn op_label(op: Op) -> &'static str {
    match op {
        Op::Delete => "削除",
        Op::Truncate => "切り詰め",
        Op::Format => "フォーマット",
    }
}

fn protected_label(kind: &ProtectedKind) -> &'static str {
    match kind {
        ProtectedKind::Root => "ルートディレクトリ",
        ProtectedKind::SystemArea => "システムの領域",
        ProtectedKind::EphemeralRoot => "一時領域のルート",
        ProtectedKind::Home => "ホームディレクトリ",
        ProtectedKind::Cwd => "作業ディレクトリ",
        ProtectedKind::RepoRoot => "リポジトリのルート",
        ProtectedKind::DotGit => ".git",
        ProtectedKind::OtherHome => "ほかの利用者のホーム",
        ProtectedKind::ConfiguredRoot => "設定で追加した保護ルート",
    }
}

fn reason_phrase(class: Class, why: &Why) -> String {
    let detail = match why {
        Why::Ephemeral => String::new(),
        Why::Mktemp => String::new(),
        Why::Vcs => String::new(),
        Why::Untracked => "（未追跡）".to_string(),
        Why::Uncommitted => "（未コミットの変更あり）".to_string(),
        Why::Unmanaged => "（管理外）".to_string(),
        Why::Unresolved(_) => "（パスを解決できない）".to_string(),
        Why::UnknownSource => "（対象集合を確定できない）".to_string(),
        Why::GitFailed => "（git の確認に失敗）".to_string(),
        Why::Protected(kind) => format!("（{}）", protected_label(kind)),
    };
    format!("{class}{detail}")
}

fn loss_phrase(why: &Why) -> &'static str {
    match why {
        Why::Untracked => "まだコミットされておらず、消えると戻せません",
        Why::Uncommitted => "コミットされていない変更が失われます",
        Why::Unmanaged => "git の管理下になく、消えると戻せません",
        Why::Unresolved(_) => "対象を確定できず、何が消えるか分かりません",
        Why::UnknownSource => "消える範囲を確定できません",
        Why::GitFailed => "管理状態を確認できません",
        Why::Protected(_) => "消えると戻せません",
        Why::Ephemeral | Why::Mktemp | Why::Vcs => "消えると戻せません",
    }
}

fn alternative_phrase(why: &Why) -> &'static str {
    match why {
        Why::Untracked => "一時領域や作業場所へ移してから消す、または先にコミットする",
        Why::Uncommitted => "先にコミットする",
        Why::Unmanaged => "一時領域や作業場所へ移してから消す、または許可ルートに追加する",
        Why::Unresolved(_) => "リテラルのパスで指定し直す",
        Why::UnknownSource => "対象をリテラルで指定し直す",
        Why::GitFailed => "git status で管理状態を確かめてから消す",
        Why::Protected(_) => "当てはまる代替はありません",
        Why::Ephemeral | Why::Mktemp | Why::Vcs => "当てはまる代替はありません",
    }
}

/// 非 allow の理由を 1 行にする。
pub fn reason_line(class: Class, why: &Why) -> String {
    format!("{}で、{}", reason_phrase(class, why), loss_phrase(why))
}

/// 非 allow の文面。`block` はエージェント向け、`ask` は利用者向けに書く。
pub fn non_allow_message(op: Op, target: &Target, class: Class, why: &Why) -> String {
    let lines = [
        format!("{}: {}", op_label(op), display_target(target)),
        format!("理由: {}", reason_line(class, why)),
        format!("代替: {}", alternative_phrase(why)),
    ];
    lines.join("\n")
}
