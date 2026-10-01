//! 判定の入口。設定を読み、効果ごとに分類と判定を出し、合成する。

use crate::config_loader as config;
use guardian_analysis::analyze;
use guardian_core::{Ask, Class, Env, Op, ProtectedKind, Target, Verdict, Why};
use guardian_judge::{GitRunner, Judge, JudgeEnv};
use guardian_policy::{config::Config, message};
use regex::Regex;
use std::path::{Path, PathBuf};

/// 判定に渡す環境。
#[derive(Debug, Clone)]
pub struct EngineEnv {
    pub home: Option<PathBuf>,
    pub tmpdir: Option<PathBuf>,
    pub cwd: PathBuf,
}

/// 効果ごとの判定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectReport {
    pub op: Op,
    pub target: Target,
    pub class: Class,
    pub why: Why,
    pub verdict: Verdict,
}

/// 一致したルール。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleReport {
    pub name: String,
    pub reason: String,
    pub verdict: Verdict,
}

/// 1 つのコマンドの判定。
#[derive(Debug, Clone)]
pub struct Report {
    pub verdict: Verdict,
    pub effects: Vec<EffectReport>,
    pub rules: Vec<RuleReport>,
    pub warnings: Vec<String>,
    pub message: String,
    /// 判定の理由。効果が無い ask でも残る（REQ-017・A13）。
    pub reason: String,
    /// 判定できない理由（REQ-038）。
    pub parse_errors: Vec<Ask>,
}

struct CompiledRule {
    name: String,
    regex: Regex,
    verdict: Verdict,
}

pub struct Engine {
    runtime: std::cell::RefCell<crate::runtime::ParserRuntime>,
    paths: Box<dyn guardian_judge::PathObserver>,
    config: Config,
    env: EngineEnv,
    judge: Judge,
    warnings: Vec<String>,
    custom_rules: Vec<CompiledRule>,
}

impl Engine {
    /// 設定を読み込んで作る。
    pub fn load(
        user_config: Option<&Path>,
        env: EngineEnv,
        mut runtime: crate::runtime::ParserRuntime,
    ) -> Engine {
        let loaded = config::load(
            user_config,
            &env.cwd,
            env.home.as_deref(),
            env.tmpdir.as_deref(),
            &mut |input| runtime.validation().parse(input),
        );
        Engine::build(loaded.config, env, loaded.warnings, None, runtime)
    }

    /// 設定をそのまま渡して作る。
    pub fn new(config: Config, env: EngineEnv, runtime: crate::runtime::ParserRuntime) -> Engine {
        Engine::build(config, env, Vec::new(), None, runtime)
    }

    /// git の起動を差し替えて作る（テスト用）。
    pub fn with_git(
        config: Config,
        env: EngineEnv,
        git: Box<dyn GitRunner>,
        runtime: crate::runtime::ParserRuntime,
    ) -> Engine {
        Engine::build(config, env, Vec::new(), Some(git), runtime)
    }

