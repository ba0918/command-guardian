use guardian_core::Verdict;
use serde_json::json;
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
    let engine = match crate::build_engine(input.cwd) {
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
    let report = engine.check(&input.command);
    for warning in &report.warnings {
        crate::diagnostic(format_args!("Warning: {warning}"));
    }
    let response = if enforce {
        let verdict = match report.verdict {
            Verdict::Allow => "allow",
            Verdict::Ask => "ask",
            Verdict::Block => "block",
        };
        json!({"status": "judged", "mode": {"enforce": true}, "verdict": verdict, "reason": report.message})
    } else {
        if crate::log::write_shadow(&report, &input.command).is_err() {
            crate::diagnostic(format_args!("Warning: Could not write shadow log."));
        }
        json!({"status": "shadow", "mode": {"enforce": false}, "reason": "Guardian shadow mode."})
    };
    let _ = crate::output(format_args!("{response}"));
    0
}
