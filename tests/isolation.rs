//! S10: 入力の上限は隔離と事後検査で実現する（REQ-039・EX-057）。
//!
//! 判定は実行ファイルを通して確かめる。cargo test のプロセスの中で深い入力を
//! 解析すると、異常終了（スタックオーバーフロー）は捕まえられずテストごと落ちる
//! ため、隔離の境界は実行ファイルの外から見る。

#[path = "../crates/guardian-parser/tests/common/mod.rs"]
mod common;

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_hook-guardian")
}

fn temp_home() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("hook-guardian-isolation-home-")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap()
}

/// 実行ファイルの終わり方。
enum Ended {
    Exit(i32),
    Signal(i32),
    /// 外側の時間でも判定が返らなかった。
    Timeout,
}

/// 上限つきで check を走らせる。
fn judge(input: &str, home: &Path) -> Ended {
    let mut child = Command::new(bin())
        .args(["check", input, "--cwd", "/tmp/scratch"])
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("TMPDIR", "/tmp")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("実行ファイルを起動できない");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait().expect("子プロセスの状態を取れない") {
            use std::os::unix::process::ExitStatusExt;
            return match (status.code(), status.signal()) {
                (Some(code), _) => Ended::Exit(code),
                (None, Some(signal)) => Ended::Signal(signal),
                (None, None) => Ended::Timeout,
            };
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ended::Timeout;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// 0/1/2 のどれかで返ったことを確かめ、そのコードを返す。深い入力でも
/// 判定は必ず返り、プロセスは落ちない（REQ-039）。
fn verdict_code(input: &str, home: &Path) -> i32 {
    match judge(input, home) {
        Ended::Exit(code) => {
            assert!(
                (0..=2).contains(&code),
                "0/1/2 以外の終了コード {code}: {}",
                label(input)
            );
            code
        }
        Ended::Signal(signal) => panic!("シグナル {signal} で落ちた: {}", label(input)),
        Ended::Timeout => panic!("判定が返らない: {}", label(input)),
    }
}

/// 失敗の表示用に、入力の先頭だけを出す。
fn label(input: &str) -> String {
    let head: String = input.chars().take(60).collect();
    format!("{head:?}（{} バイト）", input.len())
}

/// 128 段を超える入れ子の綴り（REQ-039・EX-057）。判定は block になり、
/// プロセスは異常終了しない。
fn over_limit_inputs() -> Vec<String> {
    vec![
        // `)` の直後の `#` の後ろの入れ子。
        format!("echo $(true)#{}: {}", "$(".repeat(2700), ")".repeat(2700)),
        // here-doc の `$(...)` の中の波括弧。
        format!(
            "cat <<EOF\n$({}true{})\nEOF\n",
            "{ ".repeat(2000),
            " ; }".repeat(2000)
        ),
        // バッククォートの中の入れ子の case。
        format!(
            "echo `{}:{}`",
            "case x in a) ".repeat(2000),
            " ;; esac".repeat(2000)
        ),
        // here-doc の中の入れ子の算術展開。
        format!(
            "cat <<EOF\n{}1{}\nEOF\n",
            "$((".repeat(1000),
            "))".repeat(1000)
        ),
        // `${...}` の中の `)` と入れ子のサブシェル。
        format!("{}: {}", "( ${x:-)}; ".repeat(2000), ")".repeat(2000)),
        // パラメータ展開の閉じの直後の `#` の後ろの入れ子。
        format!("echo ${{x}}#{}: {}", "$(".repeat(2000), ")".repeat(2000)),
    ]
}

/// 読めない深い綴り。子が読めれば ask、子の異常終了なら block になる
/// （REQ-038・REQ-039）。
fn unreadable_deep_inputs() -> Vec<String> {
    vec![
        // バッククォートの中の閉じない波括弧。
        format!("echo {}`", "{ ".repeat(2000)),
        // `case` の枝の中の入れ子（brush は読めない）。
        format!(
            "{}:{}",
            "case x in a) echo $(".repeat(2000),
            ");; esac".repeat(2000)
        ),
    ]
}

// @kotowari[EX-057]
#[test]
fn ex_057_a_deep_input_still_gets_a_verdict() {
    // 128 段を超える入れ子（here-doc とバッククォートの中を含む）は block になり、
    // プロセスは異常終了しない。
    let home = temp_home();
    for input in over_limit_inputs() {
        let code = verdict_code(&input, home.path());
        assert_eq!(code, 2, "深い入れ子は block になる: {}", label(&input));
    }
}

// @kotowari[REQ-038, REQ-039]
#[test]
fn req_038_unreadable_deep_inputs_still_get_a_verdict() {
    // 読めない深い綴りは、子が読めれば ask、子の異常終了なら block になる。
    // どちらでも allow にはならず、プロセスは落ちない（REQ-038・REQ-039）。
    let home = temp_home();
    for input in unreadable_deep_inputs() {
        let code = verdict_code(&input, home.path());
        assert!(
            (1..=2).contains(&code),
            "読めない入力は allow にしない: {}",
            label(&input)
        );
    }
}

// @kotowari[REQ-039]
#[test]
fn req_039_arithmetic_and_conditional_depth_blocks_over_the_limit() {
    // 129 段は block、128 段は通常どおり判定する（REQ-039）。条件式の括弧の
    // 綴りも同じ数え方をする。
    let home = temp_home();
    let at = "(".repeat(128);
    let close = ")".repeat(128);
    let over = "(".repeat(129);
    let over_close = ")".repeat(129);
    for (input, expected) in [
        (format!("echo $(( {over}1{over_close} ))"), 2),
        (format!("[[ {over}-n x{over_close} ]]"), 2),
        (format!("echo $(( {at}1{close} ))"), 0),
        (format!("[[ {at}-n x{close} ]]"), 0),
    ] {
        let code = verdict_code(&input, home.path());
        assert_eq!(code, expected, "{}", label(&input));
    }
}

// @kotowari[REQ-039]
#[test]
fn req_039_the_parse_exchange_budget_blocks_over_the_limit() {
    // 1 回の判定で行う構文解析は 1000 回まで（REQ-039）。超えたら理由をつけて
    // block にする。内側は通常どおり判定する。
    let home = temp_home();
    let under = "eval x; ".repeat(999);
    let code = verdict_code(&under, home.path());
    assert_eq!(code, 0, "予算の内側は通常どおり判定する: {}", label(&under));

    let over = "eval x; ".repeat(1000);
    let out = Command::new(bin())
        .args(["check", &over, "--cwd", "/tmp/scratch", "--format", "json"])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("TMPDIR", "/tmp")
        .output()
        .expect("実行ファイルを起動できない");
    assert_eq!(out.status.code(), Some(2), "予算の超過は block になる");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("判定の上限"), "{stdout}");
}

// @kotowari[REQ-039]
#[test]
fn req_039_the_grammar_corpus_never_makes_the_process_die() {
    // 読める入力のコーパスは、判定が返ること（0/1/2 のどれか）を確かめる。
    let home = temp_home();
    for input in common::CORPUS {
        let _ = verdict_code(input, home.path());
    }
    // 算術式の内側の括弧は、正規化した構文木では平坦な断片になる。深い綴りは
    // block になり、判定は必ず返る（REQ-039）。
    let arithmetic = format!(
        "cat <<EOF\n$((({}1{}))\nEOF\n",
        "(".repeat(6000),
        ")".repeat(6000)
    );
    let code = verdict_code(&arithmetic, home.path());
    assert_eq!(code, 2, "深い算術は block になる: {}", label(&arithmetic));
}

// @kotowari[REQ-039]
#[test]
fn req_039_ambiguous_spellings_do_not_turn_ordinary_scripts_into_asks() {
    // 浅い入れ子の普通のスクリプトは ask にしない（block/allow のまま）。
    // URL の `#`、`&&` の後ろのコメント、`COLOR=#...`、`${x}#y`、`${x%)}`、
    // 引用の中の括弧の綴りを含む。
    let home = temp_home();
    let block = format!(
        "echo see https://example.com/#top\ntrue && # keep going\nCOLOR=#ff0000\necho ${{HOME}}#x\necho ${{HOME%)}}\necho '{}'\n{}rm -rf /etc/x\n",
        "(".repeat(200),
        "echo $(true)\n".repeat(130)
    );
    let code = verdict_code(&block, home.path());
    assert_eq!(code, 2, "block になる: {}", label(&block));

    // 深い入れ子の無いスクリプトは allow のまま。
    let allow = format!("echo $(date)#\necho '{}'\n", "(".repeat(200));
    let code = verdict_code(&allow, home.path());
    assert_eq!(code, 0, "allow になる: {}", label(&allow));
}
