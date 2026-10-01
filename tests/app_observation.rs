use guardian_app::{Engine, EngineEnv};
use guardian_core::{Class, Target, Verdict};
use guardian_judge::{PathObserver, SystemGit};
use guardian_policy::Config;
use std::path::{Path, PathBuf};

struct Retarget;
impl PathObserver for Retarget {
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        let observed = std::fs::canonicalize(path)?;
        std::fs::remove_file(path)?;
        std::os::unix::fs::symlink("/etc", path)?;
        Ok(observed)
    }
}
fn runtime() -> guardian_app::runtime::ParserRuntime {
    guardian_app::runtime::ParserRuntime::new(env!("CARGO_BIN_EXE_command-guardian").into())
}

// @kotowari[REQ-005, REQ-007, REQ-010]
#[test]
fn req_007_roots_and_default_classification_share_the_observed_link_target() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let root = dir.path().canonicalize().unwrap();
    let actual = root.join("actual");
    std::fs::create_dir(&actual).unwrap();
    let link = root.join("link");
    std::os::unix::fs::symlink(&actual, &link).unwrap();
    let mut config = Config::builtin(None);
    config.protected_roots.push("/etc".into());
    let env = EngineEnv {
        home: Some(root.clone()),
        tmpdir: None,
        cwd: root.clone(),
    };
    let engine = Engine::with_observers(
        config,
        env,
        Box::new(SystemGit::new("/nonexistent/git".into())),
        Box::new(Retarget),
        runtime(),
    );
    let report = engine.check("rm link/");
    assert_eq!(std::fs::read_link(&link).unwrap(), PathBuf::from("/etc"));
    assert_eq!(report.verdict, Verdict::Ask);
    assert_eq!(report.effects[0].class, Class::Unknown);
    assert_eq!(
        report.effects[0].target,
        Target::Path {
            path: link,
            dereference: true
        }
    );
}

// @kotowari[REQ-005, REQ-007, REQ-008]
#[test]
fn req_007_missing_paths_and_non_dereferenced_paths_keep_lexical_roots() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let root = dir.path().canonicalize().unwrap();
    let safe = root.join("safe");
    std::fs::create_dir(&safe).unwrap();
    let link = safe.join("link");
    std::os::unix::fs::symlink("/etc", &link).unwrap();
    let mut config = Config::builtin(None);
    config.allowed_roots.push(safe.clone());
    config.git_enabled = false;
    config.protected_roots.push(safe.join("protected"));
    let engine = Engine::new(
        config,
        EngineEnv {
            home: Some(root.clone()),
            tmpdir: None,
            cwd: root,
        },
        runtime(),
    );
    assert_eq!(engine.check("rm safe/link").verdict, Verdict::Allow);
    assert_eq!(engine.check("rm safe/link/").verdict, Verdict::Block);
    assert_eq!(engine.check("rm safe/missing/").verdict, Verdict::Allow);
    assert_eq!(engine.check("rm safe/protected/x").verdict, Verdict::Block);
    assert_eq!(engine.check("find safe/ -delete").verdict, Verdict::Allow);
    assert_eq!(engine.check("rm safe/*").verdict, Verdict::Ask);
}
