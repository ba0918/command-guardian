//! 判定の入口。設定を読み、効果ごとに分類と判定を出し、合成する。

use crate::config::{self, Config};
use crate::guard;
use crate::message;
use guardian_core::{
    analyze, strip_quotes_and_heredocs, Class, Env, Op, ProtectedKind, Target, Verdict, Why,
};
use guardian_judge::{GitRunner, Judge, JudgeEnv};
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
    pub parse_errors: Vec<String>,
}

struct CompiledRule {
    name: String,
    regex: Regex,
    verdict: Verdict,
}

pub struct Engine {
    config: Config,
    env: EngineEnv,
    judge: Judge,
    warnings: Vec<String>,
    custom_rules: Vec<CompiledRule>,
}

impl Engine {
    /// 設定を読み込んで作る。
    pub fn load(user_config: Option<&Path>, env: EngineEnv) -> Engine {
        let loaded = config::load(
            user_config,
            &env.cwd,
            env.home.as_deref(),
            env.tmpdir.as_deref(),
        );
        Engine::build(loaded.config, env, loaded.warnings, None)
    }

    /// 設定をそのまま渡して作る。
    pub fn new(config: Config, env: EngineEnv) -> Engine {
        Engine::build(config, env, Vec::new(), None)
    }

    /// git の起動を差し替えて作る（テスト用）。
    pub fn with_git(config: Config, env: EngineEnv, git: Box<dyn GitRunner>) -> Engine {
        Engine::build(config, env, Vec::new(), Some(git))
    }

    fn build(
        config: Config,
        env: EngineEnv,
        warnings: Vec<String>,
        git: Option<Box<dyn GitRunner>>,
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

    /// コマンド文字列を判定する。
    pub fn check(&self, command: &str) -> Report {
        let core_env = Env {
            home: self.env.home.clone(),
            tmpdir: self.env.tmpdir.clone(),
            cwd: Some(self.env.cwd.clone()),
        };
        let analysis = analyze(command, &core_env);
        let mut effects = Vec::new();
        for e in &analysis.effects {
            // 無効にした組み込みルールの効果は取り出さない（REQ-026）。
            if self.config.rules_disable.iter().any(|r| r == e.op.name()) {
                continue;
            }
            effects.push(self.judge_effect(e.op, &e.target));
        }

        // カスタムのルール。引用とヒアドキュメントを外した本文に照合する。
        let body = strip_quotes_and_heredocs(command);
        let mut rules = Vec::new();
        for rule in &self.custom_rules {
            if rule.regex.is_match(&body) {
                rules.push(RuleReport {
                    name: rule.name.clone(),
                    reason: format!("カスタムルール「{}」", rule.name),
                    verdict: rule.verdict,
                });
            }
        }

        // 見張りの規則。ラッパーとシェルの内側も展開して照合する（REQ-027〜REQ-034）。
        let invocations = guard::invocations(command);
        for rule in &self.config.guard {
            if invocations.iter().any(|inv| rule.matches(inv)) {
                rules.push(RuleReport {
                    name: rule.program.clone(),
                    reason: rule.reason.clone(),
                    verdict: rule.verdict,
                });
            }
        }

        let mut verdict = Verdict::Allow;
        for e in &effects {
            verdict = verdict.worst(e.verdict);
        }
        for r in &rules {
            verdict = verdict.worst(r.verdict);
        }
        let mut message = self.compose_message(&effects, &rules);
        if !analysis.parse_errors.is_empty() {
            verdict = Verdict::Ask;
            message = parse_error_message();
        }
        Report {
            verdict,
            effects,
            rules,
            warnings: self.warnings.clone(),
            message,
            parse_errors: analysis.parse_errors,
        }
    }

    fn judge_effect(&self, op: Op, target: &Target) -> EffectReport {
        let (class, why, verdict) = self.judge_target(target);
        EffectReport {
            op,
            target: target.clone(),
            class,
            why,
            verdict,
        }
    }

    fn judge_target(&self, target: &Target) -> (Class, Why, Verdict) {
        match target {
            Target::Unresolved(text) => (
                Class::Unknown,
                Why::Unresolved(text.clone()),
                Verdict::Block,
            ),
            Target::Mktemp => (Class::Ephemeral, Why::Mktemp, Verdict::Allow),
            Target::UnknownSource => (Class::Unknown, Why::UnknownSource, Verdict::Ask),
            _ => {
                if let Some(overridden) = self.apply_roots(target) {
                    return overridden;
                }
                let c = match target {
                    Target::Path { path, dereference } => {
                        self.judge.classify_path(path, *dereference)
                    }
                    Target::GlobBase(base) => self.judge.classify_path(base, false),
                    Target::Children { base, dereference } => {
                        self.judge.classify_children_deref(base, *dereference)
                    }
                    _ => unreachable!(),
                };
                let verdict = match c.class {
                    Class::Ephemeral | Class::Vcs => Verdict::Allow,
                    Class::Protected => Verdict::Block,
                    Class::Unknown => match c.why {
                        // git の失敗は block にしない（REQ-010）。
                        Why::GitFailed => Verdict::Ask,
                        _ => self.config.unknown_verdict,
                    },
                };
                (c.class, c.why, verdict)
            }
        }
    }

    /// 設定の保護ルートと許可ルートを先に当てる。
    fn apply_roots(&self, target: &Target) -> Option<(Class, Why, Verdict)> {
        let (path, children) = match target {
            Target::Path { path, dereference } => (self.deref_path(path, *dereference), false),
            Target::GlobBase(base) => (base.clone(), false),
            Target::Children { base, dereference } => (self.deref_path(base, *dereference), true),
            _ => return None,
        };
        let path = path.as_path();
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

    /// 末尾スラッシュ付きの対象は、リンク先を解決してからルートに当てる。
    fn deref_path(&self, path: &Path, dereference: bool) -> PathBuf {
        if dereference {
            std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        } else {
            path.to_path_buf()
        }
    }

    fn compose_message(&self, effects: &[EffectReport], rules: &[RuleReport]) -> String {
        let mut worst: Option<&EffectReport> = None;
        for e in effects {
            if e.verdict == Verdict::Allow {
                continue;
            }
            worst = Some(match worst {
                None => e,
                Some(w) if e.verdict > w.verdict => e,
                Some(w) => w,
            });
        }
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

fn parse_error_message() -> String {
    "解析できない入力です\n代替: リテラルのパスで指定し直してください".to_string()
}
