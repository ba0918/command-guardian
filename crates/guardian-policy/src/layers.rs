//! 設定のファイルと層。組み込み・利用者・プロジェクトの 3 層を読み、マージする。
//! プロジェクトの層は、信頼されていなければ締める方向の変更だけを反映する。

use crate::guard::{self, GuardRule};
use guardian_core::Verdict;
use std::path::{Path, PathBuf};

use crate::config::{Config, CustomRule};

/// 1 つの層の生の値。
#[derive(Debug, Clone, Default)]
pub struct Layer {
    pub allowed_roots: Vec<PathBuf>,
    pub protected_roots: Vec<PathBuf>,
    pub unknown_verdict: Option<Verdict>,
    pub rules_disable: Vec<String>,
    pub rules_custom: Vec<CustomRule>,
    pub guard: Vec<GuardRule>,
    pub git_enabled: Option<bool>,
    pub enforce: Option<bool>,
    pub trusted_projects: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

/// 層の TOML を読む。読めない場合は Err。
pub fn parse_layer(text: &str, base: &Path, home: Option<&Path>) -> Result<Layer, String> {
    let value: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
    let mut layer = Layer::default();
    let root = &value;
    if root.as_table().is_none() {
        return Err("TOML の表ではない".into());
    }

    read_path_list(
        root,
        "paths",
        "allowed_roots",
        base,
        home,
        &mut layer.allowed_roots,
        &mut layer.warnings,
    );
    read_path_list(
        root,
        "paths",
        "protected_roots",
        base,
        home,
        &mut layer.protected_roots,
        &mut layer.warnings,
    );
    layer.unknown_verdict = read_verdict(root, "unknown", "verdict", &mut layer.warnings);
    read_string_list(
        root,
        "rules",
        "disable",
        &mut layer.rules_disable,
        &mut layer.warnings,
    );
    read_custom_rules(root, &mut layer.rules_custom, &mut layer.warnings);
    layer.guard = guard::parse_guards(root, &mut layer.warnings);
    layer.git_enabled = read_bool(root, "git", "enabled", &mut layer.warnings);
    layer.enforce = read_bool(root, "mode", "enforce", &mut layer.warnings);
    read_path_list(
        root,
        "",
        "trusted_projects",
        base,
        home,
        &mut layer.trusted_projects,
        &mut layer.warnings,
    );
    Ok(layer)
}

fn table<'a>(root: &'a toml::Value, key: &str) -> Option<&'a toml::Value> {
    if key.is_empty() {
        Some(root)
    } else {
        root.get(key)
    }
}

fn read_path_list(
    root: &toml::Value,
    section: &str,
    key: &str,
    base: &Path,
    home: Option<&Path>,
    out: &mut Vec<PathBuf>,
    warnings: &mut Vec<String>,
) {
    let Some(value) = table(root, section).and_then(|t| t.get(key)) else {
        return;
    };
    let Some(items) = value.as_array() else {
        warnings.push(format!("{section}.{key} はリストではないため無視します"));
        return;
    };
    for item in items {
        let Some(s) = item.as_str() else {
            warnings.push(format!(
                "{section}.{key} の要素が文字列ではないため無視します"
            ));
            continue;
        };
        out.push(resolve_config_path(s, base, home));
    }
}

fn resolve_config_path(s: &str, base: &Path, home: Option<&Path>) -> PathBuf {
    let expanded = if s == "~" {
        home.map(|h| h.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(s))
    } else if let Some(rest) = s.strip_prefix("~/") {
        match home {
            Some(h) => h.join(rest),
            None => PathBuf::from(s),
        }
    } else {
        PathBuf::from(s)
    };
    if expanded.is_absolute() {
        expanded
    } else {
        base.join(expanded)
    }
}

fn read_string_list(
    root: &toml::Value,
    section: &str,
    key: &str,
    out: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    let Some(value) = table(root, section).and_then(|t| t.get(key)) else {
        return;
    };
    let Some(items) = value.as_array() else {
        warnings.push(format!("{section}.{key} はリストではないため無視します"));
        return;
    };
    for item in items {
        match item.as_str() {
            Some(s) => out.push(s.to_string()),
            None => warnings.push(format!(
                "{section}.{key} の要素が文字列ではないため無視します"
            )),
        }
    }
}

fn read_verdict(
    root: &toml::Value,
    section: &str,
    key: &str,
    warnings: &mut Vec<String>,
) -> Option<Verdict> {
    let value = table(root, section).and_then(|t| t.get(key))?;
    match value.as_str() {
        Some("ask") => Some(Verdict::Ask),
        Some("block") => Some(Verdict::Block),
        Some("allow") => {
            warnings.push(format!("{section}.{key} に allow は書けないため無視します"));
            None
        }
        _ => {
            warnings.push(format!("{section}.{key} の値が不正なため無視します"));
            None
        }
    }
}

