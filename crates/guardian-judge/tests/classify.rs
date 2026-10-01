//! S3: 一時領域と保護領域の分類と判定（REQ-003, REQ-005, REQ-006）。
//!
//! 分類から判定への対応は policy の責務だが、この段では仕様の対応表
//! (REQ-006) をテスト側の写像で固定し、分類そのものを確かめる。

use guardian_core::{extract_effects, Class, Effect, Env, Op, Target, Verdict};
use guardian_judge::{Classification, Judge, JudgeEnv};
use std::path::{Path, PathBuf};

fn judge_env() -> JudgeEnv {
    JudgeEnv {
        home: Some(PathBuf::from("/home/you")),
        tmpdir: Some(PathBuf::from("/tmp")),
        cwd: Some(PathBuf::from("/home/you/work/repo")),
        git_enabled: true,
    }
}

fn judge() -> Judge {
    Judge::new(judge_env())
}

fn classify(path: &str) -> Classification {
    judge().classify_path(Path::new(path), false)
}

fn verdict(class: Class) -> Verdict {
    match class {
        Class::Ephemeral | Class::Vcs => Verdict::Allow,
        Class::Protected => Verdict::Block,
        Class::Unknown => Verdict::Ask,
    }
}

fn verdict_for(target: &Target) -> Verdict {
    match target {
        Target::Unresolved(_) => Verdict::Block,
        Target::Mktemp => Verdict::Allow,
        Target::UnknownSource => Verdict::Ask,
        Target::Path { path, dereference } => {
            verdict(judge().classify_path(path, *dereference).class)
        }
        Target::GlobBase(base) => verdict(judge().classify_path(base, false).class),
        Target::Children { base, .. } => verdict(judge().classify_children(base).class),
    }
}

fn core_env() -> Env {
    Env {
        home: Some(PathBuf::from("/home/you")),
        tmpdir: Some(PathBuf::from("/tmp")),
        cwd: Some(PathBuf::from("/home/you/work/repo")),
    }
}

fn effect(command: &str) -> Effect {
    let mut effects = extract_effects(command, &core_env());
    assert_eq!(effects.len(), 1, "{effects:?}");
    effects.remove(0)
}

// @kotowari[REQ-003, EX-001]
#[test]
fn req_003_ephemeral_region_allows() {
    // EX-001: 代入された一時領域のパスを解決して通す。
    let e = effect("S=/tmp/scratch/review; rm -rf $S");
    assert_eq!(e.op, Op::Delete);
    assert_eq!(verdict_for(&e.target), Verdict::Allow);
    assert_eq!(classify("/tmp/scratch/review").class, Class::Ephemeral);
    assert_eq!(classify("/var/tmp/build").class, Class::Ephemeral);
    assert_eq!(verdict_for(&Target::Mktemp), Verdict::Allow);
}

// @kotowari[REQ-003, EX-004]
#[test]
fn req_003_ephemeral_roots_are_protected() {
    // EX-004: 一時領域のルートそれ自体は止める。
    assert_eq!(
        verdict_for(&Target::Path {
            path: PathBuf::from("/tmp"),
            dereference: false,
        }),
        Verdict::Block
    );
    assert_eq!(
        verdict_for(&Target::Path {
            path: PathBuf::from("/var/tmp"),
            dereference: false,
        }),
        Verdict::Block
    );
    // TMPDIR のルートそれ自体。
    let j = Judge::new(JudgeEnv {
        tmpdir: Some(PathBuf::from("/home/you/tmp")),
        ..judge_env()
    });
    assert_eq!(
        j.classify_path(Path::new("/home/you/tmp"), false).class,
        Class::Protected
    );
    assert_eq!(
        j.classify_path(Path::new("/home/you/tmp/x"), false).class,
        Class::Ephemeral
    );
}