    fn build(
        config: Config,
        env: EngineEnv,
        warnings: Vec<String>,
        git: Option<Box<dyn GitRunner>>,
        runtime: crate::runtime::ParserRuntime,
    ) -> Engine {
        let judge_env = JudgeEnv {
            home: env.home.clone(),
            tmpdir: env.tmpdir.clone(),
            cwd: Some(env.cwd.clone()),
            git_enabled: config.git_enabled,
        };
        let judge = match git {
            Some(g) => Judge::with_git(judge_env, g),
            None => Judge::new(judge_env),
        };
        let mut warnings = warnings;
        let mut custom_rules = Vec::new();
        for rule in &config.rules_custom {
            match Regex::new(&rule.pattern) {
                Ok(regex) => custom_rules.push(CompiledRule {
                    name: rule.name.clone(),
                    regex,
                    verdict: rule.verdict,
                }),
                Err(e) => warnings.push(format!(
                    "カスタムのルールの正規表現が不正なため無視します: {}: {e}",
                    rule.name
                )),
            }
        }
        Engine {
            runtime: std::cell::RefCell::new(runtime),
            paths: Box::new(guardian_judge::SystemPaths),
            config,
            env,
            judge,
            warnings,
            custom_rules,
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }
    pub fn with_observers(
        config: Config,
        env: EngineEnv,
        git: Box<dyn GitRunner>,
        paths: Box<dyn guardian_judge::PathObserver>,
        runtime: crate::runtime::ParserRuntime,
    ) -> Engine {
        let mut engine = Self::with_git(config, env, git, runtime);
        engine.paths = paths;
        engine
    }

    /// コマンド文字列を判定する。
    pub fn check(&self, command: &str) -> Report {
        let core_env = Env {
            home: self.env.home.clone(),
            tmpdir: self.env.tmpdir.clone(),
            cwd: Some(self.env.cwd.clone()),
        };
        let mut runtime = self.runtime.borrow_mut();
        let mut session = runtime.judgment();
        let outcome = session.parse(command);
        let analysis = analyze(outcome, &core_env, &mut |input| session.parse(input));
        // 構文解析の ask。引用の除去の失敗も同じ列に足す（REQ-039・A23）。
        let mut asks = analysis.diagnostics;
        let mut effects = Vec::new();
        for e in &analysis.effects {
            // 無効にした組み込みルールの効果は取り出さない（REQ-026）。
            if self.config.rules_disable.iter().any(|r| r == e.op.name()) {
                continue;
            }
            match self.judge_effect(
                e.op,
                &e.target,
                std::time::Instant::now() + session.remaining(),
            ) {
                Ok(effect) => effects.push(effect),
                Err(failure) => {
                    let ask = Ask::Parse(failure);
                    if !asks.contains(&ask) {
                        asks.push(ask);
                    }
                }
            }
        }

        // カスタムのルール。引用とヒアドキュメントを外した本文に照合する。
        let mut rules = Vec::new();
        if !self.custom_rules.is_empty() {
            match session.strip_quotes(command) {
                Ok(body) => {
                    for rule in &self.custom_rules {
                        if rule.regex.is_match(&body) {
                            rules.push(RuleReport {
                                name: rule.name.clone(),
                                reason: format!("カスタムルール「{}」", rule.name),
                                verdict: rule.verdict,
                            });
                        }
                    }
                }
                Err(failure) => {
                    // 引用の除去の失敗も構文解析の失敗と同じに扱う（REQ-039・A23）。
                    // 入力に帰せる死は block、自分に帰せる失敗は ask に落ちる。
                    let ask = Ask::Parse(failure);
                    if !asks.contains(&ask) {
                        asks.push(ask);
                    }
                }
            }
        }

        // 見張りの規則。ラッパーとシェルの内側も展開して照合する（REQ-027〜REQ-034）。
        if !self.config.guard.is_empty() {
            for rule in &self.config.guard {
                if analysis.invocations.iter().any(|inv| rule.matches(inv)) {
                    rules.push(RuleReport {
                        name: rule.program.clone(),
                        reason: rule.reason.clone(),
                        verdict: rule.verdict,
                    });
                }
            }
        }

        // 見張りの規則の照合でも構文解析をする。この判定で予算を使い切って
        // いたら、上限の超過として block にする（REQ-039）。
        if session.over_budget() && !asks.iter().any(Ask::is_limit) {
            asks.push(Ask::Parse(guardian_parser::Failure::Limit));
        }
        let mut verdict = Verdict::Allow;
        for e in &effects {
            verdict = verdict.worst(e.verdict);
        }
        for r in &rules {
            verdict = verdict.worst(r.verdict);
        }
        // 構文解析由来の ask は最悪値で合成し、読めている block を上書きしない
        // （REQ-009・REQ-038・A14）。上限の超過と隔離した子の異常終了は block に
        // する（REQ-039・A22）。
        for ask in &asks {
            verdict = verdict.worst(if ask.is_limit() {
                Verdict::Block
            } else {
                Verdict::Ask
            });
        }
        let effect_message = self.compose_message(&effects, &rules);
        let message = if effect_message.is_empty() && !asks.is_empty() {
            message::ask_message(&asks)
        } else {
            effect_message
        };
        let reason = compose_reason(&effects, &rules, &asks);
        Report {
            verdict,
            effects,
            rules,
            warnings: self.warnings.clone(),
            message,
            reason,
            parse_errors: asks,
        }
    }

    fn judge_effect(
        &self,
        op: Op,
        target: &Target,
        deadline: std::time::Instant,
    ) -> Result<EffectReport, guardian_core::Failure> {
        let (class, why, verdict) = self.judge_target(target, deadline)?;
        Ok(EffectReport {
            op,
            target: target.clone(),
            class,
            why,
            verdict,
        })
    }

    fn judge_target(
        &self,
        target: &Target,
        deadline: std::time::Instant,
    ) -> Result<(Class, Why, Verdict), guardian_core::Failure> {
        match target {
            Target::Unresolved(text) => Ok((
                Class::Unknown,
                Why::Unresolved(text.clone()),
                Verdict::Block,
            )),
            Target::Mktemp => Ok((Class::Ephemeral, Why::Mktemp, Verdict::Allow)),
            Target::UnknownSource => Ok((Class::Unknown, Why::UnknownSource, Verdict::Ask)),
            _ => {
                let observed = guardian_judge::observation::observe(target, self.paths.as_ref())
                    .expect("path target");
                if let Some(overridden) = self.apply_roots(&observed) {
                    return Ok(overridden);
                }
                let c = self.judge.classify_observed(&observed, deadline)?;
                let verdict = match c.class {
                    Class::Ephemeral | Class::Vcs => Verdict::Allow,
                    Class::Protected => Verdict::Block,
                    Class::Unknown => match c.why {
                        // git の失敗は block にしない（REQ-010）。
                        Why::GitFailed => Verdict::Ask,
                        _ => self.config.unknown_verdict,
                    },
                };
                Ok((c.class, c.why, verdict))
            }
        }
    }

    /// 設定の保護ルートと許可ルートを先に当てる。
    fn apply_roots(&self, observed: &guardian_core::ObservedPath) -> Option<(Class, Why, Verdict)> {
        let path = observed.path.as_path();
        let children = observed.children;
        for root in &self.config.protected_roots {
            if root.as_os_str().is_empty() {
                continue;
            }
            if path.starts_with(root) {
                return Some((
                    Class::Protected,
                    Why::Protected(ProtectedKind::ConfiguredRoot),
                    Verdict::Block,
                ));
            }
        }
        for root in &self.config.allowed_roots {
            // 空のルートはすべてのパスに一致してしまうため当てない。
            if root.as_os_str().is_empty() {
                continue;
            }
            if path == root && !children {
                continue;
            }
            if path.starts_with(root) {
                return Some((Class::Ephemeral, Why::Ephemeral, Verdict::Allow));
            }
        }
        None
    }

    fn compose_message(&self, effects: &[EffectReport], rules: &[RuleReport]) -> String {
        let worst = worst_effect(effects);
        let bad_rules: Vec<&RuleReport> = rules
            .iter()
            .filter(|r| r.verdict != Verdict::Allow)
            .collect();
        if let Some(w) = worst {
            let text = message::non_allow_message(w.op, &w.target, w.class, &w.why);
            let others = effects
                .iter()
                .filter(|e| e.verdict != Verdict::Allow)
                .count()
                .saturating_sub(1)
                + bad_rules.len();
            if others > 0 {
                return format!("{text}\nほかに {others} 件の指摘があります");
            }
            return text;
        }
        if let Some(r) = bad_rules.first() {
            let mut text = format!("{}\n判定: {}", r.reason, r.verdict);
            if bad_rules.len() > 1 {
                text = format!("{text}\nほかに {} 件の指摘があります", bad_rules.len() - 1);
            }
            return text;
        }
        String::new()
    }
}

/// 最も重い非 allow の効果。
fn worst_effect(effects: &[EffectReport]) -> Option<&EffectReport> {
    let mut worst: Option<&EffectReport> = None;
    for effect in effects {
        if effect.verdict == Verdict::Allow {
            continue;
        }
        worst = Some(match worst {
            None => effect,
            Some(current) if effect.verdict > current.verdict => effect,
            Some(current) => current,
        });
    }
    worst
}

/// JSON の最上位に出す短い理由（REQ-017・A13）。判定の重い順に選ぶ。
fn compose_reason(effects: &[EffectReport], rules: &[RuleReport], asks: &[Ask]) -> String {
    if let Some(effect) = worst_effect(effects) {
        if effect.verdict == Verdict::Block {
            return message::reason_line(effect.class, &effect.why);
        }
    }
    // 規則が運んだ block も、構文解析の ask より重い。
    if let Some(rule) = rules.iter().find(|rule| rule.verdict == Verdict::Block) {
        return rule.reason.clone();
    }
    if !asks.is_empty() {
        return message::ask_reason(asks);
    }
    if let Some(effect) = worst_effect(effects) {
        return message::reason_line(effect.class, &effect.why);
    }
    if let Some(rule) = rules.iter().find(|rule| rule.verdict != Verdict::Allow) {
        return rule.reason.clone();
    }
    String::new()
}
