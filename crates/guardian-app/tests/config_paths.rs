use guardian_app::config_loader::find_project_config;

// @kotowari[REQ-015]
#[test]
fn req_015_user_config_metadata_errors_warn_but_missing_files_do_not() {
    let fixture = tempfile::tempdir().unwrap();
    let config = fixture.path().join("loop.toml");
    std::os::unix::fs::symlink("loop.toml", &config).unwrap();
    let mut runtime = guardian_app::runtime::ParserRuntime::new(std::path::PathBuf::new());
    let mut session = runtime.validation();
    let loaded =
        guardian_app::config_loader::load(Some(&config), fixture.path(), None, None, &mut session);
    assert_eq!(loaded.warnings.len(), 1);
    assert!(loaded.warnings[0].contains("Cannot read user configuration"));
    assert_eq!(loaded.config, guardian_policy::Config::builtin(None));
    let loaded = guardian_app::config_loader::load(
        Some(&fixture.path().join("missing.toml")),
        fixture.path(),
        None,
        None,
        &mut session,
    );
    assert!(loaded.warnings.is_empty());
}

// @kotowari[REQ-013]
#[test]
fn req_013_relative_cwd_finds_configuration_above_its_lexical_ancestors() {
    if let Some(expected) = std::env::var_os("GUARDIAN_TEST_PARENT_CONFIG") {
        assert_eq!(
            find_project_config(std::path::Path::new(".")),
            Some(expected.into())
        );
        return;
    }
    let fixture = tempfile::tempdir().unwrap();
    let child = fixture.path().join("child");
    std::fs::create_dir(&child).unwrap();
    let config = fixture.path().join(".command-guardian.toml");
    std::fs::write(&config, "[mode]\nenforce = true").unwrap();
    assert_eq!(find_project_config(&child), Some(config.clone()));
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "req_013_relative_cwd_finds_configuration_above_its_lexical_ancestors",
            "--nocapture",
        ])
        .current_dir(&child)
        .env("GUARDIAN_TEST_PARENT_CONFIG", &config)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
