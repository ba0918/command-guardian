//! S4: git による分類（REQ-004, REQ-020）。

use guardian_core::{extract_effects, Class, Target, Verdict, Why};
use guardian_judge::{Classification, GitError, GitRunner, Judge, JudgeEnv};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

mod common;
use common::{core_env, fixture, judge_for};

fn classify_effect(root: &Path, command: &str) -> Classification {
    let effects = extract_effects(command, &core_env(root));
    assert_eq!(effects.len(), 1, "{effects:?}");
    let judge = judge_for(root);
    match &effects[0].target {
        Target::Path { path, dereference } => judge.classify_path(path, *dereference),
        other => panic!("unexpected target: {other:?}"),
    }
}

// @kotowari[REQ-004, EX-005]
#[test]
fn req_004_ignored_generated_files_are_vcs() {
    let f = fixture();
    std::fs::create_dir_all(f.root.join("ignored/build")).unwrap();
    std::fs::write(f.root.join("ignored/build/out.o"), "x").unwrap();
    let c = classify_effect(&f.root, "rm -rf ignored");
    assert_eq!(c.class, Class::Vcs);
    assert_eq!(c.why, Why::Vcs);
}

// @kotowari[REQ-004, EX-002]
#[test]
fn req_004_untracked_file_is_unknown_with_untracked_reason() {
    let f = fixture();
    std::fs::write(f.root.join("notes.txt"), "notes\n").unwrap();
    let c = classify_effect(&f.root, "rm notes.txt");
    assert_eq!(c.class, Class::Unknown);
    assert_eq!(c.why, Why::Untracked);
}

// @kotowari[REQ-004, EX-028]
#[test]
fn req_004_directory_with_uncommitted_changes_is_ask() {
    let f = fixture();
    std::fs::write(f.root.join("src/a.rs"), "fn a() { changed }\n").unwrap();
    let c = classify_effect(&f.root, "rm -rf src");
    assert_eq!(c.class, Class::Unknown);
    assert_eq!(c.why, Why::Uncommitted);
}

// @kotowari[REQ-004]
#[test]
fn req_004_clean_tracked_path_is_vcs() {
    let f = fixture();
    let c = classify_effect(&f.root, "rm tracked.txt");
    assert_eq!(c.class, Class::Vcs);
}

#[derive(Clone)]
struct RecordingGit {
    calls: std::sync::Arc<Mutex<Vec<(PathBuf, PathBuf)>>>,
    result: Result<String, GitError>,
}

impl GitRunner for RecordingGit {
    fn status(&self, root: &Path, path: &Path) -> Result<String, GitError> {
        self.calls
            .lock()
            .unwrap()
            .push((root.to_path_buf(), path.to_path_buf()));
        self.result.clone()
    }
}

fn recording(result: Result<String, GitError>) -> RecordingGit {
    RecordingGit {
        calls: std::sync::Arc::new(Mutex::new(Vec::new())),
        result,
    }
}

// @kotowari[REQ-020, EX-019]
#[test]
fn req_020_git_is_not_invoked_outside_a_worktree() {
    let git = recording(Ok(String::new()));
    let judge = Judge::with_git(
        JudgeEnv {
            home: Some(PathBuf::from("/home/you")),
            tmpdir: Some(PathBuf::from("/tmp")),
            cwd: Some(PathBuf::from("/home/you/work/repo")),
            git_enabled: true,
        },
        Box::new(git.clone()),
    );
    let c = judge.classify_path(Path::new("/home/you/work/repo/notes.txt"), false);
    assert_eq!(c.class, Class::Unknown);
    assert!(git.calls.lock().unwrap().is_empty());
    assert_eq!(
        Verdict::Ask,
        match c.class {
            Class::Ephemeral | Class::Vcs => Verdict::Allow,
            Class::Protected => Verdict::Block,
            Class::Unknown => Verdict::Ask,
        }
    );
}

// @kotowari[REQ-020]
#[test]
fn req_020_git_is_invoked_inside_a_worktree() {
    let f = fixture();
    std::fs::write(f.root.join("notes.txt"), "notes\n").unwrap();
    let git = recording(Ok("?? notes.txt\n".to_string()));
    let judge = Judge::with_git(
        JudgeEnv {
            home: std::env::var_os("HOME").map(PathBuf::from),
            tmpdir: Some(PathBuf::from("/tmp")),
            cwd: Some(f.root.clone()),
            git_enabled: true,
        },
        Box::new(git.clone()),
    );
    let c = judge.classify_path(&f.root.join("notes.txt"), false);
    assert_eq!(c.why, Why::Untracked);
    let calls = git.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, f.root);
    assert!(calls[0].1.ends_with("notes.txt"));
}

// @kotowari[REQ-010, EX-011]
#[test]
fn req_010_git_failure_falls_to_ask() {
    let f = fixture();
    let git = recording(Err(GitError("boom".to_string())));
    let judge = Judge::with_git(
        JudgeEnv {
            home: std::env::var_os("HOME").map(PathBuf::from),
            tmpdir: Some(PathBuf::from("/tmp")),
            cwd: Some(f.root.clone()),
            git_enabled: true,
        },
        Box::new(git),
    );
    let c = judge.classify_path(&f.root.join("tracked.txt"), false);
    assert_eq!(c.class, Class::Unknown);
    assert_eq!(c.why, Why::GitFailed);
    assert_ne!(
        match c.class {
            Class::Ephemeral | Class::Vcs => Verdict::Allow,
            Class::Protected => Verdict::Block,
            Class::Unknown => Verdict::Ask,
        },
        Verdict::Block
    );
}

// @kotowari[REQ-004]
#[test]
fn req_004_git_disabled_keeps_worktree_paths_unknown() {
    let f = fixture();
    let judge = Judge::new(JudgeEnv {
        home: std::env::var_os("HOME").map(PathBuf::from),
        tmpdir: Some(PathBuf::from("/tmp")),
        cwd: Some(f.root.clone()),
        git_enabled: false,
    });
    let c = judge.classify_path(&f.root.join("tracked.txt"), false);
    assert_eq!(c.class, Class::Unknown);
}

// @kotowari[REQ-004]
#[test]
fn req_004_children_of_dirty_worktree_are_ask() {
    let f = fixture();
    std::fs::write(f.root.join("src/a.rs"), "fn a() { changed }\n").unwrap();
    let judge = judge_for(&f.root);
    let c = judge.classify_children(&f.root.join("src"));
    assert_eq!(c.class, Class::Unknown);
    assert_eq!(c.why, Why::Uncommitted);
}

// @kotowari[REQ-004, REQ-008]
#[test]
fn req_004_children_of_ignored_directory_are_vcs() {
    let f = fixture();
    std::fs::create_dir_all(f.root.join("ignored/vite")).unwrap();
    std::fs::write(f.root.join("ignored/vite/x"), "x").unwrap();
    let judge = judge_for(&f.root);
    let c = judge.classify_children(&f.root.join("ignored/vite"));
    assert_eq!(c.class, Class::Vcs);
}
