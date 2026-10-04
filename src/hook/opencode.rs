use serde_json::{Value, json};
use std::path::PathBuf;

struct Input {
    command: String,
    cwd: PathBuf,
    shell: PathBuf,
}

fn parse_input(text: &str) -> Option<Input> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let command = value.get("command")?.as_str()?.to_string();
    let cwd = PathBuf::from(value.get("cwd")?.as_str()?);
    let shell = PathBuf::from(value.get("shell")?.as_str()?);
    if command.is_empty() || !cwd.is_absolute() || !shell.is_absolute() {
        return None;
    }
    Some(Input {
        command,
        cwd,
        shell,
    })
}

pub(super) fn unavailable(reason: &str, enforce: Option<bool>) {
    let mut response = json!({"status": "unavailable", "reason": reason});
    if let Some(enforce) = enforce {
        response["mode"] = json!({"enforce": enforce});
    }
    let _ = crate::output(format_args!("{response}"));
}

pub(super) fn run(text: &str) -> i32 {
    let input = match parse_input(text) {
        Some(input) => input,
        _ => {
            unavailable("Invalid OpenCode hook input.", None);
            return 0;
        }
    };
    let engine = match crate::build_engine(input.cwd.clone()) {
        Ok(engine) => engine,
        Err(_) => {
            unavailable("Could not load guardian configuration.", None);
            return 0;
        }
    };
    let enforce = engine.config().enforce;
    if input.shell.file_name().is_none_or(|name| name != "bash") {
        let reason = "Unsupported shell: guardian supports Bash only.";
        if enforce {
            unavailable(reason, Some(true));
        } else {
            crate::diagnostic(format_args!("Warning: {reason}"));
            let response =
                json!({"status": "shadow", "mode": {"enforce": false}, "reason": reason});
            let _ = crate::output(format_args!("{response}"));
        }
        return 0;
    }
    let mut control = if engine.config().advisor.mode.as_str() != "off" {
        guardian_app::advisor::control::Control::inherited()
            .and_then(|mut c| c.probe(engine.config().advisor.mode).then_some(c))
    } else {
        None
    };
    let mut report = engine.check(&input.command);
    let finished = std::time::Instant::now();
    let approved = control.as_mut().and_then(|c| {
        c.start(
            engine.config().advisor.mode,
            report.verdict,
            finished,
            engine.config().advisor.timeout_ms,
        )
    });
    let completion = if approved.is_some() {
        crate::advisor::run(
            &engine.config().advisor,
            &mut report,
            &input.command,
            &input.cwd,
            || guardian_app::advisor::worker::Source::OpenCode,
            finished,
        )
    } else {
        None
    };
    drop(control);
    for warning in &report.warnings {
        crate::diagnostic(format_args!("Warning: {warning}"));
    }
    if !enforce && crate::log::write_shadow(&report, &input.command).is_err() {
        crate::diagnostic(format_args!("Warning: Could not write shadow log."));
    }
    let response = if enforce && super::defers(report.verdict, engine.config().defer_ask) {
        let warning = crate::log::write_deferred(&report, &input.command)
            .is_err()
            .then_some("Could not write shadow log.");
        if let Some(warning) = warning {
            crate::diagnostic(format_args!("Warning: {warning}"));
        }
        deferred_response(&report, warning)
    } else {
        native_response(&report, enforce)
    };
    let _ = crate::output(format_args!("{response}"));
    crate::advisor::finish(completion);
    0
}

fn native_response(report: &guardian_app::Report, enforce: bool) -> Value {
    if enforce {
        json!({"status": "judged", "mode": {"enforce": true}, "verdict": report.verdict.as_str(), "reason": report.message})
    } else {
        json!({"status": "shadow", "mode": {"enforce": false}, "reason": "Guardian shadow mode."})
    }
}

/// 委任の応答。judged の "verdict" を持たせず、allow と取り違えない別の "status" にする。
fn deferred_response(report: &guardian_app::Report, warning: Option<&str>) -> Value {
    let mut response =
        json!({"status": "deferred", "mode": {"enforce": true}, "reason": report.message});
    if let Some(warning) = warning {
        response["warning"] = json!(warning);
    }
    response
}

#[cfg(test)]
mod advisor_output_tests {
    use super::*;
    use guardian_advisor::Mode;

    // @kotowari[REQ-advisor-021, REQ-011, REQ-048, EX-advisor-006, EX-advisor-051, EX-advisor-052]
    #[test]
    fn child_ipc_high_risk_is_judged_block_not_unavailable_and_shadow_is_not_reenabled() {
        let report = crate::advisor::operational_tests::fixture_report(
            "major_destructive",
            "0.95",
            "unavailable",
            Mode::Enforce,
        );
        let output = native_response(&report, true);
        assert_eq!(output["status"], "judged");
        assert_eq!(output["verdict"], "block");
        assert_eq!(output["mode"]["enforce"], true);
        let shadow = native_response(&report, false);
        assert_eq!(shadow["status"], "shadow");
        assert_eq!(shadow["mode"]["enforce"], false);
        assert!(shadow.get("verdict").is_none());
        let ask = crate::advisor::operational_tests::fixture_report(
            "major_destructive",
            "0.95",
            "confirmed",
            Mode::Enforce,
        );
        assert_eq!(native_response(&ask, true)["verdict"], "ask");
        let observed = crate::advisor::operational_tests::fixture_report(
            "harmful_irreversible",
            "0.95",
            "confirmed",
            Mode::Observe,
        );
        assert_eq!(native_response(&observed, true)["verdict"], "allow");
        let known = crate::advisor::operational_tests::fixture_known_deletion_report();
        let output = native_response(&known, true);
        let reason = output["reason"].as_str().unwrap();
        assert!(reason.contains("/fixture/work/notes.txt"));
        assert!(reason.to_ascii_lowercase().contains("delete"));
        assert!((2..=4).contains(&reason.lines().count()));
        assert_eq!(reason, known.message);
    }
}
