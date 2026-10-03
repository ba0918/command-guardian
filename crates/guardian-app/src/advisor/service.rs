use super::{
    wire::{self, FrameKind, Reply},
    worker::{self, Request},
};
use guardian_advisor::{Failure, ScopeEvidence, combine, eligible};
use guardian_core::Verdict;
use guardian_policy::config::AdvisorConfig;
use std::io::Read;
use std::net::Shutdown;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub use super::Evaluation;

pub struct Completion {
    child: Option<Child>,
    socket: Option<UnixStream>,
    deadline: Instant,
    log_incomplete: bool,
}

#[derive(Debug, Default)]
pub struct CompletionWarning {
    pub log_incomplete: bool,
    pub unreaped: bool,
}

impl Completion {
    pub fn finish(mut self) -> CompletionWarning {
        let Some(mut child) = self.child.take() else {
            return CompletionWarning {
                log_incomplete: self.log_incomplete,
                unreaped: false,
            };
        };
        let mut warning = CompletionWarning::default();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    warning.log_incomplete = !status.success();
                    if let Some(mut stderr) = child.stderr.take() {
                        use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
                        if fcntl_getfl(&stderr)
                            .and_then(|flags| fcntl_setfl(&stderr, flags | OFlags::NONBLOCK))
                            .is_ok()
                        {
                            let mut text = [0; 256];
                            warning.log_incomplete |= stderr.read(&mut text).is_ok_and(|n| n > 0);
                        } else {
                            warning.log_incomplete = true;
                        }
                    }
                    return warning;
                }
                Err(_) => {
                    warning.log_incomplete = true;
                    break;
                }
                Ok(None) if Instant::now() >= self.deadline => {
                    warning.log_incomplete = true;
                    break;
                }
                Ok(None) => std::thread::sleep(
                    Duration::from_millis(1)
                        .min(self.deadline.saturating_duration_since(Instant::now())),
                ),
            }
        }
        if let Some(socket) = &self.socket {
            warning.unreaped = reap(&mut child, socket, self.deadline);
        }
        warning
    }
}

impl Drop for Completion {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            if let Some(socket) = &self.socket {
                let _ = socket.shutdown(Shutdown::Both);
            }
            let _ = child.kill();
            let _ = child.try_wait();
        }
    }
}

pub fn start(
    request: &Request,
    config: &AdvisorConfig,
    machine: Verdict,
    finished: Instant,
) -> (Evaluation, Completion) {
    start_with(request, config, machine, finished, spawn)
}

pub fn encoding_skip(
    state_base: Option<std::path::PathBuf>,
    config: &AdvisorConfig,
    machine: Verdict,
    finished: Instant,
) -> Completion {
    let request = worker::SkippedLog {
        skip: guardian_advisor::Skip::Encoding,
        machine: machine.as_str().into(),
        state_base,
        model: config.model.clone(),
        log: worker::Logging {
            mode: config.mode.as_str().into(),
            threshold: config.intervention_threshold,
            debug_text: false,
        },
    };
    start_message(
        &request,
        config.max_request_bytes,
        true,
        config,
        machine,
        finished,
        spawn,
    )
    .1
}

pub fn advisory_deadline(finished: Instant, timeout_ms: u64) -> Option<Instant> {
    finished.checked_add(Duration::from_millis(timeout_ms))
}

pub fn advise(
    request: &Request,
    config: &AdvisorConfig,
    machine: Verdict,
    finished: Instant,
) -> Evaluation {
    advise_with(request, config, machine, finished, spawn)
}

fn spawn(
    socket: UnixStream,
    nonce: [u8; 16],
    max_bytes: usize,
    remaining: Duration,
) -> std::io::Result<Child> {
    let nanos = u64::try_from(remaining.as_nanos())
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let nonce = nonce.iter().map(|b| format!("{b:02x}")).collect::<String>();
    Command::new(std::env::current_exe()?)
        .env(worker::WORKER_ENV, nonce)
        .env(worker::BUDGET_ENV, nanos.to_string())
        .env(worker::LIMIT_ENV, max_bytes.to_string())
        .env_remove("COMMAND_GUARDIAN_PARSER_WORKER")
        .stdin(Stdio::from(OwnedFd::from(socket)))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
}

