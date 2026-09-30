//! S5: 対象集合が未知の効果（REQ-008）。

use guardian_core::{extract_effects, Class, Effect, Target, Verdict};
use guardian_judge::{Classification, Judge};
use std::path::Path;

mod common;
use common::{core_env, fixture, judge_for, verdict_of};

fn effects(root: &Path, command: &str) -> Vec<Effect> {
    extract_effects(command, &core_env(root))
}

fn classify(root: &Path, target: &Target) -> Classification {
    let judge: Judge = judge_for(root);
    match target {
        Target::Children(base) => judge.classify_children(base),
        Target::Path { path, dereference } => judge.classify_path(path, *dereference),
        Target::GlobBase(base) => judge.classify_path(base, false),
        Target::Mktemp => Classification {
            class: Class::Ephemeral,
            why: guardian_core::Why::Mktemp,
        },
        Target::UnknownSource => Classification {
            class: Class::Unknown,
            why: guardian_core::Why::UnknownSource,
        },
        Target::Unresolved(_) => Classification {
            class: Class::Unknown,
            why: guardian_core::Why::Unresolved(String::new()),
        },
    }
}

fn worst_verdict(root: &Path, command: &str) -> Verdict {
    let mut verdict = Verdict::Allow;
    for e in effects(root, command) {
        verdict = verdict.worst(verdict_of(classify(root, &e.target).class));
    }
    verdict
}

// @kotowari[REQ-008, EX-006]
#[test]
fn req_008_find_into_ignored_directory_allows() {
    let f = fixture();
    std::fs::create_dir_all(f.root.join("node_modules/.vite")).unwrap();
    std::fs::write(f.root.join("node_modules/.vite/dep"), "x").unwrap();
    assert_eq!(
        worst_verdict(&f.root, "find node_modules/.vite -delete"),
        Verdict::Allow
    );
}

// @kotowari[REQ-008, EX-007]
#[test]
fn req_008_find_into_dirty_worktree_asks() {
    let f = fixture();
    std::fs::write(f.root.join("src/a.rs"), "fn a() { changed }\n").unwrap();
    assert_eq!(
        worst_verdict(&f.root, "find . -name __pycache__ -exec rm -rf {} +"),
        Verdict::Ask
    );
}

// @kotowari[REQ-008]
#[test]
fn req_008_xargs_asks_when_source_is_dirty_and_allows_when_ignored() {
    let f = fixture();
    std::fs::write(f.root.join("src/a.rs"), "fn a() { changed }\n").unwrap();
    assert_eq!(
        worst_verdict(&f.root, "find . -type f | xargs rm -f"),
        Verdict::Ask
    );
    std::fs::create_dir_all(f.root.join("node_modules/.vite")).unwrap();
    std::fs::write(f.root.join("node_modules/.vite/dep"), "x").unwrap();
    assert_eq!(
        worst_verdict(&f.root, "find node_modules/.vite -type f | xargs rm -f"),
        Verdict::Allow
    );
    // 供給元が無ければ ask。
    assert_eq!(worst_verdict(&f.root, "xargs rm -f"), Verdict::Ask);
}

// @kotowari[REQ-008]
#[test]
fn req_008_for_loop_with_literal_list_uses_each_item() {
    let f = fixture();
    assert_eq!(
        worst_verdict(&f.root, "for x in /tmp/a /tmp/b; do rm -rf $x; done"),
        Verdict::Allow
    );
    assert_eq!(
        worst_verdict(&f.root, "for x in /etc/passwd; do rm -f $x; done"),
        Verdict::Block
    );
}

// @kotowari[REQ-008]
#[test]
fn req_008_for_loop_with_unknown_source_asks() {
    let f = fixture();
    assert_eq!(
        worst_verdict(&f.root, "for x in $(cat list); do rm -rf $x; done"),
        Verdict::Ask
    );
}

// @kotowari[REQ-008]
#[test]
fn req_008_find_into_protected_areas_blocks() {
    let f = fixture();
    assert_eq!(
        worst_verdict(&f.root, "find /usr/lib -delete"),
        Verdict::Block
    );
    let dot_git = f.root.join(".git");
    assert_eq!(
        worst_verdict(&f.root, &format!("find {} -delete", dot_git.display())),
        Verdict::Block
    );
}
