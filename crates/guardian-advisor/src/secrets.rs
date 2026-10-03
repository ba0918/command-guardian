use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Skip {
    NotApplicable,
    Secret,
    Size,
    Encoding,
}

fn detector() -> &'static Regex {
    static DETECTOR: OnceLock<Regex> = OnceLock::new();
    DETECTOR.get_or_init(|| Regex::new(concat!(
        r"(?i)(?:sk-[a-z0-9_-]{16,}|(?:ghp_|github_pat_)[a-z0-9_]{16,}|AKIA[A-Z0-9]{16}",
        r"|-----BEGIN (?:[A-Z ]+ )?PRIVATE KEY-----[\s\S]*?(?:-----END (?:[A-Z ]+ )?PRIVATE KEY-----|$)",
        r"|Bearer\s+[a-z0-9._~+/-]{8,}=*",
        r#"|(?:password|passwd|api[_-]?key|access[_-]?token|secret[_-]?key)["']?\s*[:=]\s*["']?[^\s"',;}]+)"#
    )).expect("static secret detector"))
}

pub(crate) fn contains_secret(text: &str) -> bool {
    detector().is_match(text)
}

pub(crate) fn json_contains_secret(value: &Value) -> bool {
    match value {
        Value::String(text) => contains_secret(text),
        Value::Array(values) => values.iter().any(json_contains_secret),
        Value::Object(values) => values.iter().any(|(key, value)| {
            contains_secret(key)
                || (credential_key(key) && value.as_str().is_some_and(|v| !v.is_empty()))
                || json_contains_secret(value)
        }),
        _ => false,
    }
}

fn credential_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().replace('-', "_").as_str(),
        "password" | "passwd" | "api_key" | "apikey" | "access_token" | "secret_key"
    )
}

pub fn redact_secrets(text: &str) -> String {
    if let Ok(mut value) = serde_json::from_str::<Value>(text) {
        redact_json(&mut value);
        if let Ok(encoded) = serde_json::to_string(&value) {
            return encoded;
        }
    }
    detector().replace_all(text, "[REDACTED]").into_owned()
}

fn redact_json(value: &mut Value) {
    match value {
        Value::String(text) => *text = detector().replace_all(text, "[REDACTED]").into_owned(),
        Value::Array(values) => values.iter_mut().for_each(redact_json),
        Value::Object(values) => {
            // Renaming secret-bearing keys could collide with an existing plaintext key.
            if values.keys().any(|key| contains_secret(key)) {
                *value = Value::String("[REDACTED]".into());
                return;
            }
            for (key, value) in values {
                if credential_key(key) && !value.is_null() {
                    *value = Value::String("[REDACTED]".into());
                } else {
                    redact_json(value);
                }
            }
        }
        _ => (),
    }
}
