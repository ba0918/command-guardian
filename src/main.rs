//! command-guardian の実行ファイル。`check` と `hook` の 2 つのコマンドを持つ。

mod hook;
mod log;

use guardian_app::{Engine, EngineEnv, Report};
use guardian_core::Verdict;
use guardian_policy::message;
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    // 入力由来の解析の子プロセスとして起動されたときは、何よりも先に解析を務める。
    // 子は stdin を要求のソケットとして使うため、フックの入力より先にここへ来る。
    guardian_app::runtime::run_if_child();
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    std::process::exit(run(&args));
}

fn run(args: &[OsString]) -> i32 {
    match args.first().and_then(|s| s.to_str()) {
        Some("--help" | "-h") => {
            let _ = output(format_args!("{ROOT_HELP}"));
            0
        }
        Some("check") => cmd_check(&args[1..]),
        Some("hook") => hook::run(&args[1..]),
        Some(other) => {
            diagnostic(format_args!("Unknown command: {other}"));
            diagnostic(format_args!("Usage: command-guardian <check|hook> ..."));
            3
        }
        None => {
            diagnostic(format_args!("Usage: command-guardian <check|hook> ..."));
            3
        }
    }
}

const ROOT_HELP: &str = "Check Bash commands for destructive effects before execution.

Usage: command-guardian <check|hook> ...

Commands:
  check  Judge a command string (use check --help for arguments and options).
  hook   Read an agent hook request from stdin (use hook --help for options).

Options:
  --help, -h  Show this help without reading configuration or stdin.

Examples:
  command-guardian check 'rm -rf /etc/nginx' --cwd /tmp
  command-guardian hook --agent claude < request.json

Exit codes:
  check: 0 allow, 1 ask, 2 block, 3 unable to produce a judgment.
  hook: always 0. Help: 0. Invalid invocation: 3.";

const CHECK_HELP: &str = "Judge a Bash command without executing it.

Usage: command-guardian check [OPTIONS] <COMMAND>
       command-guardian check [OPTIONS] -- <COMMAND>

Arguments:
  COMMAND  One command string; quote it to keep shell syntax intact.

Options:
  --cwd <PATH>        Working directory (default: current working directory).
  --format text|json  Output format (default: text).
  --help, -h          Show help without loading configuration or judging a command.
  --                 Treat all following arguments as command strings, not options.

Examples:
  command-guardian check 'rm -rf /etc/nginx' --cwd /tmp --format json
  command-guardian check -- '--help'

Exit codes:
  0 allow, 1 ask, 2 block, 3 unable to produce a judgment. Help: 0.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Text,
    Json,
}

struct CheckArgs {
    command: String,
    cwd: PathBuf,
    format: Format,
}

fn cmd_check(args: &[OsString]) -> i32 {
    let parsed = match parse_check(args) {
        Ok(Some(p)) => p,
        Ok(None) => {
            let _ = output(format_args!("{CHECK_HELP}"));
            return 0;
        }
        Err(e) => {
            diagnostic(format_args!("{e}"));
            return 3;
        }
    };
    let engine = match build_engine(parsed.cwd.clone()) {
        Ok(e) => e,
        Err(code) => return code,
    };
    let report = engine.check(&parsed.command);
    for w in &report.warnings {
        diagnostic(format_args!("Warning: {w}"));
    }
    let result = match parsed.format {
        Format::Text => print_text(&report),
        Format::Json => print_json(&report),
    };
    if result.is_err() {
        return 3;
    }
    match report.verdict {
        Verdict::Allow => 0,
        Verdict::Ask => 1,
        Verdict::Block => 2,
    }
}

fn parse_check(args: &[OsString]) -> Result<Option<CheckArgs>, String> {
    let mut command: Option<String> = None;
    let mut cwd: Option<PathBuf> = None;
    let mut format = Format::Text;
    let mut i = 0;
    let mut options = true;
    while i < args.len() {
        match args[i]
            .to_str()
            .ok_or("Command and option names must be valid UTF-8")?
        {
            "--help" | "-h" if options => return Ok(None),
            "--" if options => {
                options = false;
                i += 1;
            }
            "--cwd" if options => {
                let value = args.get(i + 1).ok_or("Missing value for --cwd")?;
                cwd = Some(PathBuf::from(value));
                i += 2;
            }
            "--format" if options => {
                let value = args.get(i + 1).ok_or("Missing value for --format")?;
                format = match value.to_str().ok_or("Output format must be valid UTF-8")? {
                    "text" => Format::Text,
                    "json" => Format::Json,
                    other => return Err(format!("Unknown --format: {other}")),
                };
                i += 2;
            }
            value if options && value.starts_with("--") => {
                return Err(format!("Unknown argument: {value}"));
            }
            value => {
                if command.is_some() {
                    return Err(format!("More than one command string: {value}"));
                }
                command = Some(value.to_string());
                i += 1;
            }
        }
    }
    let command = command.ok_or("Missing command string")?;
    let cwd = match cwd {
        Some(c) => c,
        None => {
            std::env::current_dir().map_err(|e| format!("Cannot get working directory: {e}"))?
        }
    };
    Ok(Some(CheckArgs {
        command,
        cwd,
        format,
    }))
}

/// 環境から読み込んだ engine。失敗したら終了コード。
fn build_engine(cwd: PathBuf) -> Result<Engine, i32> {
    let home = env_path("HOME");
    let tmpdir = env_path("TMPDIR");
    let user_config = user_config_path();
    Ok(Engine::load(
        user_config.as_deref(),
        EngineEnv { home, tmpdir, cwd },
        guardian_app::runtime::ParserRuntime::new(std::env::current_exe().map_err(|_| 3)?),
    ))
}

/// 空文字列は「無い」として扱う。空のパスはすべてのパスに一致してしまう。
fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn user_config_path() -> Option<PathBuf> {
    guardian_app::config_loader::user_config_path(
        env_path("XDG_CONFIG_HOME").as_deref(),
        env_path("HOME").as_deref(),
    )
}

fn output(args: std::fmt::Arguments<'_>) -> std::io::Result<()> {
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{args}")?;
    stdout.flush()
}

fn diagnostic(args: std::fmt::Arguments<'_>) {
    let _ = writeln!(std::io::stderr().lock(), "{args}");
}

fn print_text(report: &Report) -> std::io::Result<()> {
    if report.verdict == Verdict::Allow {
        return output(format_args!("Verdict: allow"));
    }
    output(format_args!("Verdict: {}", report.verdict))?;
    if !report.message.is_empty() {
        output(format_args!("{}", report.message))?;
    }
    Ok(())
}

fn print_json(report: &Report) -> std::io::Result<()> {
    let effects: Vec<serde_json::Value> = report
        .effects
        .iter()
        .map(|e| {
            serde_json::json!({
                "op": e.op.name(),
                "path": message::display_target(&e.target),
                "class": e.class.as_str(),
                "verdict": e.verdict.as_str(),
                "reason": message::reason_line(e.class, &e.why),
            })
        })
        .collect();
    let rules: Vec<serde_json::Value> = report
        .rules
        .iter()
        .map(|r| {
            serde_json::json!({
                "program": r.name,
                "reason": r.reason,
                "verdict": r.verdict.as_str(),
            })
        })
        .collect();
    let json = serde_json::json!({
        "verdict": report.verdict.as_str(),
        "reason": report.reason,
        "message": report.message,
        "effects": effects,
        "rules": rules,
    });
    output(format_args!("{json}"))
}
