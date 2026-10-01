//! S2: パスの解決（REQ-002）。

mod common;
use common::extract_effects;
use guardian_core::{Effect, Env, Op, Target};
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
fn req_002_assignment_values_are_expanded_before_resolving_the_target_path() {
    assert_eq!(effects("S=etc; rm -rf /$S/x"), vec![delete(p("/etc/x"))]);
    assert_eq!(
        effects("S=x; cd /etc; rm \"$S\""),
        vec![delete(p("/etc/x"))]
    );
    assert_eq!(
        effects("S=etc; T=/$S/x; rm \"$T\""),
        vec![delete(p("/etc/x"))]
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

// @kotowari[REQ-001, REQ-002]
#[test]
fn req_002_subshell_and_function_definition_do_not_change_parent_bindings() {
    for command in [
        "S=/etc/x; (S=/tmp/x); rm \"$S\"",
        "S=/etc/x; f() { S=/tmp/x; }; rm \"$S\"",
    ] {
        assert_eq!(effects(command), vec![delete(p("/etc/x"))], "{command}");
    }
    assert_eq!(
        effects("(cd /tmp); rm x"),
        vec![delete(p("/home/you/work/repo/x"))]
    );
    assert_eq!(effects("{ cd /tmp; }; rm x"), vec![delete(p("/tmp/x"))]);
    assert_eq!(effects("(rm /etc/x)"), vec![delete(p("/etc/x"))]);
}

// @kotowari[REQ-001, REQ-002]
#[test]
fn req_002_eval_updates_parent_state_but_shell_commands_do_not() {
    assert_eq!(
        effects("S=/tmp/x; eval 'S=/etc/x'; rm \"$S\""),
        vec![delete(p("/etc/x"))]
    );
    assert_eq!(effects("eval 'cd /etc'; rm x"), vec![delete(p("/etc/x"))]);
    assert_eq!(
        effects("bash -c 'cd /etc'; rm x"),
        vec![delete(p("/home/you/work/repo/x"))]
    );
}

// @kotowari[REQ-002, REQ-008]
#[test]
fn req_002_literal_loop_state_survives_iterations_and_loop_completion() {
    assert_eq!(
        effects("S=/tmp/x; for x in /a; do S=/etc/x; done; rm \"$S\""),
        vec![delete(p("/etc/x"))]
    );
    assert_eq!(
        effects("cd /tmp; for x in /a /b; do cd child; rm x; done; rm y"),
        vec![
            delete(p("/tmp/child/x")),
            delete(p("/tmp/child/child/x")),
            delete(p("/tmp/child/child/y")),
        ]
    );
}

// @kotowari[REQ-002, REQ-007]
#[test]
fn req_007_variable_trailing_slash_is_preserved_for_dereferencing() {
    for command in ["S=/tmp/link/; rm -rf \"$S\"", "S=/tmp/link; rm -rf \"$S/\""] {
        assert_eq!(
            effects(command),
            vec![delete(Target::Path {
                path: PathBuf::from("/tmp/link"),
                dereference: true,
            })]
        );
    }
    assert_eq!(
        effects("S=/tmp/link; rm \"$S\""),
        vec![delete(p("/tmp/link"))]
    );
    assert_eq!(
        effects("S=/tmp/link/; find \"$S\" -delete"),
        vec![delete(Target::Children {
            base: PathBuf::from("/tmp/link"),
            dereference: true,
        })]
    );
}

// @kotowari[REQ-008]
#[test]
fn req_008_glob_loop_binding_represents_children_not_the_source_root() {
    for base in ["/tmp", "/etc"] {
        assert_eq!(
            effects(&format!("for x in {base}/*; do rm \"$x\"; done")),
            vec![delete(Target::Children {
                base: PathBuf::from(base),
                dereference: false,
            })]
        );
    }
    assert_eq!(
        effects("for x in /tmp/*; do rm \"$x/child\"; done"),
        vec![delete(Target::Unresolved("$x/child".into()))]
    );
}

// @kotowari[REQ-002, REQ-041]
#[test]
fn req_002_only_unquoted_tilde_is_expanded_to_home() {
    for command in ["rm '~/x'", "rm \"~/x\"", "S='~/x'; rm \"$S\""] {
        assert_eq!(effects(command), vec![delete(p("/home/you/work/repo/~/x"))]);
    }
    assert_eq!(effects("rm ~/x"), vec![delete(p("/home/you/x"))]);
    assert_eq!(
        effects("S=~/x; cd /etc; rm \"$S\""),
        vec![delete(p("/home/you/x"))]
    );
}

// @kotowari[REQ-002, REQ-041]
#[test]
fn req_002_quoted_glob_characters_are_literal_components_of_the_glob_base() {
    for name in ["a*b", "a?b", "a[b]"] {
        assert_eq!(
            effects(&format!("rm '/tmp/{name}/'*")),
            vec![delete(Target::GlobBase(PathBuf::from(format!(
                "/tmp/{name}"
            ))))]
        );
    }
    assert_eq!(
        effects("rm '/tmp/a*b/'suffix*"),
        vec![delete(Target::GlobBase(PathBuf::from("/tmp/a*b")))]
    );
}

// @kotowari[REQ-002]
#[test]
fn req_002_mktemp_dry_run_is_not_a_created_temporary_path() {
    for options in ["-u", "--dry-run", "-du"] {
        let command = format!("rm \"$(mktemp {options} /etc/guardian.XXXXXX)\"");
        let effects = effects(&command);
        assert_eq!(effects.len(), 1);
        assert!(
            matches!(effects[0].target, Target::Unresolved(_)),
            "{effects:?}"
        );
    }
    assert_eq!(
        effects("d=$(mktemp -d); rm \"$d\""),
        vec![delete(Target::Mktemp)]
    );
}
