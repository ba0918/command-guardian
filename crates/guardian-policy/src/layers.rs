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
        return Err("Expected a TOML table".into());
    }
    for section in ["paths", "unknown", "rules", "git", "mode"] {
        if root
            .get(section)
            .is_some_and(|value| value.as_table().is_none())
        {
            layer
                .warnings
                .push(format!("Ignoring {section}: expected a table"));
        }
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
        warnings.push(format!("Ignoring {section}.{key}: expected a list"));
        return;
    };
    for item in items {
        let Some(s) = item.as_str() else {
            warnings.push(format!(
                "Ignoring an entry in {section}.{key}: expected a string"
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
        warnings.push(format!("Ignoring {section}.{key}: expected a list"));
        return;
    };
    for item in items {
        match item.as_str() {
            Some(s) => out.push(s.to_string()),
            None => warnings.push(format!(
                "Ignoring an entry in {section}.{key}: expected a string"
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
            warnings.push(format!("Ignoring {section}.{key}: allow is not permitted"));
            None
        }
        _ => {
            warnings.push(format!("Ignoring {section}.{key}: invalid value"));
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
            warnings.push(format!("Ignoring {section}.{key}: expected a boolean"));
            None
        }
    }
}

fn read_custom_rules(root: &toml::Value, out: &mut Vec<CustomRule>, warnings: &mut Vec<String>) {
    let Some(value) = root.get("rules").and_then(|r| r.get("custom")) else {
        return;
    };
    let Some(items) = value.as_array() else {
        warnings.push("Ignoring rules.custom: expected a list".to_string());
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
                            "Ignoring rules.custom entry with invalid verdict: {name}"
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
                "Ignoring rules.custom entry: name, pattern and verdict are required".to_string(),
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
                    "Ignoring a weaker unknown.verdict in untrusted project configuration"
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
                "Ignoring allow rule in untrusted project configuration: {}",
                rule.name
            ));
        } else {
            config.rules_custom.push(rule.clone());
        }
    }
    if !layer.allowed_roots.is_empty() {
        warnings.push(
            "Ignoring allowed_roots additions in untrusted project configuration".to_string(),
        );
    }
    if !layer.rules_disable.is_empty() {
        warnings.push("Ignoring rules.disable in untrusted project configuration".to_string());
    }
    if layer.git_enabled == Some(false) {
        warnings.push("Ignoring git.enabled=false in untrusted project configuration".to_string());
    }
    if layer.enforce == Some(false) {
        warnings.push("Ignoring mode.enforce=false in untrusted project configuration".to_string());
    }
    if !layer.trusted_projects.is_empty() {
        warnings.push("Ignoring trusted_projects in project configuration".to_string());
    }
    warnings
}
