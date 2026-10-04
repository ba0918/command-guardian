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

// @kotowari[REQ-advisor-015, REQ-advisor-016, EX-advisor-029, EX-advisor-031]
#[test]
fn user_advice_defaults_and_explicit_values_are_adopted() {
    let dir = fixture_dir("advisor-config-");
    let root = dir.path();
    let defaults = engine(None, root);
    let a = &defaults.config().advisor;
    assert_eq!(a.mode.as_str(), "off");
    assert_eq!(a.model, "jev-latest");
    assert_eq!(a.timeout_ms, 2000);
    assert_eq!(a.max_request_bytes, 65536);
    assert_eq!(a.intervention_threshold, 0.9);
    assert_eq!(a.context_exchanges, 3);
    assert_eq!(a.context_ttl_hours, 24);
    assert!(!a.debug_text);
    let user = root.join("user.toml");
    write(
        &user,
        "[advisor]\nmode='observe'\nmodel='fixture-model'\ntimeout_ms=10000\ncontext_exchanges=0\n",
    );
    let e = engine(Some(&user), root);
    assert_eq!(e.config().advisor.mode.as_str(), "observe");
    assert_eq!(e.config().advisor.model, "fixture-model");
    assert_eq!(e.config().advisor.timeout_ms, 10000);
    assert_eq!(e.config().advisor.context_exchanges, 0);
    write(
        &user,
        "[advisor]\nmode='observe'\ntimeout_ms=18446744073709\n",
    );
    let e = engine(Some(&user), root);
    assert_eq!(e.config().advisor.timeout_ms, 18446744073709);
    assert_eq!(e.config().advisor.mode.as_str(), "observe");
    assert!(e.check("true").warnings.is_empty());
}

// @kotowari[REQ-advisor-015, EX-advisor-030]
#[test]
fn project_advice_is_discarded_before_validation_without_losing_protection() {
    for trusted in [false, true] {
        for advisor in [
            "advisor=42",
            "[advisor]\ntimeout_ms=0",
            "[advisor]\nmode=[]\nunknown_key='fixture'",
        ] {
            let dir = fixture_dir("advisor-project-");
            let root = dir.path();
            let user = root.join("user.toml");
            let trust = if trusted {
                format!("trusted_projects=[{:?}]\n", root.to_str().unwrap())
            } else {
                String::new()
            };
            write(&user, &format!("{trust}[advisor]\nmode='observe'\n"));
            write(
                &root.join(".command-guardian.toml"),
                &format!(
                    "{advisor}\n[paths]\nprotected_roots=['/tmp/advisor-fixture-protected']\n"
                ),
            );
            let e = engine(Some(&user), root);
            assert_eq!(e.config().advisor.mode.as_str(), "observe");
            assert_eq!(
                verdict_of(&e, "rm /tmp/advisor-fixture-protected/x"),
                Verdict::Block
            );
            assert!(!e.check("true").warnings.is_empty());
        }
    }
}

// @kotowari[REQ-advisor-015, REQ-advisor-016, EX-advisor-032, EX-advisor-044]
#[test]
fn invalid_user_advice_discards_the_whole_file_not_just_the_advice() {
    for invalid in [
        "timeout_ms=0",
        "timeout_ms=-1",
        "timeout_ms=18446744073710",
        "timeout_ms=18446744074710",
        "context_exchanges=-1",
        "context_ttl_hours=9223372036854775807",
        "max_request_bytes=4294967295",
        "intervention_threshold=1.1",
        "intervention_threshold=nan",
        "intervention_threshold=0.0",
        "mode='invalid'",
        "model=''",
        "debug_text=1",
        "url='https://fixture.invalid'",
        "model='fixture\u{7f}'",
    ] {
        let dir = fixture_dir("advisor-invalid-");
        let root = dir.path();
        let user = root.join("user.toml");
        write(
            &user,
            &format!(
                "[mode]\nenforce=false\n[paths]\nprotected_roots=['/tmp/rejected-user-fixture']\n[advisor]\n{invalid}\n"
            ),
        );
        write(
            &root.join(".command-guardian.toml"),
            "[paths]\nprotected_roots=['/tmp/valid-project-layer']\n",
        );
        let e = engine(Some(&user), root);
        assert!(e.config().enforce, "{invalid}");
        assert!(
            !e.config()
                .protected_roots
                .contains(&PathBuf::from("/tmp/rejected-user-fixture")),
            "{invalid}"
        );
        assert_eq!(e.config().advisor.mode.as_str(), "off");
        assert_eq!(
            verdict_of(&e, "rm /tmp/valid-project-layer/x"),
            Verdict::Block
        );
        assert!(!e.check("true").warnings.is_empty());
    }
}

// @kotowari[REQ-advisor-015, EX-advisor-043]
#[test]
fn project_advice_does_not_hide_invalid_toml_syntax() {
    let dir = fixture_dir("advisor-syntax-");
    let root = dir.path();
    write(
        &root.join(".command-guardian.toml"),
        "[paths]\nprotected_roots=['/tmp/rejected-project-fixture']\n[advisor]\nmodel='unclosed\n",
    );
    let e = engine(None, root);
    assert!(
        !e.config()
            .protected_roots
            .contains(&PathBuf::from("/tmp/rejected-project-fixture"))
    );
    assert!(!e.check("true").warnings.is_empty());
}

