//! S6: symlink・合成・内部エラー（REQ-007, REQ-009, REQ-010）。

use guardian_core::{analyze, extract_effects, Class, Env, Target, Verdict, Why};
use guardian_judge::{Classification, Judge, JudgeEnv};
use std::path::PathBuf;

fn core_env() -> Env {
    Env {
        home: Some(PathBuf::from("/home/you")),
        tmpdir: Some(PathBuf::from("/tmp")),
        cwd: Some(PathBuf::from("/home/you/work/repo")),
    }
}

fn judge() -> Judge {
    Judge::new(JudgeEnv {
        home: Some(PathBuf::from("/home/you")),
        tmpdir: Some(PathBuf::from("/tmp")),
        cwd: Some(PathBuf::from("/home/you/work/repo")),
        git_enabled: true,
    })
}

fn classify(target: &Target) -> Classification {
    match target {
        Target::Path { path, dereference } => judge().classify_path(path, *dereference),
        Target::GlobBase(base) => judge().classify_path(base, false),
        Target::Mktemp => Classification {
            class: Class::Ephemeral,
            why: Why::Mktemp,
        },
        Target::Unresolved(_) => Classification {
            class: Class::Unknown,
            why: Why::Unresolved(String::new()),
        },
        other => panic!("unexpected target: {other:?}"),
    }
}

fn verdict(class: Class) -> Verdict {
    match class {
        Class::Ephemeral | Class::Vcs => Verdict::Allow,
        Class::Protected => Verdict::Block,
        Class::Unknown => Verdict::Ask,
    }
}

// @kotowari[REQ-007, EX-008]
#[test]
fn req_007_symlink_is_classified_as_the_link_itself() {
    // リンクを置く一時領域を作る。
    let scratch = tempfile::tempdir().unwrap();
    let link = scratch.path().join("link");
    // canonicalize で確実に保護領域へ着地させるため、存在する /etc を指す。
    std::os::unix::fs::symlink("/etc", &link).unwrap();

    // 末尾スラッシュ無し: リンクそれ自体（一時領域）として allow。
    let effects = extract_effects(&format!("rm {}", link.display()), &core_env());
    assert_eq!(effects.len(), 1);
    let c = classify(&effects[0].target);
    assert_eq!(c.class, Class::Ephemeral);
    assert_eq!(verdict(c.class), Verdict::Allow);

    // 末尾スラッシュ付き: リンク先（/etc）を解決して block。
    let effects = extract_effects(&format!("rm -rf {}/", link.display()), &core_env());
    assert_eq!(effects.len(), 1);
    assert_eq!(
        effects[0].target,
        Target::Path {
            path: link.clone(),
            dereference: true
        }
    );
    let c = classify(&effects[0].target);
    assert_eq!(c.class, Class::Protected);
    assert_eq!(verdict(c.class), Verdict::Block);
}

// @kotowari[REQ-007]
#[test]
fn req_007_hardlink_is_one_link() {
    let scratch = tempfile::tempdir().unwrap();
    let a = scratch.path().join("a");
    let b = scratch.path().join("b");
    std::fs::write(&a, "x").unwrap();
    std::fs::hard_link(&a, &b).unwrap();
    let effects = extract_effects(&format!("rm {}", b.display()), &core_env());
    assert_eq!(effects.len(), 1);
    // リンク元の分類は普通のパスと同じ。
    assert_eq!(verdict(classify(&effects[0].target).class), Verdict::Allow);
}

// @kotowari[REQ-009, EX-009]
#[test]
fn req_009_worst_verdict_wins() {
    let effects = extract_effects("rm -rf /tmp/scratch/x /etc/foo", &core_env());
    assert_eq!(effects.len(), 2);
    let mut worst = Verdict::Allow;
    for e in &effects {
        worst = worst.worst(verdict(classify(&e.target).class));
    }
    assert_eq!(worst, Verdict::Block);

    let effects = extract_effects("rm -rf /tmp/scratch/x /tmp/scratch/y", &core_env());
    let mut worst = Verdict::Allow;
    for e in &effects {
        worst = worst.worst(verdict(classify(&e.target).class));
    }
    assert_eq!(worst, Verdict::Allow);
}

// @kotowari[REQ-010]
#[test]
fn req_010_parse_failure_is_reported() {
    // 閉じない引用を含む入力は解析の失敗として報告する。
    for cmd in [
        "rm -rf '/etc/foo",
        "rm -rf \"/etc/foo",
        "rm -rf $(rm /etc/foo",
    ] {
        let a = analyze(cmd, &core_env());
        assert!(!a.parse_errors.is_empty(), "{cmd}");
    }
}

// @kotowari[REQ-010]
#[test]
fn req_010_unclosed_substitution_is_an_error() {
    // 閉じないコマンド置換も解析の失敗として報告する。
    let a = analyze("rm -rf $(cat /tmp/list", &core_env());
    assert!(!a.parse_errors.is_empty());
}
