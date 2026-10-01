//! 入力由来の解析を隔離した子プロセスで行う（REQ-039）。
//!
//! 親は同じ実行ファイルを子として起動し、stdin に繋いだソケットで要求と応答を
//! やり取りする。子はスタックの上限つきのスレッドで解析する。子の異常終了と
//! 時間の上限の超過は block の原因（Failure::Limit）に落とし、判定は必ず返す。

use crate::wire::{self, Response};
use crate::Outcome;
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::sync::Once;
use std::time::Duration;

/// 子として起動されたことを示す環境変数。値は使い捨ての nonce。
const WORKER_ENV: &str = "HOOK_GUARDIAN_PARSER_WORKER";

/// 子の解析スレッドのスタック。上限つきにする。深すぎる入力は子ごと落ちるが、
/// 親はそれを ask に落とす。
pub(crate) const CHILD_STACK_BYTES: usize = 8 * 1024 * 1024;

/// 子の応答を待つ時間。代表的な入力は数ミリ秒で返るため、これは病的な入力だけに
/// 掛かる歯止め（REQ-021 の 100ms 予算は代表的な入力に対する目標）。上限いっぱいの
/// 入力でも、境界（128 段）の読み直しと応答の符号化が負荷の下で収まる幅を取る。
const CHILD_TIMEOUT: Duration = Duration::from_secs(2);

/// 子が解析を始められるまでの待ち時間。テストの実行ファイルを子にすると、子が
/// 最初の解析の入口に届くまでにテストの準備（git の起動など）が挟まる。解析
/// そのものの上限は CHILD_TIMEOUT のまま。
const WORKER_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

/// 子が「解析を始められる」印に使うバイト。
const READY_MARKER: u8 = 0xff;

/// 枠の本体の上限。壊れた枠で巨大な領域を取らないための歯止め。
const MAX_FRAME_BYTES: usize = 256 * 1024 * 1024;

/// 要求の枠の頭（nonce 16 + 種別 1 + 長さ 4）。
const REQUEST_HEADER: usize = 21;
/// 応答の枠の頭（nonce 16 + 長さ 4）。
const RESPONSE_HEADER: usize = 20;

static STARTED: Once = Once::new();

/// 子として起動されていれば親の要求を 1 つ処理してプロセスを終了する。
/// 子でなければ何もしない。実行ファイルの main と、解析の入口（parse /
/// strip_quotes）が呼ぶ。main を持たないテストの実行ファイルでも子として
/// 働けるように、入口の側からも呼ぶ。
pub fn run_if_child() {
    if std::env::var_os(WORKER_ENV).is_none() {
        return;
    }
    STARTED.call_once(|| {
        // run は必ず exit する。ここへ来るのは panic を捕まえたときだけ。
        let _ = std::panic::catch_unwind(run);
        std::process::exit(1);
    });
}

/// 子プロセスとして 1 つの要求を処理する。
fn run() -> ! {
    let Some(nonce) = nonce_from_env() else {
        std::process::exit(1);
    };
    // 親が stdin に繋いだソケット。main が最初にここへ来るので、ほかには
    // 読まれていない。
    let mut socket = unsafe { UnixStream::from_raw_fd(0) };
    // 親に「解析を始められる」印を送る。親は起動待ちと解析の上限を分ける。
    if write_ready(&mut socket, &nonce).is_err() {
        std::process::exit(1);
    }
    let Some(request) = read_request(&mut socket) else {
        std::process::exit(1);
    };
    if request.nonce != nonce {
        std::process::exit(1);
    }
    let response = match request.mode {
        wire::MODE_PARSE => {
            let (failures, script) = crate::parse_in_child(&request.input);
            Response::Parsed { failures, script }
        }
        wire::MODE_STRIP_QUOTES => Response::Stripped(crate::strip_quotes_inner(&request.input)),
        _ => std::process::exit(1),
    };
    let body = wire::encode_response(&response);
    let _ = write_response(&mut socket, &nonce, &body);
    std::process::exit(0);
}

struct Request {
    nonce: [u8; 16],
    mode: u8,
    input: String,
}

fn read_request(socket: &mut UnixStream) -> Option<Request> {
    let mut header = [0u8; REQUEST_HEADER];
    socket.read_exact(&mut header).ok()?;
    let mut nonce = [0u8; 16];
    nonce.copy_from_slice(&header[..16]);
    let mode = header[16];
    let len = u32::from_le_bytes([header[17], header[18], header[19], header[20]]) as usize;
    if len > MAX_FRAME_BYTES {
        return None;
    }
    let mut body = vec![0u8; len];
    socket.read_exact(&mut body).ok()?;
    let input = String::from_utf8(body).ok()?;
    Some(Request { nonce, mode, input })
}

