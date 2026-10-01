pub fn invocations(command: &str) -> guardian_policy::guard::InvocationAnalysis {
    guardian_policy::guard::invocations(command, &mut guardian_parser::parse)
}
pub fn parse_guard_rules_document(
    text: &str,
) -> Result<(Vec<guardian_policy::GuardRule>, Vec<String>), toml::de::Error> {
    guardian_policy::guard::parse_guard_rules_document(text, &mut guardian_parser::parse)
}
