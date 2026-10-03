use super::{
    context,
    wire::{self, FrameKind, Reply},
};
use crate::state::AdvisorState;
use guardian_advisor::{AdvisorClient, ContextWindow, Failure, Skip, State};
use guardian_core::Verdict;
use serde::{Deserialize, Serialize};
use std::os::fd::FromRawFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(super) const WORKER_ENV: &str = "COMMAND_GUARDIAN_ADVISOR_WORKER";
pub(super) const BUDGET_ENV: &str = "COMMAND_GUARDIAN_ADVISOR_BUDGET_NS";
pub(super) const LIMIT_ENV: &str = "COMMAND_GUARDIAN_ADVISOR_FRAME_LIMIT";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "host",
    content = "input",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Source {
    Cli,
    Claude(serde_json::Value),
    ClaudeEvent(serde_json::Value),
    Codex(serde_json::Value),
    OpenCode,
}

impl Source {
    pub fn from_hook(claude: bool, text: &str) -> Self {
        match guardian_advisor::parse_unique_json(text) {
            Ok(value) if claude => Self::Claude(value),
            Ok(value) => Self::Codex(value),
            Err(_) => Self::Cli,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub model: String,
    pub max_request_bytes: usize,
    pub context_exchanges: usize,
    pub context_ttl_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub command: String,
    pub cwd: String,
    pub machine: String,
    pub reasons: Vec<String>,
    pub source: Source,
    pub state_base: Option<PathBuf>,
    pub settings: Settings,
    #[serde(default)]
    pub log: Option<Logging>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Logging {
    pub mode: String,
    pub threshold: f64,
    pub debug_text: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SkippedLog {
    pub skip: Skip,
    pub machine: String,
    pub state_base: Option<PathBuf>,
    pub model: String,
    pub log: Logging,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Work {
    Evaluate(Request),
    SkippedLog(SkippedLog),
}

fn log_config(
    model: &str,
    logging: &Logging,
) -> Result<guardian_policy::config::AdvisorConfig, Failure> {
    let mode = match logging.mode.as_str() {
        "observe" => guardian_advisor::Mode::Observe,
        "enforce" => guardian_advisor::Mode::Enforce,
        _ => return Err(Failure::InvalidResponse),
    };
    if !logging.threshold.is_finite() || logging.threshold <= 0.0 || logging.threshold > 1.0 {
        return Err(Failure::InvalidResponse);
    }
    Ok(guardian_policy::config::AdvisorConfig {
        mode,
        intervention_threshold: logging.threshold,
        debug_text: logging.debug_text,
        model: model.into(),
        ..Default::default()
    })
}

fn save_log(
    base: Option<&PathBuf>,
    config: &guardian_policy::config::AdvisorConfig,
    machine: Verdict,
    reply: Reply,
    candidate_text: Option<&str>,
    started: Instant,
    deadline: Instant,
) {
    let validated = reply.validated();
    let answer = validated.as_ref().ok().and_then(|a| a.as_ref());
    let outcome = guardian_advisor::combine(
        config.mode,
        machine,
        answer.map(|(a, _)| a),
        answer
            .map(|(_, e)| *e)
            .unwrap_or(guardian_advisor::ScopeEvidence::Unavailable),
        config.intervention_threshold,
    );
    let evaluation = super::Evaluation {
        outcome,
        failure: validated.err(),
        reply: Some(reply),
        unreaped: false,
        elapsed_ms: started.elapsed().as_millis(),
    };
    let saved = || -> std::io::Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| std::io::Error::other("Advisor clock unavailable"))?
            .as_secs();
        let value = super::log::record(timestamp, config, machine, &evaluation, candidate_text);
        let base = base.ok_or_else(|| std::io::Error::other("No advisor log location"))?;
        AdvisorState::open(base, deadline)?.append_advisor_log(&value, deadline)
    };
    if saved().is_err() {
        eprintln!("Warning: Could not write advisor log.");
    }
}

pub(super) fn remaining(deadline: Instant) -> Result<Duration, Failure> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        Err(Failure::Timeout)
    } else {
        Ok(remaining)
    }
}

fn verdict(value: &str) -> Result<Verdict, Failure> {
    match value {
        "allow" => Ok(Verdict::Allow),
        "ask" => Ok(Verdict::Ask),
        "block" => Ok(Verdict::Block),
        _ => Err(Failure::InvalidResponse),
    }
}

fn acquire(request: &Request, max_bytes: usize, deadline: Instant) -> ContextWindow {
    if request.settings.context_exchanges == 0 {
        return ContextWindow::default();
    }
    match &request.source {
        Source::Cli | Source::OpenCode | Source::ClaudeEvent(_) => ContextWindow::default(),
        Source::Codex(hook) => context::acquire_codex(
            hook,
            max_bytes,
            request.settings.context_exchanges,
            deadline,
        ),
        Source::Claude(hook) => {
            let state = request
                .state_base
                .as_ref()
                .and_then(|base| AdvisorState::open(base, deadline).ok());
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|t| t.as_secs());
            match (state, now) {
                (Some(state), Some(now)) => context::acquire_claude(
                    hook,
                    &state,
                    context::CacheLimits {
                        max_bytes,
                        exchanges: request.settings.context_exchanges,
                        ttl: request.settings.context_ttl_seconds,
                        now,
                        deadline,
                    },
                ),
                _ => context::read_transcript(hook, max_bytes, deadline)
                    .ok()
                    .map(|bytes| {
                        context::claude_context(
                            hook,
                            &bytes,
                            max_bytes,
                            request.settings.context_exchanges,
                        )
                    })
                    .unwrap_or_default(),
            }
        }
    }
}

