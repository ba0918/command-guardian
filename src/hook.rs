//! フックの口。Claude Code と Codex への入出力の写像（REQ-016, REQ-022〜REQ-024）。

use crate::build_engine;
use guardian_core::Verdict;
use std::io::Read;
use std::path::PathBuf;

mod opencode;

/// 呼び出し元のエージェント。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Agent {
    Claude,
    Codex,
    OpenCode,
}

/// フックの入力から取り出した判定の材料。
struct HookInput {
    command: String,
    cwd: PathBuf,
    permission_mode: Option<String>,
}

/// stdin のフック入力を判定し、エージェントごとの出力を返す。`hook` は常に 0 で終わる。
pub fn run(args: &[std::ffi::OsString]) -> i32 {
    let agent = match parse_agent(args) {
        HookArgs::Help => {
            let _ = crate::output(format_args!("{HELP}"));
            return 0;
        }
        HookArgs::Agent(agent) => agent,
    };
    let mut text = String::new();
    if std::io::stdin().read_to_string(&mut text).is_err() {
        if agent == Some(Agent::OpenCode) {
            opencode::unavailable("Could not read hook input.", None);
        }
        return 0;
    }
    if agent == Some(Agent::OpenCode) {
        return opencode::run(&text);
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
        && value
            .get("hook_event_name")
            .is_some_and(|event| event != "PreToolUse")
    {
        if agent == Some(Agent::Claude)
            && matches!(
                value["hook_event_name"].as_str(),
                Some("UserPromptSubmit" | "MessageDisplay")
            )
        {
            let cwd = value["cwd"]
                .as_str()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/"));
            if let Ok(engine) = build_engine(cwd)
                && engine.config().advisor.mode.as_str() != "off"
                && let guardian_app::advisor::worker::Source::Claude(hook) =
                    guardian_app::advisor::worker::Source::from_hook(true, &text)
            {
                crate::advisor::cache_event(&engine.config().advisor, hook);
            }
        }
        return 0;
    }
    let (Some(agent), Some(input)) = (agent, parse_input(&text)) else {
        return 0;
    };
    let engine = match build_engine(input.cwd.clone()) {
        Ok(engine) => engine,
        Err(_) => return 0,
    };
    let mut report = engine.check(&input.command);
    let finished = std::time::Instant::now();
    let completion = crate::advisor::run(
        &engine.config().advisor,
        &mut report,
        &input.command,
        &input.cwd,
        || guardian_app::advisor::worker::Source::from_hook(agent == Agent::Claude, &text),
        finished,
    );
    for w in &report.warnings {
        crate::diagnostic(format_args!("Warning: {w}"));
    }
    // 影実行ではフックとして何も返さず、判定をログに残す（REQ-018）。
    if !engine.config().enforce {
        if crate::log::write_shadow(&report, &input.command).is_err() {
            crate::diagnostic(format_args!("Warning: Could not write shadow log."));
        }
        crate::advisor::finish(completion);
        return 0;
    }
    if let Some(output) = response(agent, &report, input.permission_mode.as_deref()) {
        let _ = crate::output(format_args!("{output}"));
    }
    crate::advisor::finish(completion);
    0
}

fn response(
    agent: Agent,
    report: &guardian_app::Report,
    permission_mode: Option<&str>,
) -> Option<serde_json::Value> {
    let decision = decision(agent, report.verdict, permission_mode)?;
    Some(serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": decision,
            "permissionDecisionReason": report.message,
        }
    }))
}

const HELP: &str = "Judge a Bash request from an agent hook; read the JSON request from stdin.

Usage: command-guardian hook --agent claude|codex|opencode

Arguments:
  No positional arguments are required. Hook input is supplied on stdin.

Options:
  --agent claude|codex|opencode  Select the calling agent's protocol.
  --help, -h           Show help without reading stdin or loading configuration.

Examples:
  command-guardian hook --agent claude < request.json
  command-guardian hook --agent codex < request.json
  command-guardian hook --agent opencode < request.json

Exit codes:
  always 0, regardless of the verdict. Help also exits with 0.";

enum HookArgs {
    Help,
    Agent(Option<Agent>),
}

/// `--agent claude|codex` を読む。
fn parse_agent(args: &[std::ffi::OsString]) -> HookArgs {
    let mut agent = None;
    let mut i = 0;
    while i < args.len() {
        if matches!(args[i].to_str(), Some("--help" | "-h")) {
            return HookArgs::Help;
        } else if args[i] == "--agent" {
            agent = match args.get(i + 1).and_then(|s| s.to_str()) {
                Some("claude") => Some(Agent::Claude),
                Some("codex") => Some(Agent::Codex),
                Some("opencode") => Some(Agent::OpenCode),
                _ => None,
            };
            i += 2;
        } else {
            i += 1;
        }
    }
    HookArgs::Agent(agent)
}

