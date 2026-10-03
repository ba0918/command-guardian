use guardian_app::advisor::{
    service::{self, Completion},
    worker::{Logging, Request, Settings, Source},
};
use guardian_app::Report;
use guardian_core::Verdict;
use guardian_policy::config::AdvisorConfig;
use std::path::Path;
use std::time::Instant;

pub fn run(
    config: &AdvisorConfig,
    report: &mut Report,
    command: &str,
    cwd: &Path,
    source: impl FnOnce() -> Source,
    finished: Instant,
) -> Option<Completion> {
    run_with(
        config,
        report,
        command,
        cwd,
        source,
        finished,
        service::start,
    )
}

fn run_with(
    config: &AdvisorConfig,
    report: &mut Report,
    command: &str,
    cwd: &Path,
    source: impl FnOnce() -> Source,
    finished: Instant,
    start: impl FnOnce(
        &Request,
        &AdvisorConfig,
        Verdict,
        Instant,
    ) -> (guardian_app::advisor::Evaluation, Completion),
) -> Option<Completion> {
    if config.mode.as_str() == "off" || report.verdict == Verdict::Block {
        return None;
    }
    let state_base = crate::log::shadow_log_path(
        crate::env_path("XDG_STATE_HOME").as_deref(),
        crate::env_path("HOME").as_deref(),
    )
    .and_then(|p| p.parent().and_then(|p| p.parent()).map(Path::to_path_buf));
    let Some(cwd) = cwd.to_str() else {
        return Some(service::encoding_skip(
            state_base,
            config,
            report.verdict,
            finished,
        ));
    };
    let request = Request {
        command: command.into(),
        cwd: cwd.into(),
        machine: report.verdict.as_str().into(),
        reasons: if report.message.is_empty() {
            Vec::new()
        } else {
            vec![report.message.clone()]
        },
        source: source(),
        state_base,
        settings: Settings {
            model: config.model.clone(),
            max_request_bytes: config.max_request_bytes,
            context_exchanges: config.context_exchanges,
            context_ttl_seconds: config.context_ttl_hours.checked_mul(3600)?,
        },
        log: Some(Logging {
            mode: config.mode.as_str().into(),
            threshold: config.intervention_threshold,
            debug_text: config.debug_text,
        }),
    };
    let (evaluation, completion) = start(&request, config, report.verdict, finished);
    if evaluation.outcome.final_verdict > report.verdict {
        report.verdict = evaluation.outcome.final_verdict;
        if let Some((answer, _)) = evaluation
            .reply
            .as_ref()
            .and_then(|reply| reply.validated().ok().flatten())
        {
            (report.reason, report.message) = guardian_policy::message::advisor_explanation(
                answer.risk(),
                report.verdict,
                &report.reason,
                &report.effects,
            );
        }
    }
    if evaluation.unreaped {
        report
            .warnings
            .push("Could not confirm advisor child termination.".into());
    }
    Some(completion)
}

pub fn finish(completion: Option<Completion>) {
    if let Some(completion) = completion {
        let warning = completion.finish();
        if warning.log_incomplete {
            crate::diagnostic(format_args!("Warning: Could not write advisor log."));
        }
        if warning.unreaped {
            crate::diagnostic(format_args!(
                "Warning: Could not confirm advisor child termination."
            ));
        }
    }
}

pub fn cache_event(config: &AdvisorConfig, hook: serde_json::Value) {
    if config.mode.as_str() == "off" || config.context_exchanges == 0 {
        return;
    }
    let Some(ttl) = config.context_ttl_hours.checked_mul(3600) else {
        return;
    };
    let state_base = crate::log::shadow_log_path(
        crate::env_path("XDG_STATE_HOME").as_deref(),
        crate::env_path("HOME").as_deref(),
    )
    .and_then(|p| p.parent().and_then(|p| p.parent()).map(Path::to_path_buf));
    let request = Request {
        command: String::new(),
        cwd: String::new(),
        machine: "allow".into(),
        reasons: Vec::new(),
        source: Source::ClaudeEvent(hook),
        state_base,
        log: None,
        settings: Settings {
            model: config.model.clone(),
            max_request_bytes: config.max_request_bytes,
            context_exchanges: config.context_exchanges,
            context_ttl_seconds: ttl,
        },
    };
    let (_, completion) = service::start(&request, config, Verdict::Allow, Instant::now());
    finish(Some(completion));
}

