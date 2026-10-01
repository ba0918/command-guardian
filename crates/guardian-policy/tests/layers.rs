use guardian_policy::layers::parse_layer;
use std::path::Path;

// @kotowari[REQ-015]
#[test]
fn req_015_invalid_section_types_warn_without_discarding_valid_sections() {
    for section in ["paths", "unknown", "rules", "git", "commands"] {
        let layer = parse_layer(
            &format!("{section} = []\n[mode]\nenforce = false"),
            Path::new("/work"),
            None,
        )
        .unwrap();
        assert!(!layer.warnings.is_empty(), "{section}");
        assert_eq!(layer.enforce, Some(false));
    }
    let layer = parse_layer(
        "mode = 'invalid'\n[git]\nenabled = false",
        Path::new("/work"),
        None,
    )
    .unwrap();
    assert!(!layer.warnings.is_empty());
    assert_eq!(layer.git_enabled, Some(false));
    let layer = parse_layer("[mode]\nenforce = false", Path::new("/work"), None).unwrap();
    assert!(layer.warnings.is_empty());
}