/// Bash のコマンドを含む入力だけを取り出す（REQ-024）。
fn parse_input(text: &str) -> Option<HookInput> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    if value.get("tool_name")?.as_str()? != "Bash" {
        return None;
    }
    let command = value
        .get("tool_input")?
        .get("command")?
        .as_str()?
        .to_string();
    if command.is_empty() {
        return None;
    }
    let cwd = value
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"));
    let permission_mode = value
        .get("permission_mode")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Some(HookInput {
        command,
        cwd,
        permission_mode,
    })
}

/// エージェントごとの出口。`None` は何も返さない。
fn decision(agent: Agent, verdict: Verdict, permission_mode: Option<&str>) -> Option<&'static str> {
    match agent {
        Agent::Claude => match verdict {
            Verdict::Allow => None,
            Verdict::Ask => match permission_mode {
                Some("dontAsk") | Some("bypassPermissions") => None,
                _ => Some("ask"),
            },
            Verdict::Block => Some("deny"),
        },
        Agent::Codex => match verdict {
            Verdict::Block => Some("deny"),
            Verdict::Allow | Verdict::Ask => None,
        },
        Agent::OpenCode => None,
    }
}

#[cfg(test)]
mod advisor_output_tests {
    use super::*;
    use guardian_advisor::{Assessment, Mode, RawDistribution, ScopeEvidence, combine};

    // @kotowari[REQ-advisor-021, REQ-011, REQ-023, REQ-022, EX-advisor-041]
    #[test]
    fn codex_ask_from_child_ipc_has_no_native_output_but_independent_harm_is_deny() {
        let ask = crate::advisor::operational_tests::fixture_report(
            "major_destructive",
            "0.95",
            "confirmed",
            Mode::Enforce,
        );
        assert_eq!(ask.verdict, Verdict::Ask);
        assert!(response(Agent::Codex, &ask, None).is_none());
        assert!(response(Agent::Claude, &ask, Some("dontAsk")).is_none());
        assert_eq!(
            response(Agent::Claude, &ask, None).unwrap()["hookSpecificOutput"]["permissionDecision"],
            "ask"
        );
        assert_eq!(
            response(Agent::Claude, &ask, None).unwrap()["hookSpecificOutput"]["permissionDecisionReason"],
            ask.message
        );
        let block = crate::advisor::operational_tests::fixture_report(
            "harmful_irreversible",
            "0.95",
            "confirmed",
            Mode::Enforce,
        );
        assert_eq!(block.verdict, Verdict::Block);
        assert_eq!(
            response(Agent::Codex, &block, None).unwrap()["hookSpecificOutput"]["permissionDecision"],
            "deny"
        );
        assert_eq!(
            response(Agent::Codex, &block, None).unwrap()["hookSpecificOutput"]["permissionDecisionReason"],
            block.message
        );
        let known = crate::advisor::operational_tests::fixture_known_deletion_report();
        for agent in [Agent::Claude, Agent::Codex] {
            let output = response(agent, &known, None).unwrap();
            let reason = output["hookSpecificOutput"]["permissionDecisionReason"]
                .as_str()
                .unwrap();
            assert!(reason.contains("/fixture/work/notes.txt"));
            assert!(reason.to_ascii_lowercase().contains("delete"));
            assert!((2..=4).contains(&reason.lines().count()));
            assert_eq!(reason, known.message);
        }
    }

    // @kotowari[REQ-advisor-003, REQ-022, REQ-023]
    #[test]
    fn validated_major_destructive_matched_advice_becomes_ask_but_codex_still_has_no_output_decision()
     {
        let answer = Assessment::validate(
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
        .unwrap();
        let result = combine(
            Mode::Enforce,
            Verdict::Allow,
            Some(&answer),
            ScopeEvidence::Confirmed,
            0.9,
        );
        assert_eq!(result.final_verdict, Verdict::Ask);
        assert_eq!(decision(Agent::Codex, result.final_verdict, None), None);
        assert_eq!(
            decision(Agent::Claude, result.final_verdict, Some("dontAsk")),
            None
        );
        assert_eq!(
            decision(Agent::Claude, result.final_verdict, None),
            Some("ask")
        );
    }
}
