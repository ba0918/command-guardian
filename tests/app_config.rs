//! S8: 設定のファイルと層・信頼・壊れたとき（REQ-005, REQ-006, REQ-013, REQ-014, REQ-015）。

use guardian_core::Verdict;
mod app_support;
use app_support::{Engine, EngineEnv};
use guardian_policy::Config;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use guardian_judge::{GitError, GitRunner};

fn env(cwd: &Path) -> EngineEnv {
    EngineEnv {
        home: Some(PathBuf::from("/home/you")),
        tmpdir: Some(PathBuf::from("/tmp")),
        cwd: cwd.to_path_buf(),
    }
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn fixture_dir(prefix: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap()
}

fn engine(user_config: Option<&Path>, cwd: &Path) -> Engine {
    Engine::load(user_config, env(cwd))
}

fn verdict_of(engine: &Engine, command: &str) -> Verdict {
    engine.check(command).verdict
}

// @kotowari[REQ-013, REQ-005, EX-015]
#[test]
fn req_013_project_config_is_found_from_a_parent_of_cwd() {
    let dir = fixture_dir("command-guardian-conf-");
    let root = dir.path().canonicalize().unwrap();
    let child = root.join("child");
    std::fs::create_dir_all(&child).unwrap();
    write(
        &root.join(".command-guardian.toml"),
        "[paths]\nprotected_roots = [\"/home/you/scratch/protected\"]\n",
    );
    let e = engine(None, &child);
    assert_eq!(
        verdict_of(&e, "rm -rf /home/you/scratch/protected/x"),
        Verdict::Block
    );
}

// @kotowari[REQ-013]
#[test]
fn req_013_lists_append_and_scalars_take_the_later_layer() {
    let dir = fixture_dir("command-guardian-conf-");
    let root = dir.path().canonicalize().unwrap();
    let user = root.join("config.toml");
    write(
        &user,
        "[paths]\nprotected_roots = [\"/home/you/a\"]\n[unknown]\nverdict = \"block\"\n",
    );
    // 信頼していないプロジェクトの緩和は無視される。
    write(
        &root.join(".command-guardian.toml"),
        "[paths]\nprotected_roots = [\"/home/you/b\"]\n[unknown]\nverdict = \"ask\"\n",
    );
    let e = engine(Some(&user), &root);
    assert_eq!(verdict_of(&e, "rm -rf /home/you/a/x"), Verdict::Block);
    assert_eq!(verdict_of(&e, "rm -rf /home/you/b/x"), Verdict::Block);
    assert!(e
        .check("rm -rf /home/you/c/x")
        .warnings
        .iter()
        .any(|w| w.contains("unknown.verdict")));
    assert_eq!(verdict_of(&e, "rm -rf /home/you/c/x"), Verdict::Block);
}

// @kotowari[REQ-015, EX-016]
#[test]
fn req_015_broken_user_config_keeps_builtin_defaults_and_warns() {
    let dir = fixture_dir("command-guardian-conf-");
    let root = dir.path().canonicalize().unwrap();
    let user = root.join("config.toml");
    write(&user, "これは TOML ではない [ paths\n");
    let e = engine(Some(&user), &root);
    let r = e.check("rm -rf /tmp/scratch/x");
    assert!(
        r.warnings.iter().any(|w| w.contains("user configuration")),
        "{:?}",
        r.warnings
    );
    assert_eq!(r.verdict, Verdict::Allow);
    assert_eq!(verdict_of(&e, "rm -rf /etc/x"), Verdict::Block);
}

// @kotowari[REQ-013, REQ-015]
#[test]
fn req_015_invalid_general_user_settings_discard_that_file_but_keep_a_valid_project() {
    let dir = fixture_dir("command-guardian-conf-");
    let root = dir.path().canonicalize().unwrap();
    let user = root.join("config.toml");
    write(&user, "paths = []\n[mode]\nenforce = false\n");
    write(
        &root.join(".command-guardian.toml"),
        "[paths]\nprotected_roots = ['/tmp/project-protected']\n",
    );
    let e = engine(Some(&user), &root);
    assert!(e.config().enforce);
    assert_eq!(
        verdict_of(&e, "rm /tmp/project-protected/x"),
        Verdict::Block
    );
    assert!(e
        .check("true")
        .warnings
        .iter()
        .any(|w| w.contains("user configuration")));
}

// @kotowari[REQ-013, REQ-015]
#[test]
fn req_015_invalid_general_project_settings_keep_the_valid_user_layer() {
    let dir = fixture_dir("command-guardian-conf-");
    let root = dir.path().canonicalize().unwrap();
    let user = root.join("config.toml");
    write(
        &user,
        "[paths]\nprotected_roots = ['/tmp/user-protected']\n",
    );
    write(&root.join(".command-guardian.toml"), "rules = []\n[paths]\nprotected_roots = ['/tmp/rejected-project']\n[mode]\nenforce = false\n");
    let e = engine(Some(&user), &root);
    assert!(e.config().enforce);
    assert_eq!(verdict_of(&e, "rm /tmp/user-protected/x"), Verdict::Block);
    assert!(!e
        .config()
        .protected_roots
        .contains(&PathBuf::from("/tmp/rejected-project")));
    assert!(e
        .check("true")
        .warnings
        .iter()
        .any(|w| w.contains("project configuration")));
}

// @kotowari[REQ-014, EX-014]
#[test]
fn req_014_untrusted_project_settings_only_tighten() {
    let dir = fixture_dir("command-guardian-conf-");
    let root = dir.path().canonicalize().unwrap();
    write(
        &root.join(".command-guardian.toml"),
        r#"
[paths]
allowed_roots = ["/"]
protected_roots = ["/home/you/scratch/protected"]
[unknown]
verdict = "block"
[rules]
disable = ["truncate"]
[git]
enabled = false
[mode]
enforce = false
"#,
    );
    let e = engine(None, &root);
    let r = e.check("rm -rf /home/you/scratch/protected/x");
    // 締める変更（保護ルート）は反映される。
    assert_eq!(r.verdict, Verdict::Block);
    // 緩める変更（許可ルートの "/" 追加）は無視され、警告が出る。
    assert!(
        r.warnings.iter().any(|w| w.contains("allowed_roots")),
        "{:?}",
        r.warnings
    );
    assert_eq!(verdict_of(&e, "rm -rf /etc/x"), Verdict::Block);
    // rules.disable も無視され、警告が出る（切り詰めの効果は取り出されたまま）。
    assert!(
        r.warnings.iter().any(|w| w.contains("rules.disable")),
        "{:?}",
        r.warnings
    );
    let truncate = e.check("echo x > /home/you/out");
    assert!(
        truncate
            .effects
            .iter()
            .any(|x| x.op == guardian_core::Op::Truncate),
        "{:?}",
        truncate.effects
    );
    // mode.enforce の無効化も無視され、警告が出る。
    assert!(
        r.warnings.iter().any(|w| w.contains("mode.enforce")),
        "{:?}",
        r.warnings
    );
    assert!(e.config().enforce);
}

// @kotowari[REQ-014, EX-030]
#[test]
fn req_014_project_trusted_projects_cannot_trust_itself() {
    let dir = fixture_dir("command-guardian-conf-");
    let root = dir.path().canonicalize().unwrap();
    write(
        &root.join(".command-guardian.toml"),
        &format!(
            "trusted_projects = [\"{}\"]\n[paths]\nallowed_roots = [\"/\"]\n",
            root.display()
        ),
    );
    let e = engine(None, &root);
    let r = e.check("rm -rf /etc/x");
    assert!(
        r.warnings.iter().any(|w| w.contains("trusted_projects")),
        "{:?}",
        r.warnings
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("allowed_roots")),
        "{:?}",
        r.warnings
    );
    assert_eq!(r.verdict, Verdict::Block);
}