fn reap(child: &mut Child, socket: &UnixStream, deadline: Instant) -> bool {
    let _ = socket.shutdown(Shutdown::Both);
    let _ = child.kill();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return false,
            Err(_) => return true,
            Ok(None) if Instant::now() >= deadline => return true,
            Ok(None) => std::thread::sleep(
                Duration::from_millis(1).min(deadline.saturating_duration_since(Instant::now())),
            ),
        }
    }
}

fn advise_with<S>(
    request: &Request,
    config: &AdvisorConfig,
    machine: Verdict,
    finished: Instant,
    spawn: S,
) -> Evaluation
where
    S: FnOnce(UnixStream, [u8; 16], usize, Duration) -> std::io::Result<Child>,
{
    let (mut evaluation, completion) = start_with(request, config, machine, finished, spawn);
    let warning = completion.finish();
    evaluation.unreaped |= warning.unreaped;
    evaluation
}

pub fn start_with<S>(
    request: &Request,
    config: &AdvisorConfig,
    machine: Verdict,
    finished: Instant,
    spawn: S,
) -> (Evaluation, Completion)
where
    S: FnOnce(UnixStream, [u8; 16], usize, Duration) -> std::io::Result<Child>,
{
    start_message(
        request,
        request.settings.max_request_bytes,
        request.log.is_some(),
        config,
        machine,
        finished,
        spawn,
    )
}

fn start_message<S>(
    request: &impl serde::Serialize,
    max_request_bytes: usize,
    logging: bool,
    config: &AdvisorConfig,
    machine: Verdict,
    finished: Instant,
    spawn: S,
) -> (Evaluation, Completion)
where
    S: FnOnce(UnixStream, [u8; 16], usize, Duration) -> std::io::Result<Child>,
{
    let mut completion = Completion {
        child: None,
        socket: None,
        deadline: advisory_deadline(finished, config.timeout_ms).unwrap_or(finished),
        log_incomplete: false,
    };
    let mut evaluation = Evaluation {
        outcome: combine(
            config.mode,
            machine,
            None,
            ScopeEvidence::Unavailable,
            config.intervention_threshold,
        ),
        reply: None,
        failure: None,
        unreaped: false,
        elapsed_ms: 0,
    };
    if !eligible(config.mode, machine) {
        return (evaluation, completion);
    }
    let result = || -> Result<Reply, Failure> {
        let deadline = advisory_deadline(finished, config.timeout_ms).ok_or(Failure::Timeout)?;
        worker::remaining(deadline)?;
        let max_bytes = wire::request_limit(max_request_bytes)?;
        let mut nonce = [0; 16];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut nonce))
            .map_err(|_| Failure::Communication)?;
        worker::remaining(deadline)?;
        let (mut socket, child_socket) = UnixStream::pair().map_err(|_| Failure::Communication)?;
        let mut child = spawn(child_socket, nonce, max_bytes, worker::remaining(deadline)?)
            .map_err(|_| Failure::Communication)?;
        let reply = wire::write_frame(
            &mut socket,
            nonce,
            FrameKind::Request,
            request,
            max_bytes,
            deadline,
        )
        .and_then(|()| wire::read_reply(&mut socket, nonce, deadline))
        .and_then(|reply| {
            worker::remaining(deadline)?;
            if !matches!(reply, Reply::Failure { .. }) {
                reply.validated()?;
            }
            Ok(reply)
        });
        match reply {
            Ok(reply) => {
                completion.child = Some(child);
                completion.socket = Some(socket);
                Ok(reply)
            }
            Err(failure) => {
                evaluation.unreaped = reap(&mut child, &socket, deadline);
                Err(failure)
            }
        }
    };
    match result() {
        Ok(reply) => {
            match reply.validated() {
                Ok(Some((answer, evidence))) => {
                    evaluation.outcome = combine(
                        config.mode,
                        machine,
                        Some(&answer),
                        evidence,
                        config.intervention_threshold,
                    )
                }
                Ok(None) => (),
                Err(failure) => evaluation.failure = Some(failure),
            }
            evaluation.reply = Some(reply);
        }
        Err(failure) => {
            evaluation.failure = Some(failure);
            completion.log_incomplete = logging;
        }
    }
    evaluation.elapsed_ms = finished.elapsed().as_millis();
    (evaluation, completion)
}

