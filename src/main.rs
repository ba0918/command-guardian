//! hook-guardian の実行ファイル。`check` と `hook` の 2 つのコマンドを持つ。

mod hook;
mod log;

use guardian_core::Verdict;
use guardian_policy::{message, Engine, EngineEnv, Report};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(run(&args));
}

fn run(args: &[String]) -> i32 {
    match args.first().map(|s| s.as_str()) {
        Some("check") => cmd_check(&args[1..]),
        Some("hook") => hook::run(&args[1..]),
        Some(other) => {
            eprintln!("知らないコマンドです: {other}");
            eprintln!("使い方: hook-guardian <check|hook> ...");
            3
        }
        None => {
            eprintln!("使い方: hook-guardian <check|hook> ...");
            3
        }
    }
}

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

fn cmd_check(args: &[String]) -> i32 {
    let parsed = match parse_check(args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return 3;
        }
    };
    let engine = match build_engine(parsed.cwd.clone()) {
        Ok(e) => e,
        Err(code) => return code,
    };
    let report = engine.check(&parsed.command);
    for w in &report.warnings {
        eprintln!("警告: {w}");
    }
    match parsed.format {
        Format::Text => print_text(&report),
        Format::Json => print_json(&report),
    }
    match report.verdict {
        Verdict::Allow => 0,
        Verdict::Ask => 1,
        Verdict::Block => 2,
    }
}

fn parse_check(args: &[String]) -> Result<CheckArgs, String> {
    let mut command: Option<String> = None;
    let mut cwd: Option<PathBuf> = None;
    let mut format = Format::Text;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--cwd" => {
                let value = args.get(i + 1).ok_or("--cwd の値がありません")?;
                cwd = Some(PathBuf::from(value));
                i += 2;
            }
            "--format" => {
                let value = args.get(i + 1).ok_or("--format の値がありません")?;
                format = match value.as_str() {
                    "text" => Format::Text,
                    "json" => Format::Json,
                    other => return Err(format!("知らない --format です: {other}")),
                };
                i += 2;
            }
            value if value.starts_with("--") => {
                return Err(format!("知らない引数です: {value}"));
            }
            value => {
                if command.is_some() {
                    return Err(format!("コマンド文字列が 2 つあります: {value}"));
                }
                command = Some(value.to_string());
                i += 1;
            }
        }
    }
    let command = command.ok_or("コマンド文字列がありません")?;
    let cwd = match cwd {
        Some(c) => c,
        None => {
            std::env::current_dir().map_err(|e| format!("作業ディレクトリを取れません: {e}"))?
        }
    };
    Ok(CheckArgs {
        command,
        cwd,
        format,
    })
}

/// 環境から読み込んだ engine。失敗したら終了コード。
fn build_engine(cwd: PathBuf) -> Result<Engine, i32> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let tmpdir = std::env::var_os("TMPDIR").map(PathBuf::from);
    let user_config = user_config_path();
    Ok(Engine::load(
        user_config.as_deref(),
        EngineEnv { home, tmpdir, cwd },
    ))
}

fn user_config_path() -> Option<PathBuf> {
    guardian_policy::config::user_config_path(
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .as_deref(),
        std::env::var_os("HOME").map(PathBuf::from).as_deref(),
    )
}

fn print_text(report: &Report) {
    if report.verdict == Verdict::Allow {
        println!("判定: allow");
        return;
    }
    println!("判定: {}", report.verdict);
    if !report.message.is_empty() {
        println!("{}", report.message);
    }
}

fn print_json(report: &Report) {
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
        "message": report.message,
        "effects": effects,
        "rules": rules,
    });
    println!("{json}");
}
