//! S8: 依存の境界（REQ-036・EX-052）。

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

// @kotowari[EX-052]
#[test]
fn ex_052_core_and_policy_do_not_depend_on_the_oss_parser() {
    // 直接依存に OSS のシェルパーサは現れない。依存は guardian-parser の中だけ。
    for directory in ["crates/guardian-core", "crates/guardian-policy"] {
        let manifest = workspace_root().join(directory).join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest).unwrap();
        assert!(
            !text.contains("brush"),
            "{} が OSS のパーサに依存している",
            manifest.display()
        );
    }
    // 版は workspace で固定する。
    let root = std::fs::read_to_string(workspace_root().join("Cargo.toml")).unwrap();
    assert!(root.contains("brush-parser = \"=0.4.0\""), "{root}");
}

// @kotowari[EX-052]
#[test]
fn ex_052_core_exposes_only_normalized_types() {
    // core の公開 API は guardian-parser の公開型だけを包む。
    let analysis = guardian_core::analyze("rm -rf /etc/x", &guardian_core::Env::default());
    assert_eq!(analysis.effects.len(), 1);
    assert!(analysis.parse_errors.is_empty());
    let outcome = guardian_parser::parse("rm -rf /etc/x");
    assert!(outcome.failures.is_empty());
    assert_eq!(outcome.script.items.len(), 1);
}
