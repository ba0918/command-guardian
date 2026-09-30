//! 影実行のログ（REQ-019）。判定を 1 行ずつ追記し、所有者だけが読める権限にする。

use guardian_policy::{message, Report};
use std::fs::{DirBuilder, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// ログのパス。`XDG_STATE_HOME` が無いときは `~/.local/state` の下。
pub fn shadow_log_path(xdg_state_home: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    let base = match xdg_state_home {
        Some(state) => state.to_path_buf(),
        None => home?.join(".local/state"),
    };
    Some(base.join("hook-guardian/shadow.log"))
}

/// 影実行の 1 判定を追記する。
pub fn write_shadow(report: &Report, command: &str) -> std::io::Result<()> {
    let xdg_state_home = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let Some(path) = shadow_log_path(xdg_state_home.as_deref(), home.as_deref()) else {
        return Ok(());
    };
    let dir = path.parent().expect("ログのパスには親がある");
    DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&path)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    writeln!(
        file,
        "{now}\t{}\t{}\t{}\t{command}",
        report.verdict,
        reason_text(report),
        target_text(report)
    )
}

/// 理由。文面は複数行なので 1 行にたたむ。
fn reason_text(report: &Report) -> String {
    if report.message.is_empty() {
        "-".to_string()
    } else {
        report.message.replace('\n', " ")
    }
}

/// 対象パス。最も重い効果の対象、効果が無ければ "-"。
fn target_text(report: &Report) -> String {
    let mut worst: Option<&guardian_policy::EffectReport> = None;
    for e in &report.effects {
        worst = Some(match worst {
            None => e,
            Some(w) if e.verdict > w.verdict => e,
            Some(w) => w,
        });
    }
    worst
        .map(|e| message::display_target(&e.target))
        .unwrap_or_else(|| "-".to_string())
}
