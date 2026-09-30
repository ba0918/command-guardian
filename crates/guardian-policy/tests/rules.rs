//! S9: ルールの意味論（REQ-026）。

use guardian_core::{Op, Verdict};
use guardian_policy::{Engine, EngineEnv};
use std::path::{Path, PathBuf};

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

// @kotowari[REQ-026, EX-035]
#[test]
fn req_026_disabled_effect_is_not_extracted() {
    let dir = fixture_dir("hook-guardian-rules-");
    let root = dir.path().canonicalize().unwrap();
    let user = root.join("config.toml");
    write(&user, "[rules]\ndisable = [\"truncate\"]\n");
    let e = Engine::load(Some(&user), env(&root));
    let r = e.check("echo x > /etc/foo");
    assert!(
        !r.effects.iter().any(|x| x.op == Op::Truncate),
        "{:?}",
        r.effects
    );
    assert_eq!(r.verdict, Verdict::Allow);
}

// @kotowari[REQ-026]
#[test]
fn req_026_disabled_delete_effect_is_not_extracted() {
    let dir = fixture_dir("hook-guardian-rules-");
    let root = dir.path().canonicalize().unwrap();
    let user = root.join("config.toml");
    write(&user, "[rules]\ndisable = [\"delete\"]\n");
    let e = Engine::load(Some(&user), env(&root));
    assert_eq!(e.check("rm -rf /etc/nginx").verdict, Verdict::Allow);
    assert_eq!(e.check("find /usr/lib -delete").verdict, Verdict::Allow);
}

// @kotowari[REQ-026, EX-036]
#[test]
fn req_026_custom_rule_verdict_joins_the_composition() {
    let dir = fixture_dir("hook-guardian-rules-");
    let root = dir.path().canonicalize().unwrap();
    let user = root.join("config.toml");
    write(
        &user,
        r#"
[[rules.custom]]
name = "push"
pattern = "git\\s+push"
verdict = "ask"
"#,
    );
    let e = Engine::load(Some(&user), env(&root));
    let r = e.check("git push origin main");
    assert_eq!(r.verdict, Verdict::Ask);
    assert_eq!(r.rules.len(), 1);
    assert_eq!(r.rules[0].name, "push");
    assert_eq!(r.rules[0].verdict, Verdict::Ask);
    // 引用を外した本文に当たる。
    let r = e.check("git \"push\" origin main");
    assert_eq!(r.verdict, Verdict::Ask);
    // 当たらないコマンドは変わらない。
    assert_eq!(e.check("git status").verdict, Verdict::Allow);
}

// @kotowari[REQ-026]
#[test]
fn req_026_allow_custom_rule_is_only_effective_from_user_config() {
    let dir = fixture_dir("hook-guardian-rules-");
    let root = dir.path().canonicalize().unwrap();
    let user = root.join("config.toml");
    write(
        &user,
        r#"
[[rules.custom]]
name = "ok"
pattern = "^git status$"
verdict = "allow"
"#,
    );
    let e = Engine::load(Some(&user), env(&root));
    let r = e.check("git status");
    assert_eq!(r.verdict, Verdict::Allow);
    assert_eq!(r.rules.len(), 1);
    assert_eq!(r.rules[0].verdict, Verdict::Allow);

    // 信頼したプロジェクト設定でも allow のルールは利用者設定に限る。
    let project = root.join("proj");
    std::fs::create_dir_all(&project).unwrap();
    write(
        &root.join(".hook-guardian.toml"),
        r#"
[[rules.custom]]
name = "proj-allow"
pattern = ".*"
verdict = "allow"
"#,
    );
    let user2 = root.join("config2.toml");
    write(
        &user2,
        &format!(
            "trusted_projects = [\"{}\"]\n[[rules.custom]]\nname = \"ok\"\npattern = \"^git status$\"\nverdict = \"allow\"\n",
            root.display()
        ),
    );
    let e = Engine::load(Some(&user2), env(&root));
    let r = e.check("git status");
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains("利用者設定でのみ有効")),
        "{:?}",
        r.warnings
    );
    assert!(
        !r.rules.iter().any(|x| x.name == "proj-allow"),
        "{:?}",
        r.rules
    );
}

// @kotowari[REQ-026]
#[test]
fn req_026_custom_rule_from_untrusted_project_tightens() {
    let dir = fixture_dir("hook-guardian-rules-");
    let root = dir.path().canonicalize().unwrap();
    write(
        &root.join(".hook-guardian.toml"),
        r#"
[[rules.custom]]
name = "no-push"
pattern = "git\\s+push"
verdict = "block"
"#,
    );
    let e = Engine::load(None, env(&root));
    let r = e.check("git push origin main");
    assert_eq!(r.verdict, Verdict::Block);
    assert_eq!(r.rules[0].name, "no-push");
}

// @kotowari[REQ-026]
#[test]
fn req_026_broken_custom_pattern_is_ignored_with_a_warning() {
    let dir = fixture_dir("hook-guardian-rules-");
    let root = dir.path().canonicalize().unwrap();
    let user = root.join("config.toml");
    write(
        &user,
        r#"
[[rules.custom]]
name = "broken"
pattern = "("
verdict = "block"
"#,
    );
    let e = Engine::load(Some(&user), env(&root));
    let r = e.check("anything");
    assert!(
        r.warnings.iter().any(|w| w.contains("正規表現")),
        "{:?}",
        r.warnings
    );
    assert_eq!(r.verdict, Verdict::Allow);
}
