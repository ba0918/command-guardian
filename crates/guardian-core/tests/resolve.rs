//! S2: パスの解決（REQ-002）。

use guardian_core::{extract_effects, Effect, Env, Op, Target};
use std::path::PathBuf;

fn env() -> Env {
    Env {
        home: Some(PathBuf::from("/home/you")),
        tmpdir: Some(PathBuf::from("/tmp")),
        cwd: Some(PathBuf::from("/home/you/work/repo")),
    }
}

fn delete(target: Target) -> Effect {
    Effect {
        op: Op::Delete,
        target,
    }
}

fn effects(command: &str) -> Vec<Effect> {
    extract_effects(command, &env())
}

fn p(path: &str) -> Target {
    Target::Path {
        path: PathBuf::from(path),
        dereference: false,
    }
}

// @kotowari[REQ-002]
#[test]
fn req_002_absolute_relative_and_tilde_paths_resolve() {
    assert_eq!(effects("rm -rf /tmp/x"), vec![delete(p("/tmp/x"))]);
    assert_eq!(
        effects("rm -rf notes.txt"),
        vec![delete(Target::Path {
            path: PathBuf::from("/home/you/work/repo/notes.txt"),
            dereference: false,
        })]
    );
    assert_eq!(
        effects("rm -rf ~/notes.txt"),
        vec![delete(p("/home/you/notes.txt"))]
    );
}

// @kotowari[REQ-002]
#[test]
fn req_002_home_tmpdir_and_pwd_resolve() {
    assert_eq!(
        effects("rm -rf $HOME/notes.txt"),
        vec![delete(p("/home/you/notes.txt"))]
    );
    assert_eq!(
        effects("rm -rf $TMPDIR/scratch"),
        vec![delete(p("/tmp/scratch"))]
    );
    assert_eq!(
        effects("rm -rf $PWD/built"),
        vec![delete(p("/home/you/work/repo/built"))]
    );
}

// @kotowari[REQ-002]
#[test]
fn req_002_literal_assignment_resolves() {
    assert_eq!(
        effects("S=/tmp/scratch/review; rm -rf $S"),
        vec![delete(p("/tmp/scratch/review"))]
    );
    assert_eq!(
        effects("S=/tmp/scratch && rm -rf $S"),
        vec![delete(p("/tmp/scratch"))]
    );
}

// @kotowari[REQ-002]
#[test]
fn req_002_literal_cd_moves_relative_paths() {
    assert_eq!(
        effects("cd /tmp/scratch && rm -rf build"),
        vec![delete(p("/tmp/scratch/build"))]
    );
}

// @kotowari[REQ-002]
#[test]
fn req_002_cd_dash_is_not_a_directory_named_dash() {
    // `cd -` は OLDPWD で、REQ-002 が解決できるとするパスではない。
    // cwd 相対の "-" にせず、続く相対パスは未解決として扱う。
    assert_eq!(
        effects("cd -; rm -rf x"),
        vec![delete(Target::Unresolved("x".to_string()))]
    );
}

// @kotowari[REQ-002]
#[test]
fn req_002_mktemp_paths_resolve() {
    assert_eq!(
        effects("d=$(mktemp -d); rm -rf \"$d\""),
        vec![delete(Target::Mktemp)]
    );
}

// @kotowari[REQ-002]
#[test]
fn req_002_glob_base_is_outermost_directory() {
    assert_eq!(
        effects("rm -rf /tmp/scratch/*"),
        vec![delete(Target::GlobBase(PathBuf::from("/tmp/scratch")))]
    );
    assert_eq!(
        effects("rm -rf *"),
        vec![delete(Target::GlobBase(PathBuf::from(
            "/home/you/work/repo"
        )))]
    );
    assert_eq!(
        effects("rm -rf ~/build/*.o"),
        vec![delete(Target::GlobBase(PathBuf::from("/home/you/build")))]
    );
    assert_eq!(
        effects("S=/tmp/scratch; rm -rf $S/logs/*"),
        vec![delete(Target::GlobBase(PathBuf::from("/tmp/scratch/logs")))]
    );
}

// @kotowari[REQ-002]
#[test]
fn req_002_unresolvable_paths_are_distinguished() {
    assert_eq!(
        effects("rm -rf $X"),
        vec![delete(Target::Unresolved("$X".to_string()))]
    );
    assert_eq!(
        effects("rm -rf $(cat /tmp/list)"),
        vec![delete(Target::Unresolved("$(cat /tmp/list)".to_string()))]
    );
    // 代入の値が解決できないと、それを使うパスも未解決。
    // 文面には根の `$X` を出す。
    assert_eq!(
        effects("S=$X; rm -rf $S"),
        vec![delete(Target::Unresolved("$X".to_string()))]
    );
}

// @kotowari[REQ-002]
#[test]
fn req_002_paths_built_on_mktemp_resolve() {
    assert_eq!(
        effects("d=$(mktemp -d); rm -rf $d/*"),
        vec![delete(Target::Mktemp)]
    );
    assert_eq!(
        effects("d=$(mktemp -d); rm -rf $d/log"),
        vec![delete(Target::Mktemp)]
    );
    // mktemp の外へ出る綴りは未解決のまま。
    assert_eq!(
        effects("d=$(mktemp -d); rm -rf $d/../x"),
        vec![delete(Target::Unresolved("$d/../x".to_string()))]
    );
}

// @kotowari[REQ-002]
#[test]
fn req_002_other_user_tilde_paths_are_unresolved() {
    // "~" と "~/" 以外の "~..." は、cwd 相対にせず未解決として扱う。
    assert_eq!(
        effects("rm -rf ~root/x"),
        vec![delete(Target::Unresolved("~root/x".to_string()))]
    );
    assert_eq!(
        effects("rm -rf ~+/x"),
        vec![delete(Target::Unresolved("~+/x".to_string()))]
    );
}