// @kotowari[REQ-014]
#[test]
fn req_014_trusted_project_settings_apply_fully() {
    let dir = fixture_dir("command-guardian-conf-");
    let root = dir.path().canonicalize().unwrap();
    let user = root.join("config.toml");
    write(
        &user,
        &format!("trusted_projects = [\"{}\"]\n", root.display()),
    );
    write(
        &root.join(".command-guardian.toml"),
        "[paths]\nallowed_roots = [\"/home/you/scratch\"]\n[unknown]\nverdict = \"ask\"\n",
    );
    let e = engine(Some(&user), &root);
    assert_eq!(
        verdict_of(&e, "rm -rf /home/you/scratch/build/cache"),
        Verdict::Allow
    );
}

// @kotowari[REQ-005, REQ-006]
#[test]
fn req_005_006_configured_protected_and_allowed_roots_apply() {
    let dir = fixture_dir("command-guardian-conf-");
    let root = dir.path().canonicalize().unwrap();
    let user = root.join("config.toml");
    write(
        &user,
        "[paths]\nprotected_roots = [\"/etc/nginx\"]\nallowed_roots = [\"/home/you/scratch\"]\n",
    );
    let e = engine(Some(&user), &root);
    // 保護ルートとその配下は block。
    assert_eq!(verdict_of(&e, "rm -rf /etc/nginx"), Verdict::Block);
    assert_eq!(verdict_of(&e, "rm -rf /etc/nginx/conf.d"), Verdict::Block);
    // 許可ルートの配下は allow、ルートそれ自体は含まれない。
    assert_eq!(
        verdict_of(&e, "rm -rf /home/you/scratch/build"),
        Verdict::Allow
    );
    assert_eq!(verdict_of(&e, "rm -rf /home/you/scratch"), Verdict::Ask);
    // 既定の許可ルート（一時領域のルート）ではルートそれ自体が block のまま。
    assert_eq!(verdict_of(&e, "rm -rf /tmp"), Verdict::Block);
}