fn prepare<A>(
    request: &Request,
    deadline: Instant,
    acquire: A,
) -> Result<
    (
        guardian_advisor::SendPermit,
        guardian_advisor::ScopeEvidence,
    ),
    Reply,
>
where
    A: FnOnce(&Request, usize, Instant) -> ContextWindow,
{
    remaining(deadline).map_err(|failure| Reply::Failure { failure })?;
    let machine = verdict(&request.machine).map_err(|failure| Reply::Failure { failure })?;
    if machine == Verdict::Block {
        return Err(Reply::Skipped {
            skip: Skip::NotApplicable,
        });
    }
    if request.settings.model.is_empty()
        || request.settings.model.chars().any(char::is_control)
        || request.settings.max_request_bytes == 0
        || request.settings.context_ttl_seconds == 0
    {
        return Err(Reply::Failure {
            failure: Failure::InvalidResponse,
        });
    }
    let max_bytes = wire::request_limit(request.settings.max_request_bytes)
        .map_err(|failure| Reply::Failure { failure })?;
    let context = acquire(request, max_bytes, deadline);
    remaining(deadline).map_err(|failure| Reply::Failure { failure })?;
    let evidence = context.evidence;
    let state = State {
        command: request.command.clone(),
        cwd: request.cwd.clone(),
        machine,
        reasons: request.reasons.clone(),
        context,
    };
    let permit = guardian_advisor_typesafe::prepare(
        &state,
        &request.settings.model,
        request.settings.max_request_bytes,
    )
    .map_err(|skip| Reply::Skipped { skip })?;
    remaining(deadline).map_err(|failure| Reply::Failure { failure })?;
    Ok((permit, evidence))
}

#[cfg(test)]
pub(super) fn assess<C: AdvisorClient>(
    request: &Request,
    deadline: Instant,
    client: &mut C,
) -> Reply {
    assess_with_context(request, deadline, client, acquire)
}

#[cfg(test)]
fn assess_with_context<C: AdvisorClient, A>(
    request: &Request,
    deadline: Instant,
    client: &mut C,
    acquire: A,
) -> Reply
where
    A: FnOnce(&Request, usize, Instant) -> ContextWindow,
{
    let (permit, evidence) = match prepare(request, deadline, acquire) {
        Ok(value) => value,
        Err(reply) => return reply,
    };
    assess_permit(&permit, evidence, deadline, client)
}

fn assess_permit<C: AdvisorClient>(
    permit: &guardian_advisor::SendPermit,
    evidence: guardian_advisor::ScopeEvidence,
    deadline: Instant,
    client: &mut C,
) -> Reply {
    let answer = remaining(deadline).and_then(|time| client.assess(permit, time));
    if let Err(failure) = remaining(deadline) {
        return Reply::Failure { failure };
    }
    match answer {
        Ok(answer) => Reply::Assessment {
            risk: answer.risk_distribution().clone(),
            scope: answer.scope_distribution().clone(),
            evidence,
        },
        Err(failure) => Reply::Failure { failure },
    }
}

