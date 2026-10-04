use guardian_policy::layers::parse_layer;
use std::path::Path;

// @kotowari[REQ-015]
#[test]
fn req_015_invalid_section_types_reject_the_whole_file() {
    for section in ["paths", "unknown", "rules", "git", "commands"] {
        let result = parse_layer(
            &format!("{section} = []\n[mode]\nenforce = false"),
            Path::new("/work"),
            None,
        );
        assert!(result.is_err(), "{section}: {result:?}");
    }
    let result = parse_layer(
        "mode = 'invalid'\n[git]\nenabled = false",
        Path::new("/work"),
        None,
    );
    assert!(result.is_err(), "{result:?}");
    let layer = parse_layer("[mode]\nenforce = false", Path::new("/work"), None).unwrap();
    assert!(layer.warnings.is_empty());
}

// @kotowari[REQ-015]
#[test]
fn req_015_invalid_general_values_reject_the_whole_file() {
    for invalid in [
        "[paths]\nallowed_roots = false",
        "[paths]\nprotected_roots = [\"/etc/valid\", false]",
        "[unknown]\nverdict = 'allow'",
        "[unknown]\nverdict = false",
        "[git]\nenabled = 'false'",
        "[rules]\ndisable = false",
        "[rules]\ndisable = ['delete', false]",
        "[rules]\ndisable = ['typo']",
        "[rules]\ncustom = false",
        "[rules]\ncustom = [{ name = 'bad', pattern = '[', verdict = 'block' }]",
        "[rules]\ncustom = [{ name = 'bad', pattern = 'x', verdict = false }]",
        "[commands]\nguard = false",
    ] {
        let result = parse_layer(
            &format!("[mode]\nenforce = false\n{invalid}"),
            Path::new("/work"),
            None,
        );
        assert!(result.is_err(), "{invalid}: {result:?}");
    }
    assert!(
        parse_layer(
            "trusted_projects = [false]\n[mode]\nenforce = false",
            Path::new("/work"),
            None
        )
        .is_err()
    );
}

// @kotowari[REQ-015, REQ-034]
#[test]
fn req_034_invalid_guard_rules_do_not_discard_the_valid_general_settings() {
    let layer = parse_layer("[mode]\nenforce = false\n[[commands.guard]]\nprogram = 'git'\nreason = 'invalid'\nverdict = false\ndeny = [['push']]", Path::new("/work"), None).unwrap();
    assert_eq!(layer.enforce, Some(false));
    assert!(layer.guard.is_empty());
    assert_eq!(layer.warnings.len(), 1);
}

// @kotowari[REQ-059]
#[test]
fn req_059_user_defer_ask_true_is_adopted() {
    let layer = parse_layer("[mode]\ndefer_ask = true", Path::new("/work"), None).unwrap();
    assert_eq!(layer.defer_ask, Some(true));
    let mut config = guardian_policy::Config::builtin(None);
    guardian_policy::layers::merge(&mut config, &layer);
    assert!(config.defer_ask);
}

// @kotowari[REQ-059]
#[test]
fn req_059_defer_ask_is_off_by_default() {
    assert!(!guardian_policy::Config::builtin(None).defer_ask);
    let layer = parse_layer("[mode]\nenforce = true", Path::new("/work"), None).unwrap();
    assert_eq!(layer.defer_ask, None);
}

// @kotowari[REQ-059, REQ-015]
#[test]
fn req_059_invalid_user_defer_ask_rejects_the_whole_file() {
    for invalid in ["'yes'", "1", "[]"] {
        let result = parse_layer(
            &format!("[mode]\ndefer_ask = {invalid}\n[git]\nenabled = false"),
            Path::new("/work"),
            None,
        );
        assert!(result.is_err(), "{invalid}: {result:?}");
    }
}

// @kotowari[REQ-059]
#[test]
fn req_059_project_defer_ask_is_removed_with_a_warning_before_validation() {
    for value in ["true", "'yes'"] {
        let layer = guardian_policy::layers::parse_project_layer(
            &format!("[mode]\ndefer_ask = {value}\n[paths]\nprotected_roots = ['/work/keep']"),
            Path::new("/work"),
            None,
        )
        .unwrap();
        assert_eq!(layer.defer_ask, None, "{value}");
        assert_eq!(layer.protected_roots, vec![Path::new("/work/keep")]);
        assert!(
            layer.warnings.iter().any(|w| w.contains("mode.defer_ask")),
            "{value}: {:?}",
            layer.warnings
        );
    }
}
