//! 設定の探索・読込と、隔離された設定例の検証。
use crate::runtime::ValidationSession;
use guardian_core::{Env, Verdict};
use guardian_policy::layers::{self, Layer};
use guardian_policy::{Config, GuardRule};
use std::path::{Path, PathBuf};

pub struct Loaded {
    pub config: Config,
    pub warnings: Vec<String>,
    pub project_config: Option<PathBuf>,
}
pub fn user_config_path(xdg: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    xdg.filter(|x| !x.as_os_str().is_empty())
        .map(|x| x.join("command-guardian/config.toml"))
        .or_else(|| {
            home.filter(|h| !h.as_os_str().is_empty())
                .map(|h| h.join(".config/command-guardian/config.toml"))
        })
}
pub fn find_project_config(cwd: &Path) -> Option<PathBuf> {
    cwd.ancestors()
        .map(|dir| dir.join(".command-guardian.toml"))
        .find(|candidate| candidate.is_file())
}
fn same_path(a: &Path, b: &Path) -> bool {
    std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf())
        == std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf())
}
fn read_layer(
    path: &Path,
    base: &Path,
    home: Option<&Path>,
    session: &mut ValidationSession,
) -> Result<Layer, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut layer =
        layers::parse_layer(&text, base, home).map_err(|e| format!("{}: {e}", path.display()))?;
    validate_examples(&mut layer.guard, &mut layer.warnings, session);
    Ok(layer)
}
pub fn load(
    user_config: Option<&Path>,
    cwd: &Path,
    home: Option<&Path>,
    tmpdir: Option<&Path>,
    session: &mut ValidationSession,
) -> Loaded {
    let mut warnings = Vec::new();
    let mut config = Config::builtin(tmpdir);
    let user = match user_config {
        Some(path) if path.exists() => {
            match read_layer(path, path.parent().unwrap_or(Path::new("/")), home, session) {
                Ok(layer) => layer,
                Err(e) => {
                    warnings.push(format!("利用者設定を読めないため既定で続けます: {e}"));
                    Layer::default()
                }
            }
        }
        _ => Layer::default(),
    };
    warnings.extend(user.warnings.iter().cloned());
    let trusted = user.trusted_projects.clone();
    layers::merge(&mut config, &user);
    let project_config = find_project_config(cwd);
    if let Some(path) = &project_config {
        let file_dir = path.parent().unwrap_or(Path::new("/"));
        let base =
            guardian_judge::find_worktree_root(file_dir).unwrap_or_else(|| file_dir.to_path_buf());
        match read_layer(path, &base, home, session) {
            Ok(mut layer) => {
                warnings.extend(layer.warnings.iter().cloned());
                if trusted
                    .iter()
                    .any(|t| same_path(t, file_dir) || same_path(t, &base))
                {
                    layer.rules_custom.retain(|rule| {
                        if rule.verdict == Verdict::Allow {
                            warnings.push(format!("allow を返すカスタムのルールは利用者設定でのみ有効なため無視します: {}", rule.name)); false
                        } else { true }
                    });
                    layers::merge(&mut config, &layer);
                } else {
                    warnings.extend(layers::restrict_project(&mut config, &layer));
                }
            }
            Err(e) => warnings.push(format!("プロジェクト設定を読めないため無視します: {e}")),
        }
    }
    Loaded {
        config,
        warnings,
        project_config,
    }
}

pub fn validate_examples(
    rules: &mut Vec<GuardRule>,
    warnings: &mut Vec<String>,
    session: &mut ValidationSession,
) {
    rules.retain(|rule| {
        for (examples, deny) in [(&rule.examples_deny, true), (&rule.examples_allow, false)] {
            for example in examples {
                let outcome = session.parse(example);
                let facts = guardian_analysis::analyze(
                    outcome,
                    &Env {
                        home: None,
                        tmpdir: None,
                        cwd: None,
                    },
                    &mut |input| session.parse(input),
                );
                let error = match facts.invocations.as_slice() {
                    [inv] if rule.matches(inv) == deny => continue,
                    [_] if deny => "examples.deny に一致しない例がある",
                    [_] => "examples.allow に一致する例がある",
                    _ => "例を 1 つの起動として読めない",
                };
                warnings.push(format!(
                    "見張りの規則を無効にします（{}）: {error}: {example}",
                    rule.program
                ));
                return false;
            }
        }
        true
    });
}
