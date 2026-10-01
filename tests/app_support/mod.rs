#![allow(dead_code)]
pub use guardian_app::EngineEnv;
pub struct Engine(guardian_app::Engine);
fn runtime() -> guardian_app::runtime::ParserRuntime {
    guardian_app::runtime::ParserRuntime::new(env!("CARGO_BIN_EXE_command-guardian").into())
}
impl std::ops::Deref for Engine {
    type Target = guardian_app::Engine;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Engine {
    pub fn load(path: Option<&std::path::Path>, env: EngineEnv) -> Self {
        Self(guardian_app::Engine::load(path, env, runtime()))
    }
    pub fn new(config: guardian_policy::Config, env: EngineEnv) -> Self {
        Self(guardian_app::Engine::new(config, env, runtime()))
    }
    pub fn with_git(
        config: guardian_policy::Config,
        env: EngineEnv,
        git: Box<dyn guardian_judge::GitRunner>,
    ) -> Self {
        Self(guardian_app::Engine::with_git(config, env, git, runtime()))
    }
}
pub fn invocations(command: &str) -> Vec<guardian_core::Invocation> {
    guardian_analysis::analyze(
        guardian_parser::parse(command),
        &guardian_core::Env {
            home: None,
            tmpdir: None,
            cwd: None,
        },
        &mut guardian_parser::parse,
    )
    .invocations
}
pub fn parse_guard_rules_document(
    text: &str,
) -> Result<(Vec<guardian_policy::GuardRule>, Vec<String>), toml::de::Error> {
    let value: toml::Value = toml::from_str(text)?;
    let mut warnings = Vec::new();
    let mut rules = guardian_policy::guard::parse_guards(&value, &mut warnings);
    guardian_app::config_loader::validate_examples(
        &mut rules,
        &mut warnings,
        &mut runtime().validation(),
    );
    Ok((rules, warnings))
}