// @kotowari[REQ-006]
#[test]
fn req_006_unknown_verdict_can_be_block() {
    let dir = fixture_dir("command-guardian-conf-");
    let root = dir.path().canonicalize().unwrap();
    let user = root.join("config.toml");
    write(&user, "[unknown]\nverdict = \"block\"\n");
    let e = engine(Some(&user), &root);
    assert_eq!(
        verdict_of(&e, "rm -rf /home/you/work/scratch/x"),
        Verdict::Block
    );
}

struct RecordingGit {
    calls: Arc<Mutex<usize>>,
}

impl GitRunner for RecordingGit {
    fn status(&self, _root: &Path, _path: &Path) -> Result<String, GitError> {
        *self.calls.lock().unwrap() += 1;
        Ok(String::new())
    }
}

// @kotowari[REQ-006]
#[test]
fn req_006_git_enabled_false_does_not_invoke_git_or_classify_vcs() {
    let calls = Arc::new(Mutex::new(0usize));
    let mut config = Config::builtin(Some(Path::new("/tmp")));
    config.git_enabled = false;
    let e = Engine::with_git(
        config,
        env(Path::new("/home/you/work/repo")),
        Box::new(RecordingGit {
            calls: calls.clone(),
        }),
    );
    let r = e.check("rm -rf worktree-file");
    assert_eq!(r.verdict, Verdict::Ask);
    assert_eq!(*calls.lock().unwrap(), 0);
}

// @kotowari[REQ-013]
#[test]
fn req_013_user_config_relative_paths_resolve_against_its_directory() {
    let dir = fixture_dir("command-guardian-conf-");
    let root = dir.path().canonicalize().unwrap();
    let user = root.join("command-guardian/config.toml");
    write(&user, "[paths]\nprotected_roots = [\"prot\"]\n");
    let e = Engine::load(Some(&user), env(&root));
    let expected = root.join("command-guardian/prot");
    assert!(e.config().protected_roots.contains(&expected));
}

// @kotowari[REQ-015]
#[test]
fn req_015_broken_project_config_is_ignored_with_a_warning() {
    let dir = fixture_dir("command-guardian-conf-");
    let root = dir.path().canonicalize().unwrap();
    write(&root.join(".command-guardian.toml"), "= broken");
    let e = engine(None, &root);
    let r = e.check("rm -rf /home/you/x");
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains("project configuration")),
        "{:?}",
        r.warnings
    );
    assert_eq!(r.verdict, Verdict::Ask);
}

/// フックの環境（GIT_DIR など）を引き継がずに git を起動する。
fn git(root: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(root).env("GIT_CONFIG_NOSYSTEM", "1");
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(&name);
        }
    }
    cmd
}

fn git_repo() -> tempfile::TempDir {
    let dir = fixture_dir("command-guardian-policy-git-");
    let root = dir.path();
    std::fs::write(root.join("tracked.txt"), "x").unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "."],
        vec![
            "-c",
            "user.email=t@example.com",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "init",
        ],
    ] {
        let out = git(root).args(&args).output().unwrap();
        assert!(out.status.success(), "git {args:?}");
    }
    dir
}

// @kotowari[REQ-006]
#[test]
fn req_006_user_git_enabled_false_makes_worktree_paths_unknown() {
    let repo = git_repo();
    let root = repo.path().canonicalize().unwrap();
    let user = root.join("user-config.toml");
    write(&user, "[git]\nenabled = false\n");
    // フィクスチャは実ユーザのホームの下にあるため、home を実環境に合わせる。
    let e = Engine::load(
        Some(&user),
        EngineEnv {
            home: std::env::var_os("HOME").map(PathBuf::from),
            tmpdir: Some(PathBuf::from("/tmp")),
            cwd: root.clone(),
        },
    );
    assert_eq!(verdict_of(&e, "rm -rf tracked.txt"), Verdict::Ask);
}

// @kotowari[REQ-027]
#[test]
fn req_027_untrusted_project_guard_rule_warning_is_kept() {
    // reason の無い規則は形の誤り。信頼していないプロジェクト設定でも
    // 無効にした警告を落とさない。
    let dir = fixture_dir("command-guardian-conf-");
    let root = dir.path().canonicalize().unwrap();
    write(
        &root.join(".command-guardian.toml"),
        "[[commands.guard]]\nprogram = \"git\"\ndeny = [[\"push\"]]\n",
    );
    let e = engine(None, &root);
    let r = e.check("git push origin main");
    assert!(
        r.warnings.iter().any(|w| w.contains("command guard")),
        "{:?}",
        r.warnings
    );
}
