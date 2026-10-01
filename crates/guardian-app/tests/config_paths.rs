use guardian_app::config_loader::find_project_config;

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