// @kotowari[REQ-005, EX-024]
#[test]
fn req_005_system_areas_are_protected_with_descendants() {
    // EX-024: システムの領域は止める。
    assert_eq!(
        verdict_for(&Target::Path {
            path: PathBuf::from("/usr/local/lib/foo"),
            dereference: false,
        }),
        Verdict::Block
    );
    for p in [
        "/etc",
        "/etc/nginx",
        "/usr",
        "/bin/ls",
        "/var/log/x",
        "/opt/app",
    ] {
        assert_eq!(classify(p).class, Class::Protected, "{p}");
    }
    // /var/tmp とその配下だけは保護領域にしない。
    assert_eq!(classify("/var/tmp/x").class, Class::Ephemeral);
}

// @kotowari[REQ-005]
#[test]
fn req_005_root_dot_git_other_home_home_cwd_are_protected() {
    assert_eq!(classify("/").class, Class::Protected);
    assert_eq!(classify("/repo/.git").class, Class::Protected);
    assert_eq!(classify("/repo/.git/config").class, Class::Protected);
    assert_eq!(classify("/home/other/notes.txt").class, Class::Protected);
    assert_eq!(classify("/home/you").class, Class::Protected);
    assert_eq!(classify("/home/you/work/repo").class, Class::Protected);
    assert_eq!(classify("/home/you/work/repo/src").class, Class::Unknown);
}

// @kotowari[REQ-005, EX-029]
#[test]
fn req_005_repo_root_itself_is_protected() {
    // EX-029: glob は base の分類で判定する。
    // 一時領域（/tmp）の中では ephemeral が勝つため、作業ツリーの外の場所に作る。
    let dir = tempfile::Builder::new()
        .prefix("command-guardian-repo-")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    let root = dir.path().canonicalize().unwrap();
    // フィクスチャは実ユーザのホームの下にあるため、home を実環境に合わせる。
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let j = Judge::new(JudgeEnv {
        home,
        tmpdir: Some(PathBuf::from("/tmp")),
        cwd: Some(root.clone()),
        git_enabled: true,
    });
    assert_eq!(j.classify_path(&root, false).class, Class::Protected);
    // リポジトリのルートそれ自体だけが保護。配下は git の分類に従う。
    assert_eq!(j.classify_path(&root.join("src"), false).class, Class::Vcs);
    // glob の base がリポジトリのルートに一致するときは block。
    let cwd = root.clone();
    let mut effects = extract_effects(
        "rm -rf *",
        &Env {
            home: Some(PathBuf::from("/home/you")),
            tmpdir: Some(PathBuf::from("/tmp")),
            cwd: Some(cwd),
        },
    );
    assert_eq!(effects.len(), 1);
    let e = effects.remove(0);
    assert_eq!(e.target, Target::GlobBase(root.clone()));
    let classified = j.classify_path(&root, false);
    assert_eq!(verdict(classified.class), Verdict::Block);
}

// @kotowari[REQ-005]
#[test]
fn req_005_system_area_children_are_blocked_for_unknown_sets() {
    let j = judge();
    assert_eq!(
        j.classify_children(Path::new("/usr/lib")).class,
        Class::Protected
    );
    assert_eq!(
        j.classify_children(Path::new("/tmp")).class,
        Class::Ephemeral
    );
}

// @kotowari[REQ-006, EX-003]
#[test]
fn req_006_class_maps_to_verdict_and_unresolved_blocks() {
    // EX-003: 解決できないパスは止める。
    let e = effect("rm -rf $X");
    assert!(matches!(e.target, Target::Unresolved(_)), "{:?}", e.target);
    assert_eq!(verdict_for(&e.target), Verdict::Block);
}

// @kotowari[REQ-003, REQ-005]
#[test]
fn req_003_005_ephemeral_subtree_is_not_protected() {
    assert_eq!(classify("/tmp/scratch/deep/dir").class, Class::Ephemeral);
    assert_eq!(
        judge().classify_children(Path::new("/tmp/scratch")).class,
        Class::Ephemeral
    );
}
