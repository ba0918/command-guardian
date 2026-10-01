//! 隔離解析と観測を組み立て、一判定のsessionを所有する。
use crate::{config_loader, runtime::ParserRuntime};
use guardian_core::{Ask, Effect, Env, Failure};
use guardian_judge::{GitRunner, Judge, JudgeEnv, PathObserver, SystemGit, SystemPaths};
use guardian_policy::{Config, Policy};
pub use guardian_policy::{EffectReport, Report, RuleReport};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct EngineEnv {
    pub home: Option<PathBuf>,
    pub tmpdir: Option<PathBuf>,
    pub cwd: PathBuf,
}
pub struct Engine {
    runtime: std::cell::RefCell<ParserRuntime>,
    paths: Box<dyn PathObserver>,
    policy: Policy,
    env: EngineEnv,
    judge: Judge,
}
impl Engine {
    pub fn load(user_config: Option<&Path>, env: EngineEnv, mut runtime: ParserRuntime) -> Self {
        let loaded = config_loader::load(
            user_config,
            &env.cwd,
            env.home.as_deref(),
            env.tmpdir.as_deref(),
            &mut runtime.validation(),
        );
        Self::build(
            loaded.config,
            env,
            loaded.warnings,
            Box::new(SystemGit::default()),
            Box::new(SystemPaths),
            runtime,
        )
    }
    pub fn new(config: Config, env: EngineEnv, runtime: ParserRuntime) -> Self {
        Self::with_git(config, env, Box::new(SystemGit::default()), runtime)
    }
    pub fn with_git(
        config: Config,
        env: EngineEnv,
        git: Box<dyn GitRunner>,
        runtime: ParserRuntime,
    ) -> Self {
        Self::with_observers(config, env, git, Box::new(SystemPaths), runtime)
    }
    pub fn with_observers(
        config: Config,
        env: EngineEnv,
        git: Box<dyn GitRunner>,
        paths: Box<dyn PathObserver>,
        runtime: ParserRuntime,
    ) -> Self {
        Self::build(config, env, Vec::new(), git, paths, runtime)
    }
    fn build(
        config: Config,
        env: EngineEnv,
        warnings: Vec<String>,
        git: Box<dyn GitRunner>,
        paths: Box<dyn PathObserver>,
        runtime: ParserRuntime,
    ) -> Self {
        let judge = Judge::with_git(
            JudgeEnv {
                home: env.home.clone(),
                tmpdir: env.tmpdir.clone(),
                cwd: Some(env.cwd.clone()),
                git_enabled: config.git_enabled,
            },
            git,
        );
        Self {
            runtime: std::cell::RefCell::new(runtime),
            paths,
            policy: Policy::new(config, warnings),
            env,
            judge,
        }
    }
    pub fn config(&self) -> &Config {
        self.policy.config()
    }
    pub fn check(&self, command: &str) -> Report {
        let env = Env {
            home: self.env.home.clone(),
            tmpdir: self.env.tmpdir.clone(),
            cwd: Some(self.env.cwd.clone()),
        };
        let mut runtime = self.runtime.borrow_mut();
        let mut session = runtime.judgment();
        let outcome = session.parse(command);
        let facts = guardian_analysis::analyze(outcome, &env, &mut |input| session.parse(input));
        let mut asks = facts.diagnostics;
        let mut effects = Vec::new();
        for effect in &facts.effects {
            if !self.policy.enabled(effect.op) {
                continue;
            }
            match self.observe_effect(effect, Instant::now() + session.remaining()) {
                Ok(report) => effects.push(report),
                Err(failure) => add_failure(&mut asks, failure),
            }
        }
        let body = if self.policy.needs_body() {
            match session.strip_quotes(command) {
                Ok(body) => Some(body),
                Err(failure) => {
                    add_failure(&mut asks, failure);
                    None
                }
            }
        } else {
            None
        };
        let rules = self.policy.rules(&facts.invocations, body.as_deref());
        let mut report = self.policy.report(effects, rules, asks);
        if session.over_budget() && !report.parse_errors.iter().any(Ask::is_limit) {
            report.parse_errors.push(Ask::Parse(Failure::Limit));
            report = self
                .policy
                .report(report.effects, report.rules, report.parse_errors);
        }
        report
    }
    fn observe_effect(&self, effect: &Effect, deadline: Instant) -> Result<EffectReport, Failure> {
        let (class, why, verdict) = if let Some(value) = self.policy.value_target(&effect.target) {
            value
        } else {
            let observed =
                guardian_judge::observation::observe(&effect.target, self.paths.as_ref())
                    .expect("path target");
            if let Some(value) = self.policy.apply_roots(&observed) {
                value
            } else {
                let classification = self.judge.classify_observed(&observed, deadline)?;
                let verdict = self
                    .policy
                    .classification_verdict(classification.class, &classification.why);
                (classification.class, classification.why, verdict)
            }
        };
        Ok(EffectReport {
            op: effect.op,
            target: effect.target.clone(),
            class,
            why,
            verdict,
        })
    }
}
fn add_failure(asks: &mut Vec<Ask>, failure: Failure) {
    let ask = Ask::Parse(failure);
    if !asks.contains(&ask) {
        asks.push(ask);
    }
}