#[cfg(test)]
pub(super) mod operational_tests {
    use super::*;
    use std::os::fd::OwnedFd;
    use std::process::{Command, Stdio};
    const CHILD: &str = r#"import json,socket,sys
s=socket.socket(fileno=0)
data=b''
while True:
 b=s.recv(4096)
 if not b:break
 data+=b
assert len(data)>=22 and data[0]==1 and data[17]==1
assert len(data)==22+int.from_bytes(data[18:22],'big')
request=json.loads(data[22:])
assert request['machine'] in ['allow','ask'] and request['command'] in ['printf fixture-private','rm /fixture/work/notes.txt /fixture/work/other.txt']
risk=sys.argv[1];prob=float(sys.argv[2]);evidence=sys.argv[3]
labels=['harmful_irreversible','major_destructive','irreversible_only','no_harm','unknown']
body=json.dumps({'status':'assessment','risk':{'selected':risk,'probabilities':[[k,prob if k==risk else (1-prob)/4] for k in labels]},'scope':{'selected':'matched' if evidence=='confirmed' else 'unknown','probabilities':[['matched',.98 if evidence=='confirmed' else .01],['mismatched',.01],['unknown',.01 if evidence=='confirmed' else .98]]},'evidence':evidence}).encode()
s.sendall(bytes([1])+data[1:17]+bytes([2])+len(body).to_bytes(4,'big')+body)
s.shutdown(socket.SHUT_WR)
"#;