pub fn run_if_child() {
    if std::env::var_os(WORKER_ENV).is_none() {
        return;
    }
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(run);
    std::process::exit(if matches!(result, Ok(Ok(()))) { 0 } else { 1 });
}

fn run() -> Result<(), Failure> {
    let nonce = decode_nonce(&std::env::var(WORKER_ENV).map_err(|_| Failure::InvalidResponse)?)?;
    let nanos: u64 = std::env::var(BUDGET_ENV)
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|n| *n > 0)
        .ok_or(Failure::InvalidResponse)?;
    let max_bytes: usize = std::env::var(LIMIT_ENV)
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or(Failure::InvalidResponse)?;
    u32::try_from(max_bytes).map_err(|_| Failure::InvalidResponse)?;
    let deadline = Instant::now()
        .checked_add(Duration::from_nanos(nanos))
        .ok_or(Failure::Timeout)?;
    // Only this startup dispatch owns fd 0; no public stdin consumer or argument handling has run.
    let mut socket = unsafe { UnixStream::from_raw_fd(0) };
    let work: Work = wire::read_frame(&mut socket, nonce, FrameKind::Request, max_bytes, deadline)?;
    let request = match work {
        Work::Evaluate(request) => request,
        Work::SkippedLog(request) => {
            let started = Instant::now();
            let config = log_config(&request.model, &request.log)?;
            let machine = verdict(&request.machine)?;
            let reply = Reply::Skipped { skip: request.skip };
            wire::write_reply(&mut socket, nonce, &reply, deadline)?;
            save_log(
                request.state_base.as_ref(),
                &config,
                machine,
                reply,
                None,
                started,
                deadline,
            );
            return Ok(());
        }
    };
    if wire::request_limit(request.settings.max_request_bytes)? != max_bytes {
        return Err(Failure::InvalidResponse);
    }
    if let Source::ClaudeEvent(hook) = &request.source {
        if request.settings.context_exchanges > 0 {
            if let (Some(base), Ok(now)) = (
                &request.state_base,
                SystemTime::now().duration_since(UNIX_EPOCH),
            ) {
                if let Ok(state) = AdvisorState::open(base, deadline) {
                    context::process_claude_event(
                        &state,
                        hook,
                        context::CacheLimits {
                            max_bytes,
                            exchanges: request.settings.context_exchanges,
                            ttl: request.settings.context_ttl_seconds,
                            now: now.as_secs(),
                            deadline,
                        },
                    );
                }
            }
        }
        return wire::write_reply(
            &mut socket,
            nonce,
            &Reply::Skipped {
                skip: Skip::NotApplicable,
            },
            deadline,
        );
    }
    let started = Instant::now();
    let log_config = request
        .log
        .as_ref()
        .map(|logging| log_config(&request.settings.model, logging))
        .transpose()?;
    let mut client = guardian_advisor_typesafe::TypeSafeClient::from_env();
    let mut candidate_text = None;
    let reply = match prepare(&request, deadline, acquire) {
        Ok((permit, evidence)) => {
            if log_config.as_ref().is_some_and(|c| c.debug_text) {
                candidate_text = std::str::from_utf8(permit.body()).ok().map(str::to_owned);
            }
            assess_permit(&permit, evidence, deadline, &mut client)
        }
        Err(reply) => reply,
    };
    wire::write_reply(&mut socket, nonce, &reply, deadline)?;
    if let Some(config) = log_config {
        let machine = verdict(&request.machine)?;
        save_log(
            request.state_base.as_ref(),
            &config,
            machine,
            reply,
            candidate_text.as_deref(),
            started,
            deadline,
        );
    }
    Ok(())
}

pub(super) fn decode_nonce(value: &str) -> Result<[u8; 16], Failure> {
    if value.len() != 32 || !value.is_ascii() {
        return Err(Failure::InvalidResponse);
    }
    let mut nonce = [0; 16];
    for (i, byte) in nonce.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)
            .map_err(|_| Failure::InvalidResponse)?;
    }
    Ok(nonce)
}