// @kotowari[REQ-015, EX-112]
#[test]
fn invalid_command_container_discards_the_file_but_preserves_other_layers() {
    for commands in ["commands=[]", "[commands]\nguard=42"] {
        let dir = fixture_dir("command-container-");
        let root = dir.path();
        let user = root.join("user.toml");
        write(
            &user,
            &format!(
                "{commands}\n[mode]\nenforce=false\n[paths]\nprotected_roots=['/tmp/rejected-container']\n"
            ),
        );
        write(
            &root.join(".command-guardian.toml"),
            "[paths]\nprotected_roots=['/tmp/valid-project-layer']\n",
        );
        let e = engine(Some(&user), root);
        assert!(e.config().enforce);
        assert!(
            !e.config()
                .protected_roots
                .contains(&PathBuf::from("/tmp/rejected-container"))
        );
        assert_eq!(
            verdict_of(&e, "rm /tmp/valid-project-layer/x"),
            Verdict::Block
        );
        assert!(!e.check("true").warnings.is_empty());
    }
}

// @kotowari[REQ-034, REQ-015, EX-113]
#[test]
fn invalid_individual_guard_keeps_the_files_other_settings_and_valid_guard() {
    let dir = fixture_dir("individual-guard-");
    let root = dir.path();
    let user = root.join("user.toml");
    write(
        &user,
        "[paths]\nprotected_roots=['/tmp/valid-user-layer']\n[[commands.guard]]\nprogram=42\n[[commands.guard]]\nprogram='fixture-command'\nreason='fixture guard'\nverdict='block'\ndeny=[['dangerous']]\n",
    );
    let e = engine(Some(&user), root);
    assert_eq!(verdict_of(&e, "rm /tmp/valid-user-layer/x"), Verdict::Block);
    assert_eq!(verdict_of(&e, "fixture-command dangerous"), Verdict::Block);
    assert_eq!(e.config().guard.len(), 1);
    assert!(!e.check("true").warnings.is_empty());
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
    assert!(
        e.check("rm -rf /home/you/c/x")
            .warnings
            .iter()
            .any(|w| w.contains("unknown.verdict"))
    );
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
    assert!(
        e.check("true")
            .warnings
            .iter()
            .any(|w| w.contains("user configuration"))
    );
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
    write(
        &root.join(".command-guardian.toml"),
        "rules = []\n[paths]\nprotected_roots = ['/tmp/rejected-project']\n[mode]\nenforce = false\n",
    );
    let e = engine(Some(&user), &root);
    assert!(e.config().enforce);
    assert_eq!(verdict_of(&e, "rm /tmp/user-protected/x"), Verdict::Block);
    assert!(
        !e.config()
            .protected_roots
            .contains(&PathBuf::from("/tmp/rejected-project"))
    );
    assert!(
        e.check("true")
            .warnings
            .iter()
            .any(|w| w.contains("project configuration"))
    );
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

// @kotowari[REQ-059, REQ-014]
#[test]
fn req_059_project_defer_ask_is_ignored_with_a_warning_even_when_trusted() {
    for trusted in [false, true] {
        let dir = fixture_dir("defer-ask-project-");
        let root = dir.path().canonicalize().unwrap();
        let user = root.join("user.toml");
        let trust = if trusted {
            format!("trusted_projects=[{:?}]\n", root.to_str().unwrap())
        } else {
            String::new()
        };
        write(&user, &trust);
        write(
            &root.join(".command-guardian.toml"),
            "[mode]\ndefer_ask = true\n",
        );
        let e = engine(Some(&user), &root);
        assert!(!e.config().defer_ask, "trusted: {trusted}");
        assert!(
            e.check("true")
                .warnings
                .iter()
                .any(|w| w.contains("mode.defer_ask")),
            "trusted: {trusted}"
        );
    }
}

// @kotowari[REQ-059, REQ-015, EX-116]
#[test]
fn ex_116_invalid_project_defer_ask_keeps_the_protected_root() {
    let dir = fixture_dir("defer-ask-project-invalid-");
    let root = dir.path();
    write(
        &root.join(".command-guardian.toml"),
        "[mode]\ndefer_ask = 'yes'\n[paths]\nprotected_roots = ['/tmp/defer-ask-fixture-protected']\n",
    );
    let e = engine(None, root);
    assert!(!e.config().defer_ask);
    assert_eq!(
        verdict_of(&e, "rm /tmp/defer-ask-fixture-protected/x"),
        Verdict::Block
    );
    assert!(
        e.check("true")
            .warnings
            .iter()
            .any(|w| w.contains("mode.defer_ask"))
    );
}

// @kotowari[REQ-059, REQ-015]
#[test]
fn req_059_invalid_user_defer_ask_discards_the_file_and_stays_off() {
    let dir = fixture_dir("defer-ask-user-invalid-");
    let root = dir.path();
    let user = root.join("user.toml");
    write(&user, "[mode]\ndefer_ask = 'yes'\n[git]\nenabled = false\n");
    let e = engine(Some(&user), root);
    assert!(!e.config().defer_ask);
    assert!(e.config().git_enabled);
    assert!(
        e.check("true")
            .warnings
            .iter()
            .any(|w| w.contains("user configuration"))
    );
}