    // @kotowari[REQ-advisor-017, REQ-advisor-018, REQ-advisor-021, EX-advisor-034, EX-advisor-042]
    #[test]
    fn blocked_acquisition_in_child_is_killed_at_whole_deadline_and_native_cli_keeps_allow_without_model_query(
    ) {
        use std::time::Duration;
        let fixture = tempfile::tempdir().unwrap();
        let fifo = fixture.path().join("acquisition-fifo");
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            &fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )
        .unwrap();
        let started_file = fixture.path().join("acquisition-started");
        let model_file = fixture.path().join("model-query");
        let pid_file = fixture.path().join("child-pid");
        let child = r#"import socket,json,sys,os
s=socket.socket(fileno=0)
data=b''
while True:
 b=s.recv(4096)
 if not b:break
 data+=b
assert data[0]==1 and data[17]==1 and len(data)==22+int.from_bytes(data[18:22],'big')
request=json.loads(data[22:]);assert request['machine']=='allow'
open(sys.argv[4],'w').write(str(os.getpid()))
open(sys.argv[2],'w').write('acquisition started')
with open(sys.argv[1],'rb') as transcript: transcript.read()
open(sys.argv[3],'w').write('model query reached')
"#;
        let config = AdvisorConfig {
            mode: guardian_advisor::Mode::Enforce,
            timeout_ms: 2000,
            ..Default::default()
        };
        let mut report = Report {
            verdict: Verdict::Allow,
            effects: Vec::new(),
            rules: Vec::new(),
            warnings: Vec::new(),
            message: String::new(),
            reason: "fixture-machine-reason".into(),
            parse_errors: Vec::new(),
        };
        let finished = Instant::now();
        let completion = run_with(
            &config,
            &mut report,
            "printf fixture-private",
            Path::new("/fixture/private"),
            || Source::Cli,
            finished,
            |request, config, machine, finished| {
                let result =
                    service::start_with(request, config, machine, finished, |socket, _, _, _| {
                        Command::new("/usr/bin/python3")
                            .env_clear()
                            .arg("-c")
                            .arg(child)
                            .arg(&fifo)
                            .arg(&started_file)
                            .arg(&model_file)
                            .arg(&pid_file)
                            .stdin(Stdio::from(OwnedFd::from(socket)))
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .spawn()
                    });
                assert_eq!(result.0.failure, Some(guardian_advisor::Failure::Timeout));
                result
            },
        );
        assert!(started_file.exists());
        assert!(!model_file.exists());
        assert!(finished.elapsed() >= Duration::from_millis(1900));
        assert!(finished.elapsed() < Duration::from_millis(2500));
        assert_eq!(report.verdict, Verdict::Allow);
        assert_eq!(crate::check_exit(&report), 0);
        let mut output = Vec::new();
        crate::print_json_to(&report, &mut output).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output).unwrap()["verdict"],
            "allow"
        );
        let pid: u32 = std::fs::read_to_string(pid_file).unwrap().parse().unwrap();
        for _ in 0..100 {
            let status = std::fs::read_to_string(format!("/proc/{pid}/stat"));
            if match &status {
                Err(_) => true,
                Ok(text) => text.split_whitespace().nth(2) == Some("Z"),
            } {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let status = std::fs::read_to_string(format!("/proc/{pid}/stat"));
        assert!(match &status {
            Err(_) => true,
            Ok(text) => text.split_whitespace().nth(2) == Some("Z"),
        });
        assert!(completion.unwrap().finish().log_incomplete);
    }

    pub(crate) fn fixture_report(
        risk: &str,
        probability: &str,
        evidence: &str,
        mode: guardian_advisor::Mode,
    ) -> Report {
        fixture_report_from(
            risk,
            probability,
            evidence,
            mode,
            Report {
                verdict: Verdict::Allow,
                effects: Vec::new(),
                rules: Vec::new(),
                warnings: Vec::new(),
                message: String::new(),
                reason: "fixture-machine-reason".into(),
                parse_errors: Vec::new(),
            },
        )
    }

    fn fixture_report_from(
        risk: &str,
        probability: &str,
        evidence: &str,
        mode: guardian_advisor::Mode,
        mut report: Report,
    ) -> Report {
        let config = AdvisorConfig {
            mode,
            timeout_ms: 1000,
            ..Default::default()
        };
        let command = if report.effects.is_empty() {
            "printf fixture-private"
        } else {
            "rm /fixture/work/notes.txt /fixture/work/other.txt"
        };
        let completion = run_with(
            &config,
            &mut report,
            command,
            Path::new("/fixture/private"),
            || Source::Cli,
            Instant::now(),
            |request, config, machine, finished| {
                service::start_with(request, config, machine, finished, |socket, _, _, _| {
                    Command::new("/usr/bin/python3")
                        .env_clear()
                        .arg("-c")
                        .arg(CHILD)
                        .args([risk, probability, evidence])
                        .stdin(Stdio::from(OwnedFd::from(socket)))
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                })
            },
        );
        assert!(!completion.unwrap().finish().unreaped);
        report
    }

    pub(crate) fn fixture_known_deletion_report() -> Report {
        use guardian_core::{Class, Op, Target, Why};
        use guardian_policy::{Config, EffectReport, Policy};
        let effects = ["notes.txt", "other.txt"].map(|name| EffectReport {
            op: Op::Delete,
            target: Target::Path {
                path: Path::new("/fixture/work").join(name),
                dereference: false,
            },
            class: Class::Unknown,
            why: Why::Unmanaged,
            verdict: Verdict::Ask,
        });
        let mechanical = Policy::new(Config::builtin(None), Vec::new()).report(
            effects.to_vec(),
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(mechanical.verdict, Verdict::Ask);
        assert_eq!(mechanical.message.lines().count(), 4);
        eprintln!("mechanical known-effect message:\n{}", mechanical.message);
        let original_reason = mechanical.reason.clone();
        let original_effects = mechanical.effects.clone();
        let report = fixture_report_from(
            "major_destructive",
            "0.95",
            "unavailable",
            guardian_advisor::Mode::Enforce,
            mechanical,
        );
        assert_eq!(report.verdict, Verdict::Block);
        assert_eq!(report.effects, original_effects);
        assert!(report.reason.contains(&original_reason));
        assert!(report.reason.contains("major_destructive"));
        eprintln!("strengthened known-effect message:\n{}", report.message);
        report
    }

    // @kotowari[REQ-advisor-003, REQ-advisor-021, REQ-011, EX-advisor-006, EX-advisor-042]
    #[test]
    fn child_wire_reply_reaches_native_cli_json_and_exit_without_invented_machine_effects() {
        use guardian_advisor::Mode;
        for (risk, probability, evidence, mode, verdict, exit) in [
            (
                "major_destructive",
                "0.95",
                "unavailable",
                Mode::Enforce,
                Verdict::Block,
                2,
            ),
            (
                "major_destructive",
                "0.89",
                "unavailable",
                Mode::Enforce,
                Verdict::Allow,
                0,
            ),
            (
                "major_destructive",
                "0.95",
                "confirmed",
                Mode::Enforce,
                Verdict::Ask,
                1,
            ),
            (
                "harmful_irreversible",
                "0.95",
                "confirmed",
                Mode::Enforce,
                Verdict::Block,
                2,
            ),
            (
                "major_destructive",
                "0.95",
                "unavailable",
                Mode::Observe,
                Verdict::Allow,
                0,
            ),
        ] {
            let report = fixture_report(risk, probability, evidence, mode);
            assert_eq!(report.verdict, verdict);
            assert_eq!(crate::check_exit(&report), exit);
            let mut output = Vec::new();
            crate::print_json_to(&report, &mut output).unwrap();
            let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
            assert_eq!(json["verdict"], verdict.as_str());
            assert_eq!(json["effects"], serde_json::json!([]));
            if verdict == Verdict::Allow {
                assert_eq!(json["reason"], "fixture-machine-reason");
                assert!(report.message.is_empty());
            } else {
                assert!(json["reason"].as_str().unwrap().contains(risk));
                assert!(json["reason"]
                    .as_str()
                    .unwrap()
                    .contains("fixture-machine-reason"));
                assert!((2..=4).contains(&report.message.lines().count()));
                assert!(report.message.lines().all(|line| !line.is_empty()));
                assert!(report.message.contains(risk));
                eprintln!(
                    "native JSON: {json}\nnative text: Verdict: {} — {}",
                    report.verdict, report.message
                );
            }
            assert!(!String::from_utf8(output)
                .unwrap()
                .contains("fixture-private"));
        }
        for machine in [Verdict::Allow, Verdict::Ask] {
            let report = fixture_report_from(
                "major_destructive",
                "0.95",
                "unavailable",
                Mode::Enforce,
                Report {
                    verdict: machine,
                    effects: Vec::new(),
                    rules: Vec::new(),
                    warnings: Vec::new(),
                    message: if machine == Verdict::Ask {
                        "Cannot judge\nReason: syntax\nAlternative: none\nAdditional findings: 1"
                            .into()
                    } else {
                        String::new()
                    },
                    reason: if machine == Verdict::Ask {
                        "Cannot judge: command syntax could not be read".into()
                    } else {
                        String::new()
                    },
                    parse_errors: Vec::new(),
                },
            );
            assert_eq!(report.verdict, Verdict::Block);
            assert!((2..=4).contains(&report.message.lines().count()));
            assert!(report.reason.contains("major_destructive"));
            assert!(report.effects.is_empty());
            eprintln!(
                "{machine} -> block reason: {}\n{}",
                report.reason, report.message
            );
        }
        let known = fixture_known_deletion_report();
        let mut output = Vec::new();
        crate::print_json_to(&known, &mut output).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
        let human = json["message"].as_str().unwrap();
        assert!(human.contains("/fixture/work/notes.txt"));
        assert!(human.to_ascii_lowercase().contains("delete"));
        assert!((2..=4).contains(&human.lines().count()));
        assert!(json["reason"]
            .as_str()
            .unwrap()
            .contains("major_destructive"));
        assert_eq!(json["effects"][0]["path"], "/fixture/work/notes.txt");
        eprintln!("known-effect native JSON: {json}");
    }
}