#[cfg(test)]
mod tests {
    use super::*;
    use guardian_advisor::{Assessment, RawDistribution, SendPermit};
    struct Client {
        calls: usize,
        delay: Duration,
        body: Option<serde_json::Value>,
    }
    impl AdvisorClient for Client {
        fn assess(&mut self, permit: &SendPermit, _: Duration) -> Result<Assessment, Failure> {
            self.calls += 1;
            self.body = Some(serde_json::from_slice(permit.body()).unwrap());
            std::thread::sleep(self.delay);
            Assessment::validate(
                RawDistribution {
                    selected: "major_destructive".into(),
                    probabilities: vec![
                        ("harmful_irreversible".into(), 0.01),
                        ("major_destructive".into(), 0.95),
                        ("irreversible_only".into(), 0.01),
                        ("no_harm".into(), 0.01),
                        ("unknown".into(), 0.02),
                    ],
                },
                RawDistribution {
                    selected: "matched".into(),
                    probabilities: vec![
                        ("matched".into(), 0.98),
                        ("mismatched".into(), 0.01),
                        ("unknown".into(), 0.01),
                    ],
                },
            )
        }
    }
    fn request() -> Request {
        Request {
            command: "printf fixture-safe".into(),
            cwd: "/fixture/work".into(),
            machine: "allow".into(),
            reasons: Vec::new(),
            source: Source::Codex(
                serde_json::json!({"hook_event_name":"PreToolUse","transcript_path":"/fixture/absent","session_id":"fixture-session","turn_id":"fixture-turn","tool_use_id":"fixture-call"}),
            ),
            state_base: None,
            log: None,
            settings: Settings {
                model: "fixture-model".into(),
                max_request_bytes: 65536,
                context_exchanges: 3,
                context_ttl_seconds: 86400,
            },
        }
    }

    // @kotowari[REQ-advisor-017, EX-advisor-034]
    #[test]
    fn acquisition_that_returns_after_deadline_cannot_start_the_model_even_as_missing_context() {
        let mut client = Client {
            calls: 0,
            delay: Duration::ZERO,
            body: None,
        };
        let started = Instant::now();
        let reply = assess_with_context(
            &request(),
            started + Duration::from_millis(20),
            &mut client,
            |_, _, _| {
                std::thread::sleep(Duration::from_millis(40));
                ContextWindow::default()
            },
        );
        assert_eq!(
            reply,
            Reply::Failure {
                failure: Failure::Timeout
            }
        );
        assert_eq!(client.calls, 0);
    }
    // @kotowari[REQ-advisor-006, REQ-advisor-017, REQ-advisor-003, EX-advisor-006]
    #[test]
    fn fast_missing_context_still_assesses_and_cannot_use_a_matched_answer_as_verified_scope() {
        let mut client = Client {
            calls: 0,
            delay: Duration::ZERO,
            body: None,
        };
        let reply = assess(
            &request(),
            Instant::now() + Duration::from_secs(1),
            &mut client,
        );
        assert_eq!(client.calls, 1);
        assert_eq!(
            client.body.unwrap()["state"]["context"]["source_verified"],
            false
        );
        let (answer, evidence) = reply.validated().unwrap().unwrap();
        assert_eq!(evidence, guardian_advisor::ScopeEvidence::Unavailable);
        assert_eq!(
            guardian_advisor::combine(
                guardian_advisor::Mode::Enforce,
                Verdict::Allow,
                Some(&answer),
                evidence,
                0.9
            )
            .final_verdict,
            Verdict::Block
        );
    }
    // @kotowari[REQ-advisor-017, REQ-advisor-009, EX-advisor-018]
    #[test]
    fn expired_budget_secret_or_large_body_never_enters_model_boundary_and_late_answer_is_not_adopted(
    ) {
        let mut client = Client {
            calls: 0,
            delay: Duration::ZERO,
            body: None,
        };
        assert_eq!(
            assess(&request(), Instant::now(), &mut client),
            Reply::Failure {
                failure: Failure::Timeout
            }
        );
        let mut secret = request();
        secret.command = "printf sk-fixtureabcdefghijklmnopqrstuvwxyz".into();
        assert_eq!(
            assess(
                &secret,
                Instant::now() + Duration::from_secs(1),
                &mut client
            ),
            Reply::Skipped { skip: Skip::Secret }
        );
        let mut large = request();
        large.command = "x".repeat(65536);
        assert_eq!(
            assess(&large, Instant::now() + Duration::from_secs(1), &mut client),
            Reply::Skipped { skip: Skip::Size }
        );
        assert_eq!(client.calls, 0);
        client.delay = Duration::from_millis(40);
        assert_eq!(
            assess(
                &request(),
                Instant::now() + Duration::from_millis(20),
                &mut client
            ),
            Reply::Failure {
                failure: Failure::Timeout
            }
        );
        assert_eq!(client.calls, 1);
    }
}
