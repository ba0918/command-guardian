//! フックの口。Claude Code と Codex への入出力の写像（REQ-016, REQ-022〜REQ-024）。

use crate::build_engine;
use guardian_core::Verdict;
use std::io::Read;
use std::path::PathBuf;

/// 呼び出し元のエージェント。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Agent {
    Claude,
    Codex,
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
        return 0;
    }
    let (Some(agent), Some(input)) = (agent, parse_input(&text)) else {
        return 0;
    };
    let engine = match build_engine(input.cwd) {
        Ok(engine) => engine,
        Err(_) => return 0,
    };
    let report = engine.check(&input.command);
    for w in &report.warnings {
        crate::diagnostic(format_args!("Warning: {w}"));
    }
    // 影実行ではフックとして何も返さず、判定をログに残す（REQ-018）。
    if !engine.config().enforce {
        let _ = crate::log::write_shadow(&report, &input.command);
        return 0;
    }
    if let Some(decision) = decision(agent, report.verdict, input.permission_mode.as_deref()) {
        let output = serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": decision,
                "permissionDecisionReason": report.message,
            }
        });
        let _ = crate::output(format_args!("{output}"));
    }
    0
}

const HELP: &str = "Judge a Bash request from an agent hook; read the JSON request from stdin.

Usage: command-guardian hook --agent claude|codex

Arguments:
  No positional arguments are required. Hook input is supplied on stdin.

Options:
  --agent claude|codex  Select the calling agent's protocol.
  --help, -h           Show help without reading stdin or loading configuration.

Examples:
  command-guardian hook --agent claude < request.json
  command-guardian hook --agent codex < request.json

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
    }
}