fn write_ready(socket: &mut UnixStream, nonce: &[u8; 16]) -> std::io::Result<()> {
    let mut frame = Vec::with_capacity(17);
    frame.extend_from_slice(nonce);
    frame.push(READY_MARKER);
    socket.write_all(&frame)
}

fn write_response(socket: &mut UnixStream, nonce: &[u8; 16], body: &[u8]) -> std::io::Result<()> {
    let mut frame = Vec::with_capacity(RESPONSE_HEADER + body.len());
    frame.extend_from_slice(nonce);
    frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
    frame.extend_from_slice(body);
    socket.write_all(&frame)
}

/// 親側: 解析を子に依頼する。子の異常終了・時間切れ・壊れた応答は None。
pub(crate) fn request_parse(input: &str) -> Option<Outcome> {
    match exchange(wire::MODE_PARSE, input)? {
        Response::Parsed { failures, script } => Some(Outcome {
            script: script.unwrap_or_default(),
            failures,
        }),
        Response::Stripped(_) => None,
    }
}

/// 親側: 引用を外した本文を子に依頼する。
pub(crate) fn request_strip_quotes(input: &str) -> Option<String> {
    match exchange(wire::MODE_STRIP_QUOTES, input)? {
        Response::Stripped(text) => Some(text),
        Response::Parsed { .. } => None,
    }
}

fn exchange(mode: u8, input: &str) -> Option<Response> {
    let exe = std::env::current_exe().ok()?;
    let nonce = make_nonce();
    let (mut parent_end, child_end) = UnixStream::pair().ok()?;
    let mut child = Command::new(exe)
        .env(WORKER_ENV, hex(&nonce))
        .stdin(Stdio::from(OwnedFd::from(child_end)))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let result = exchange_with(&mut parent_end, &nonce, mode, input);
    // 応答が届いていてもいなくても、子は必ず reap する。
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn exchange_with(
    socket: &mut UnixStream,
    nonce: &[u8; 16],
    mode: u8,
    input: &str,
) -> Option<Response> {
    // 相手が読まないまま書き込みが詰まる場合にも時間で切れるようにする。
    let _ = socket.set_write_timeout(Some(CHILD_TIMEOUT));
    let mut request = Vec::with_capacity(REQUEST_HEADER + input.len());
    request.extend_from_slice(nonce);
    request.push(mode);
    request.extend_from_slice(&(input.len() as u32).to_le_bytes());
    request.extend_from_slice(input.as_bytes());
    socket.write_all(&request).ok()?;
    let _ = socket.set_write_timeout(None);
    // 子の起動（解析を始められる印）を待つ。
    socket.set_read_timeout(Some(WORKER_STARTUP_TIMEOUT)).ok()?;
    read_ready(socket, nonce)?;
    // ここからは解析そのものの時間。異常終了も時間切れも ask に落ちる。
    socket.set_read_timeout(Some(CHILD_TIMEOUT)).ok()?;
    let (got, body) = read_response(socket)?;
    if got != *nonce {
        return None;
    }
    wire::decode_response(&body).ok()
}

fn read_ready(socket: &mut UnixStream, nonce: &[u8; 16]) -> Option<()> {
    let mut frame = [0u8; 17];
    socket.read_exact(&mut frame).ok()?;
    if &frame[..16] != nonce || frame[16] != READY_MARKER {
        return None;
    }
    Some(())
}

fn read_response(socket: &mut UnixStream) -> Option<([u8; 16], Vec<u8>)> {
    let mut header = [0u8; RESPONSE_HEADER];
    socket.read_exact(&mut header).ok()?;
    let mut nonce = [0u8; 16];
    nonce.copy_from_slice(&header[..16]);
    let len = u32::from_le_bytes([header[16], header[17], header[18], header[19]]) as usize;
    if len > MAX_FRAME_BYTES {
        return None;
    }
    let mut body = vec![0u8; len];
    socket.read_exact(&mut body).ok()?;
    Some((nonce, body))
}

/// 使い捨ての nonce。値そのものは秘密ではない（偶然の起動を弾く印）。
fn make_nonce() -> [u8; 16] {
    use std::hash::{BuildHasher, Hasher};
    let mut nonce = [0u8; 16];
    for chunk in nonce.chunks_mut(8) {
        let digest = std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish()
            .to_le_bytes();
        chunk.copy_from_slice(&digest[..chunk.len()]);
    }
    nonce
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn nonce_from_env() -> Option<[u8; 16]> {
    let text = std::env::var(WORKER_ENV).ok()?;
    if text.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for (index, chunk) in text.as_bytes().chunks(2).enumerate() {
        let chunk = std::str::from_utf8(chunk).ok()?;
        out[index] = u8::from_str_radix(chunk, 16).ok()?;
    }
    Some(out)
}