#[cfg(test)]
mod tests {
    use super::super::worker::{Request, Settings, Source};
    use super::*;
    use guardian_advisor::{Mode, ScopeEvidence};
    use guardian_core::Verdict;
    use guardian_policy::config::AdvisorConfig;
    use std::os::fd::OwnedFd;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    fn request() -> Request {
        Request {
            command: "printf fixture-safe".into(),
            cwd: "/fixture/work".into(),
            machine: "allow".into(),
            reasons: Vec::new(),
            source: Source::Cli,
            state_base: None,
            log: None,
            settings: Settings {
                model: "fixture-model".into(),
                max_request_bytes: 65536,
                context_exchanges: 0,
                context_ttl_seconds: 86400,
            },
        }
    }

    // @kotowari[REQ-advisor-011, REQ-advisor-018, REQ-advisor-021]
    #[test]
    fn startup_failure_preserves_machine_verdict_and_reports_that_its_log_could_not_be_written() {
        let mut request = request();
        request.log = Some(super::super::worker::Logging {
            mode: "enforce".into(),
            threshold: 0.9,
            debug_text: false,
        });
        let (evaluation, completion) = start_with(
            &request,
            &config(Mode::Enforce, 500),
            Verdict::Allow,
            Instant::now(),
            |_, _, _, _| Err(std::io::Error::other("fixture startup failure")),
        );
        assert_eq!(evaluation.failure, Some(Failure::Communication));
        assert_eq!(evaluation.outcome.final_verdict, Verdict::Allow);
        assert!(completion.finish().log_incomplete);
    }
    fn config(mode: Mode, timeout_ms: u64) -> AdvisorConfig {
        AdvisorConfig {
            mode,
            timeout_ms,
            ..Default::default()
        }
    }
    const CHILD: &str = r#"import json,socket,sys,time,os
s=socket.socket(fileno=0)
data=b''
while True:
 b=s.recv(4096)
 if not b:break
 data+=b
mode=sys.argv[1]
if mode=='hang':
 while True:time.sleep(1)
if mode=='die':os._exit(1)
if mode=='bad':s.sendall(b'bad');os._exit(0)
if mode=='late':time.sleep(0.15)
body=json.dumps({'status':'assessment','risk':{'selected':'major_destructive','probabilities':[['harmful_irreversible',0.01],['major_destructive',0.95],['irreversible_only',0.01],['no_harm',0.01],['unknown',0.02]]},'scope':{'selected':'unknown','probabilities':[['matched',0.01],['mismatched',0.01],['unknown',0.98]]},'evidence':'unavailable'}).encode()
s.sendall(bytes([1])+data[1:17]+bytes([2])+len(body).to_bytes(4,'big')+body)
s.shutdown(socket.SHUT_WR)
if mode=='loghang':
 while True:time.sleep(1)
"#;

