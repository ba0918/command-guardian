#![allow(dead_code)]
use guardian_core::{Effect, Env};
pub fn analyze(command: &str, env: &Env) -> guardian_analysis::Analysis {
    guardian_analysis::analyze(
        guardian_parser::parse(command),
        env,
        &mut guardian_parser::parse,
    )
}
pub fn extract_effects(command: &str, env: &Env) -> Vec<Effect> {
    analyze(command, env).effects
}
