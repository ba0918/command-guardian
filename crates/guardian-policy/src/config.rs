//! マージ後の設定値。
use crate::guard::GuardRule;
use guardian_core::Verdict;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomRule {
    pub name: String,
    pub pattern: String,
    pub verdict: Verdict,
}

pub(crate) fn normalize_root(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if out.file_name().is_some_and(|name| name != "..") {
                    out.pop();
                } else if !out.has_root() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub advisor: AdvisorConfig,
    pub allowed_roots: Vec<PathBuf>,
    pub protected_roots: Vec<PathBuf>,
    pub unknown_verdict: Verdict,
    pub rules_disable: Vec<String>,
    pub rules_custom: Vec<CustomRule>,
    pub guard: Vec<GuardRule>,
    pub git_enabled: bool,
    pub enforce: bool,
    pub defer_ask: bool,
    pub trusted_projects: Vec<PathBuf>,
}

impl Config {
    pub fn builtin(tmpdir: Option<&Path>) -> Config {
        let mut allowed = vec![PathBuf::from("/tmp"), PathBuf::from("/var/tmp")];
        if let Some(t) = tmpdir.filter(|t| !t.as_os_str().is_empty())
            && !allowed.contains(&t.to_path_buf())
        {
            allowed.push(t.to_path_buf());
        }
        Config {
            advisor: AdvisorConfig::default(),
            allowed_roots: allowed,
            protected_roots: Vec::new(),
            unknown_verdict: Verdict::Ask,
            rules_disable: Vec::new(),
            rules_custom: Vec::new(),
            guard: Vec::new(),
            git_enabled: true,
            enforce: true,
            defer_ask: false,
            trusted_projects: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdvisorConfig {
    pub mode: guardian_advisor::Mode,
    pub model: String,
    pub timeout_ms: u64,
    pub max_request_bytes: usize,
    pub intervention_threshold: f64,
    pub context_exchanges: usize,
    pub context_ttl_hours: u64,
    pub debug_text: bool,
}

impl Default for AdvisorConfig {
    fn default() -> Self {
        Self {
            mode: guardian_advisor::Mode::Off,
            model: "jev-latest".into(),
            timeout_ms: 2000,
            max_request_bytes: 65536,
            intervention_threshold: 0.9,
            context_exchanges: 3,
            context_ttl_hours: 24,
            debug_text: false,
        }
    }
}

impl AdvisorConfig {
    pub(crate) fn parse(value: &toml::Value) -> Option<Self> {
        use std::time::{Duration, SystemTime};
        let table = value.as_table()?;
        if table.keys().any(|key| {
            !matches!(
                key.as_str(),
                "mode"
                    | "model"
                    | "timeout_ms"
                    | "max_request_bytes"
                    | "intervention_threshold"
                    | "context_exchanges"
                    | "context_ttl_hours"
                    | "debug_text"
            )
        }) {
            return None;
        }
        let mut a = Self::default();
        if let Some(value) = table.get("mode") {
            a.mode = match value.as_str()? {
                "off" => guardian_advisor::Mode::Off,
                "observe" => guardian_advisor::Mode::Observe,
                "enforce" => guardian_advisor::Mode::Enforce,
                _ => return None,
            };
        }
        if let Some(value) = table.get("model") {
            let model = value.as_str()?;
            if model.is_empty() || model.chars().any(char::is_control) {
                return None;
            }
            a.model = model.into();
        }
        fn integer(
            table: &toml::map::Map<String, toml::Value>,
            key: &str,
            default: u64,
            zero: bool,
        ) -> Option<u64> {
            let value = match table.get(key) {
                Some(value) => u64::try_from(value.as_integer()?).ok()?,
                None => default,
            };
            if value == 0 && !zero {
                return None;
            }
            Some(value)
        }
        a.timeout_ms = integer(table, "timeout_ms", a.timeout_ms, false)?;
        a.max_request_bytes = usize::try_from(integer(
            table,
            "max_request_bytes",
            a.max_request_bytes as u64,
            false,
        )?)
        .ok()?;
        a.context_exchanges = usize::try_from(integer(
            table,
            "context_exchanges",
            a.context_exchanges as u64,
            true,
        )?)
        .ok()?;
        a.context_ttl_hours = integer(table, "context_ttl_hours", a.context_ttl_hours, false)?;
        let timeout = Duration::from_millis(a.timeout_ms);
        u64::try_from(timeout.as_nanos()).ok()?;
        timeout.checked_add(Duration::from_millis(1000))?;
        a.timeout_ms.checked_add(1000)?;
        let ttl = Duration::from_secs(a.context_ttl_hours.checked_mul(3600)?);
        SystemTime::UNIX_EPOCH.checked_add(ttl)?;
        u32::try_from(a.max_request_bytes.checked_add(65536)?).ok()?;
        if let Some(value) = table.get("intervention_threshold") {
            a.intervention_threshold = match value {
                toml::Value::Float(f) => *f,
                toml::Value::Integer(i) => *i as f64,
                _ => return None,
            };
            if !a.intervention_threshold.is_finite()
                || a.intervention_threshold <= 0.0
                || a.intervention_threshold > 1.0
            {
                return None;
            }
        }
        if let Some(value) = table.get("debug_text") {
            a.debug_text = value.as_bool()?;
        }
        Some(a)
    }
}