    // @kotowari[REQ-advisor-011, REQ-advisor-018]
    #[test]
    fn completed_advice_is_available_before_a_stopped_log_writer_is_reaped_and_never_changes_afterward()
     {
        let started = Instant::now();
        let (evaluation, completion) = start_with(
            &request(),
            &config(Mode::Enforce, 500),
            Verdict::Allow,
            started,
            |socket, _, _, _| {
                Command::new("/usr/bin/python3")
                    .env_clear()
                    .arg("-c")
                    .arg(CHILD)
                    .arg("loghang")
                    .stdin(Stdio::from(OwnedFd::from(socket)))
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
            },
        );
        assert_eq!(evaluation.outcome.final_verdict, Verdict::Block);
        assert!(started.elapsed() < Duration::from_millis(400));
        let warning = completion.finish();
        assert!(warning.log_incomplete);
        assert_eq!(evaluation.outcome.final_verdict, Verdict::Block);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    // @kotowari[REQ-advisor-018, REQ-advisor-003, EX-advisor-035, EX-advisor-006]
    #[test]
    fn valid_child_is_composed_only_by_parent_and_observe_preserves_machine() {
        for mode in [Mode::Enforce, Mode::Observe] {
            let result = advise_with(
                &request(),
                &config(mode, 1000),
                Verdict::Allow,
                Instant::now(),
                |socket, _, _, _| {
                    Command::new("/usr/bin/python3")
                        .env_clear()
                        .arg("-c")
                        .arg(CHILD)
                        .arg("valid")
                        .stdin(Stdio::from(OwnedFd::from(socket)))
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                },
            );
            assert_eq!(result.outcome.candidate, Verdict::Block);
            assert_eq!(
                result.outcome.final_verdict,
                if mode == Mode::Enforce {
                    Verdict::Block
                } else {
                    Verdict::Allow
                }
            );
            let (_, evidence) = result.reply.as_ref().unwrap().validated().unwrap().unwrap();
            assert_eq!(evidence, ScopeEvidence::Unavailable);
        }
    }

    // @kotowari[REQ-advisor-017, REQ-advisor-018, EX-advisor-036]
    #[test]
    fn stopped_dead_invalid_or_unstartable_child_preserves_machine_without_unbounded_wait() {
        for mode in ["hang", "die", "bad", "late"] {
            let started = Instant::now();
            let result = advise_with(
                &request(),
                &config(Mode::Enforce, 40),
                Verdict::Ask,
                started,
                |socket, _, _, _| {
                    Command::new("/usr/bin/python3")
                        .env_clear()
                        .arg("-c")
                        .arg(CHILD)
                        .arg(mode)
                        .stdin(Stdio::from(OwnedFd::from(socket)))
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                },
            );
            assert_eq!(result.outcome.final_verdict, Verdict::Ask);
            assert!(result.failure.is_some());
            assert!(started.elapsed() < Duration::from_secs(1));
        }
        let result = advise_with(
            &request(),
            &config(Mode::Enforce, 40),
            Verdict::Allow,
            Instant::now(),
            |_, _, _, _| Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
        );
        assert_eq!(result.outcome.final_verdict, Verdict::Allow);
    }

    // @kotowari[REQ-advisor-017, EX-advisor-033]
    #[test]
    fn advisory_deadline_starts_at_machine_finish_not_the_original_mechanical_start() {
        let finished = Instant::now();
        let machine_started = finished - Duration::from_secs(4);
        let deadline = advisory_deadline(finished, 2000).unwrap();
        assert_eq!(deadline.duration_since(finished), Duration::from_secs(2));
        assert_eq!(
            deadline.duration_since(machine_started),
            Duration::from_secs(6)
        );
    }

    // @kotowari[REQ-advisor-001, EX-advisor-002]
    #[test]
    fn off_and_machine_block_never_launch_a_child_or_acquire_context() {
        for (mode, machine) in [(Mode::Off, Verdict::Allow), (Mode::Enforce, Verdict::Block)] {
            let result = advise_with(
                &request(),
                &config(mode, 40),
                machine,
                Instant::now(),
                |_, _, _, _| panic!("no worker may be launched"),
            );
            assert_eq!(result.outcome.final_verdict, machine);
            assert!(result.reply.is_none());
        }
    }
}
