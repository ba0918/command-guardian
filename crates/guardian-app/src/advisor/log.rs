use super::{wire::Reply, Evaluation};
use guardian_advisor::redact_secrets;
use guardian_core::Verdict;
use guardian_policy::config::AdvisorConfig;
use serde_json::{json, Value};

pub fn record(
    timestamp: u64,
    config: &AdvisorConfig,
    machine: Verdict,
    evaluation: &Evaluation,
    candidate_text: Option<&str>,
) -> Value {
    let mut value = json!({
        "timestamp": timestamp,
        "mode": config.mode.as_str(),
        "machine": machine.to_string(),
        "candidate": evaluation.outcome.candidate.to_string(),
        "final": evaluation.outcome.final_verdict.to_string(),
        "elapsed_ms": evaluation.elapsed_ms,
        "failure": evaluation.failure,
        "model": redact_secrets(&config.model),
        "unreaped": evaluation.unreaped,
    });
    match &evaluation.reply {
        Some(Reply::Assessment { risk, scope, .. }) => {
            if let Ok(answer) = guardian_advisor::Assessment::validate(risk.clone(), scope.clone())
            {
                value["reason"] = json!(answer.risk_distribution().selected);
            }
        }
        Some(Reply::Skipped { skip }) => value["skip"] = json!(skip),
        Some(Reply::Failure { .. }) | None => (),
    }
    if config.debug_text {
        if let Some(text) = candidate_text {
            value["text"] = json!(redact_secrets(text));
        }
    }
    value
}