fn read_bool(
    root: &toml::Value,
    section: &str,
    key: &str,
    warnings: &mut Vec<String>,
) -> Option<bool> {
    let value = table(root, section).and_then(|t| t.get(key))?;
    match value.as_bool() {
        Some(b) => Some(b),
        None => {
            warnings.push(format!("{section}.{key} が真偽値ではないため無視します"));
            None
        }
    }
}

fn read_custom_rules(root: &toml::Value, out: &mut Vec<CustomRule>, warnings: &mut Vec<String>) {
    let Some(value) = root.get("rules").and_then(|r| r.get("custom")) else {
        return;
    };
    let Some(items) = value.as_array() else {
        warnings.push("rules.custom はリストではないため無視します".to_string());
        return;
    };
    for item in items {
        let name = item.get("name").and_then(|v| v.as_str());
        let pattern = item.get("pattern").and_then(|v| v.as_str());
        let verdict = item.get("verdict").and_then(|v| v.as_str());
        match (name, pattern, verdict) {
            (Some(name), Some(pattern), Some(verdict)) => {
                let verdict = match verdict {
                    "allow" => Verdict::Allow,
                    "ask" => Verdict::Ask,
                    "block" => Verdict::Block,
                    _ => {
                        warnings.push(format!(
                            "rules.custom の verdict が不正なため無視します: {name}"
                        ));
                        continue;
                    }
                };
                out.push(CustomRule {
                    name: name.to_string(),
                    pattern: pattern.to_string(),
                    verdict,
                });
            }
            _ => warnings.push(
                "rules.custom の規則に name・pattern・verdict が無いため無視します".to_string(),
            ),
        }
    }
}

/// 層をマージする。リストは足し合わせ、スカラーは後勝ち。
pub fn merge(config: &mut Config, layer: &Layer) {
    config
        .allowed_roots
        .extend(layer.allowed_roots.iter().cloned());
    config
        .protected_roots
        .extend(layer.protected_roots.iter().cloned());
    if let Some(v) = layer.unknown_verdict {
        config.unknown_verdict = v;
    }
    config
        .rules_disable
        .extend(layer.rules_disable.iter().cloned());
    config
        .rules_custom
        .extend(layer.rules_custom.iter().cloned());
    config.guard.extend(layer.guard.iter().cloned());
    if let Some(b) = layer.git_enabled {
        config.git_enabled = b;
    }
    if let Some(b) = layer.enforce {
        config.enforce = b;
    }
    config
        .trusted_projects
        .extend(layer.trusted_projects.iter().cloned());
}

/// 信頼していないプロジェクトの層から、締める方向の変更だけを反映する。
pub fn restrict_project(config: &mut Config, layer: &Layer) -> Vec<String> {
    let mut warnings = Vec::new();
    config
        .protected_roots
        .extend(layer.protected_roots.iter().cloned());
    if let Some(v) = layer.unknown_verdict {
        match (config.unknown_verdict, v) {
            (_, Verdict::Block) => config.unknown_verdict = Verdict::Block,
            (Verdict::Block, Verdict::Ask) => {
                warnings.push(
                    "信頼していないプロジェクト設定の unknown.verdict の緩和を無視します"
                        .to_string(),
                );
            }
            _ => {}
        }
    }
    config.guard.extend(layer.guard.iter().cloned());
    for rule in &layer.rules_custom {
        if rule.verdict == Verdict::Allow {
            warnings.push(format!(
                "信頼していないプロジェクト設定の allow のルールを無視します: {}",
                rule.name
            ));
        } else {
            config.rules_custom.push(rule.clone());
        }
    }
    if !layer.allowed_roots.is_empty() {
        warnings
            .push("信頼していないプロジェクト設定の allowed_roots の追加を無視します".to_string());
    }
    if !layer.rules_disable.is_empty() {
        warnings.push("信頼していないプロジェクト設定の rules.disable を無視します".to_string());
    }
    if layer.git_enabled == Some(false) {
        warnings
            .push("信頼していないプロジェクト設定の git.enabled の無効化を無視します".to_string());
    }
    if layer.enforce == Some(false) {
        warnings
            .push("信頼していないプロジェクト設定の mode.enforce の無効化を無視します".to_string());
    }
    if !layer.trusted_projects.is_empty() {
        warnings.push("プロジェクト設定の trusted_projects は無視します".to_string());
    }
    warnings
}
