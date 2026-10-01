//! 観測済みの値から規則と最悪値を合成する。
use crate::{message, Config};
use guardian_core::{
    Ask, Class, Invocation, ObservedPath, Op, ProtectedKind, Target, Verdict, Why,
};
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectReport {
    pub op: Op,
    pub target: Target,
    pub class: Class,
    pub why: Why,
    pub verdict: Verdict,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleReport {
    pub name: String,
    pub reason: String,
    pub verdict: Verdict,
}
#[derive(Debug, Clone)]
pub struct Report {
    pub verdict: Verdict,
    pub effects: Vec<EffectReport>,
    pub rules: Vec<RuleReport>,
    pub warnings: Vec<String>,
    pub message: String,
    pub reason: String,
    pub parse_errors: Vec<Ask>,
}
struct CompiledRule {
    name: String,
    regex: Regex,
    verdict: Verdict,
}
pub struct Policy {
    config: Config,
    warnings: Vec<String>,
    custom_rules: Vec<CompiledRule>,
}
impl Policy {
    pub fn new(config: Config, mut warnings: Vec<String>) -> Self {
        let mut custom_rules = Vec::new();
        for rule in &config.rules_custom {
            match Regex::new(&rule.pattern) {
                Ok(regex) => custom_rules.push(CompiledRule {
                    name: rule.name.clone(),
                    regex,
                    verdict: rule.verdict,
                }),
                Err(e) => warnings.push(format!(
                    "Ignoring custom rule with invalid regular expression: {}: {e}",
                    rule.name
                )),
            }
        }
        Self {
            config,
            warnings,
            custom_rules,
        }
    }
    pub fn config(&self) -> &Config {
        &self.config
    }
    pub fn needs_body(&self) -> bool {
        !self.custom_rules.is_empty()
    }
    pub fn enabled(&self, op: Op) -> bool {
        !self.config.rules_disable.iter().any(|r| r == op.name())
    }
    pub fn classification_verdict(&self, class: Class, why: &Why) -> Verdict {
        match class {
            Class::Ephemeral | Class::Vcs => Verdict::Allow,
            Class::Protected => Verdict::Block,
            Class::Unknown if *why == Why::GitFailed => Verdict::Ask,
            Class::Unknown => self.config.unknown_verdict,
        }
    }
    pub fn value_target(&self, target: &Target) -> Option<(Class, Why, Verdict)> {
        match target {
            Target::Unresolved(text) => Some((
                Class::Unknown,
                Why::Unresolved(text.clone()),
                Verdict::Block,
            )),
            Target::Mktemp => Some((Class::Ephemeral, Why::Mktemp, Verdict::Allow)),
            Target::UnknownSource => Some((Class::Unknown, Why::UnknownSource, Verdict::Ask)),
            _ => None,
        }
    }
    pub fn apply_roots(&self, observed: &ObservedPath) -> Option<(Class, Why, Verdict)> {
        let path = &observed.path;
        for root in &self.config.protected_roots {
            if !root.as_os_str().is_empty() && path.starts_with(root) {
                return Some((
                    Class::Protected,
                    Why::Protected(ProtectedKind::ConfiguredRoot),
                    Verdict::Block,
                ));
            }
        }
        if !observed.children && self.config.allowed_roots.iter().any(|root| path == root) {
            return None;
        }
        for root in &self.config.allowed_roots {
            if !root.as_os_str().is_empty()
                && (path != root || observed.children)
                && path.starts_with(root)
            {
                return Some((Class::Ephemeral, Why::Ephemeral, Verdict::Allow));
            }
        }
        None
    }
    pub fn rules(&self, invocations: &[Invocation], body: Option<&str>) -> Vec<RuleReport> {
        let mut rules = Vec::new();
        if let Some(body) = body {
            for rule in &self.custom_rules {
                if rule.regex.is_match(body) {
                    rules.push(RuleReport {
                        name: rule.name.clone(),
                        reason: format!("Custom rule: {}", rule.name),
                        verdict: rule.verdict,
                    });
                }
            }
        }
        for rule in &self.config.guard {
            if invocations.iter().any(|inv| rule.matches(inv)) {
                rules.push(RuleReport {
                    name: rule.program.clone(),
                    reason: rule.reason.clone(),
                    verdict: rule.verdict,
                });
            }
        }
        rules
    }
    pub fn report(
        &self,
        effects: Vec<EffectReport>,
        rules: Vec<RuleReport>,
        asks: Vec<Ask>,
    ) -> Report {
        let verdict = effects
            .iter()
            .map(|e| e.verdict)
            .chain(rules.iter().map(|r| r.verdict))
            .chain(asks.iter().map(|ask| {
                if ask.is_limit() {
                    Verdict::Block
                } else {
                    Verdict::Ask
                }
            }))
            .fold(Verdict::Allow, Verdict::worst);
        let effect_message = compose_message(&effects, &rules);
        let explicit_block = effects.iter().any(|e| e.verdict == Verdict::Block)
            || rules.iter().any(|r| r.verdict == Verdict::Block);
        let message = if (asks.iter().any(Ask::is_limit) && !explicit_block)
            || (effect_message.is_empty() && !asks.is_empty())
        {
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
}

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
fn compose_message(effects: &[EffectReport], rules: &[RuleReport]) -> String {
    let bad_rules: Vec<_> = rules
        .iter()
        .filter(|r| r.verdict != Verdict::Allow)
        .collect();
    let rule = bad_rules
        .iter()
        .find(|r| r.verdict == Verdict::Block)
        .or_else(|| bad_rules.first());
    if let Some(w) = worst_effect(effects)
        .filter(|effect| rule.is_none_or(|rule| effect.verdict >= rule.verdict))
    {
        let text = message::non_allow_message(w.op, &w.target, w.class, &w.why);
        let others = effects
            .iter()
            .filter(|e| e.verdict != Verdict::Allow)
            .count()
            .saturating_sub(1)
            + bad_rules.len();
        return if others > 0 {
            format!("{text}\nAdditional findings: {others}")
        } else {
            text
        };
    }
    if let Some(r) = rule {
        let text = format!(
            "{}\nVerdict: {}\nAlternative: No applicable alternative.",
            message::display_inline(&r.reason),
            r.verdict
        );
        let others = bad_rules.len() - 1
            + effects
                .iter()
                .filter(|e| e.verdict != Verdict::Allow)
                .count();
        return if others > 0 {
            format!("{text}\nAdditional findings: {others}")
        } else {
            text
        };
    }
    String::new()
}
fn compose_reason(effects: &[EffectReport], rules: &[RuleReport], asks: &[Ask]) -> String {
    if let Some(effect) = worst_effect(effects) {
        if effect.verdict == Verdict::Block {
            return message::reason_line(effect.class, &effect.why);
        }
    }
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
