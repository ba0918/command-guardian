//! judge のテストで共有する git リポジトリのフィクスチャ。

#![allow(dead_code)]
pub fn extract_effects(command: &str, env: &guardian_core::Env) -> Vec<guardian_core::Effect> {
    guardian_analysis::extract_effects(
        guardian_parser::parse(command),
        env,
        &mut guardian_parser::parse,
    )
}

use guardian_core::Env;
use guardian_judge::{Judge, JudgeEnv};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Fixture {
    /// フィクスチャの寿命を保持する。
    pub _dir: tempfile::TempDir,
    pub root: PathBuf,
}

pub fn git_run(root: &Path, args: &[&str]) {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1");
    clear_git_env(&mut command);
    let out = command.output().unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// git がフックに渡す環境（GIT_DIR など）を外す。フィクスチャの git が
/// 呼び出し元のリポジトリではなく、フィクスチャ自身を見るようにする。
pub fn clear_git_env(command: &mut Command) {
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(&name);
        }
    }
}

/// .git と 1 コミットだけを持つフィクスチャ。
pub fn fixture() -> Fixture {
    let dir = tempfile::Builder::new()
        .prefix("command-guardian-git-")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap();
    let root = dir.path().canonicalize().unwrap();
    git_run(&root, &["init", "-q"]);
    std::fs::write(root.join("tracked.txt"), "tracked\n").unwrap();
    std::fs::write(root.join(".gitignore"), "ignored/\nnode_modules/\n").unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
    git_run(&root, &["add", "."]);
    git_run(
        &root,
        &[
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=test",
            "commit",
            "-qm",
            "init",
        ],
    );
    Fixture { _dir: dir, root }
}

pub fn judge_for(root: &Path) -> Judge {
    Judge::new(JudgeEnv {
        home: std::env::var_os("HOME").map(PathBuf::from),
        tmpdir: Some(PathBuf::from("/tmp")),
        cwd: Some(root.to_path_buf()),
        git_enabled: true,
    })
}

pub fn core_env(root: &Path) -> Env {
    Env {
        home: std::env::var_os("HOME").map(PathBuf::from),
        tmpdir: Some(PathBuf::from("/tmp")),
        cwd: Some(root.to_path_buf()),
    }
}
