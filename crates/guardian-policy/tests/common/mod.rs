pub fn facts(command: &str) -> guardian_core::CommandFacts {
    guardian_analysis::analyze(
        guardian_parser::parse(command),
        &guardian_core::Env {
            home: None,
            tmpdir: None,
            cwd: None,
        },
        &mut guardian_parser::parse,
    )
}
pub fn invocations(command: &str) -> Vec<guardian_core::Invocation> {
    facts(command).invocations
}
pub fn parse_guard_rules_document(
    text: &str,
) -> Result<(Vec<guardian_policy::GuardRule>, Vec<String>), toml::de::Error> {
    let value: toml::Value = toml::from_str(text)?;
    let mut warnings = Vec::new();
    let rules = guardian_policy::guard::parse_guards(&value, &mut warnings);
    Ok((rules, warnings))
}
