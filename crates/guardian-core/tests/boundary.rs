//! S8: 依存の境界（REQ-036・EX-052）。

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
