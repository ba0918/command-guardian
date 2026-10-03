//! 影実行のログ（REQ-019）。判定を 1 行ずつ追記し、所有者だけが読める権限にする。

use guardian_app::Report;
use guardian_policy::message;
use std::fs::{DirBuilder, File};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// ログのパス。`XDG_STATE_HOME` が無いときは `~/.local/state` の下。
/// 空文字列は「無い」として扱う。空のパスは cwd 相対になってしまう。
pub fn shadow_log_path(xdg_state_home: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    let state = xdg_state_home.filter(|p| !p.as_os_str().is_empty());
    let base = match state {
        Some(state) => state.to_path_buf(),
        None => home
            .filter(|h| !h.as_os_str().is_empty())?
            .join(".local/state"),
    };
    Some(base.join("command-guardian/shadow.log"))
}

/// 影実行の 1 判定を追記する。
pub fn write_shadow(report: &Report, command: &str) -> std::io::Result<()> {
    let xdg_state_home = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let Some(path) = shadow_log_path(xdg_state_home.as_deref(), home.as_deref()) else {
        return Err(std::io::Error::other("No shadow log location"));
    };
    let dir = path.parent().expect("ログのパスには親がある");
    let base = dir.parent().expect("ログのディレクトリには親がある");
    DirBuilder::new().recursive(true).mode(0o700).create(base)?;
    let base = File::open(base)?;
    use rustix::fs::{Mode, OFlags, mkdirat, openat};
    let private_mode = Mode::from_bits_truncate(0o700);
    match mkdirat(&base, "command-guardian", private_mode) {
        Ok(()) | Err(rustix::io::Errno::EXIST) => {}
        Err(error) => return Err(error.into()),
    }
    let dir = openat(
        &base,
        "command-guardian",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    rustix::fs::fchmod(&dir, private_mode)?;
    let mut file = File::from(openat(
        &dir,
        "shadow.log",
        OFlags::WRONLY
            | OFlags::CREATE
            | OFlags::APPEND
            | OFlags::NOFOLLOW
            | OFlags::NONBLOCK
            | OFlags::CLOEXEC,
        Mode::from_bits_truncate(0o600),
    )?);
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other("Shadow log is not a regular file"));
    }
    file.lock()?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    writeln!(
        file,
        "{now}\t{}\t{}\t{}\t{}",
        report.verdict,
        escape_field(&reason_text(report)),
        escape_field(&target_text(report)),
        escape_field(command)
    )
}

/// 1 行 1 レコードを守るため、改行とタブをエスケープする。
fn escape_field(s: &str) -> String {
    s.replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

/// 理由。無ければ "-"。
fn reason_text(report: &Report) -> String {
    if report.message.is_empty() {
        "-".to_string()
    } else {
        report.message.clone()
    }
}

/// 対象パス。最も重い効果の対象、効果が無ければ "-"。
fn target_text(report: &Report) -> String {
    let mut worst: Option<&guardian_app::EffectReport> = None;
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
