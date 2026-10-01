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
    assert!(parse_layer(
        "trusted_projects = [false]\n[mode]\nenforce = false",
        Path::new("/work"),
        None
    )
    .is_err());
}

// @kotowari[REQ-015, REQ-034]
#[test]
fn req_034_invalid_guard_rules_do_not_discard_the_valid_general_settings() {
    let layer = parse_layer("[mode]\nenforce = false\n[[commands.guard]]\nprogram = 'git'\nreason = 'invalid'\nverdict = false\ndeny = [['push']]", Path::new("/work"), None).unwrap();
    assert_eq!(layer.enforce, Some(false));
    assert!(layer.guard.is_empty());
    assert_eq!(layer.warnings.len(), 1);
}
