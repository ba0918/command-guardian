//! 入力由来の解析を隔離した子プロセスで行う（REQ-039）。
//!
//! 親は同じ実行ファイルを子として 1 度だけ起動し、stdin に繋いだソケットで
//! 要求と応答をやり取りする。子はスタックの上限つきのスレッドで解析し、親が
//! ソケットを閉じるまで働き続ける。子の死は原因で分ける（REQ-039・A23）:
//! 入力に帰せる死（スタックオーバーフロー、時間の上限の超過）と上限の超過は
//! Failure::Limit（block）、自分に帰せる死（panic、起動とプロトコルの失敗、
//! 帰せない死）は Failure::Internal（ask）。1 回の判定で行うやり取りの回数と
//! 時間には予算を設ける（REQ-039）。

use super::budget;
use super::wire::{self, Response};
use guardian_parser::{Failure, Outcome};
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// 子として起動されたことを示す環境変数。値は使い捨ての nonce。
const WORKER_ENV: &str = "COMMAND_GUARDIAN_PARSER_WORKER";

/// 子の解析スレッドのスタック。上限つきにする。深すぎる入力は子ごと落ちるが、
/// 親はそれを入力に帰せる死として block にする（REQ-039・A23）。
pub(crate) const CHILD_STACK_BYTES: usize = 8 * 1024 * 1024;

/// 引用の除去を行うスレッドのスタック。設定の照合は判定の重さに効くため、
/// 解析の死より深い入力まで本文を返す（隔離前の解析スレッドと同じ 32 MiB）。
/// 除去が死んだときは原因で分け、入力に帰せる死は block、自分に帰せる失敗は
/// ask にする（REQ-039・A23）。
pub(crate) const STRIP_STACK_BYTES: usize = 32 * 1024 * 1024;

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

/// 子の死を確かめるときに待つ幅。ソケットの終わりを見てからプロセスの状態が
/// 見えるまでの隙間を埋める。
const DEATH_GRACE: Duration = Duration::from_millis(100);

/// スタックオーバーフローで子が落ちるときのシグナル（Linux の値。libc には
/// 依存しない）。SIGABRT は Rust の実行時がスタックオーバーフローを検出した
/// ときに使う。
const SIGILL: i32 = 4;
const SIGABRT: i32 = 6;
const SIGBUS: i32 = 7;
const SIGSEGV: i32 = 11;

/// 枠の本体の上限。壊れた枠で巨大な領域を取らないための歯止め。
const MAX_FRAME_BYTES: usize = 256 * 1024 * 1024;

/// 要求の枠の頭（nonce 16 + 種別 1 + 長さ 4）。
const REQUEST_HEADER: usize = 21;
/// 応答の枠の頭（nonce 16 + 長さ 4）。
const RESPONSE_HEADER: usize = 20;

/// 使い回す子。プロセスごとに 1 つ持ち、やり取りは直列化する。
struct Worker {
    child: Child,
    socket: UnixStream,
    nonce: [u8; 16],
}

pub struct ParserRuntime {
    worker: Option<Worker>,
    executable: PathBuf,
}

impl ParserRuntime {
    pub fn new(executable: PathBuf) -> Self {
        Self {
            worker: None,
            executable,
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 子として起動されていれば親の要求を、親がソケットを閉じるまで処理する。
/// 子でなければ何もしない。実行ファイルの main と、解析の入口（parse /
/// strip_quotes）が呼ぶ。main を持たないテストの実行ファイルでも子として
/// 働けるように、入口の側からも呼ぶ。子では 1 つのスレッドだけが働き、ほかの
/// 入口は Once で止まる（ほかのテストが親の要求を乱さない）。
pub fn run_if_child() {
    if std::env::var_os(WORKER_ENV).is_none() {
        return;
    }
    {
        // run は親がソケットを閉じるまで返らない。ここへ来るのは panic を
        // 捕まえたときだけ。
        let _ = std::panic::catch_unwind(run);
        std::process::exit(1);
    }
}

/// 子プロセスとして親の要求を順に処理する。
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
    loop {
        let Some(request) = read_request(&mut socket) else {
            // 親がソケットを閉じた。
            std::process::exit(0);
        };
        if request.nonce != nonce {
            std::process::exit(1);
        }
        let response = match request.mode {
            wire::MODE_PARSE => {
                let (failures, script) = super::parse_in_child(&request.input);
                Response::Parsed { failures, script }
            }
            wire::MODE_STRIP_QUOTES => {
                Response::Stripped(super::strip_quotes_in_child(&request.input))
            }
            _ => std::process::exit(1),
        };
        let body = wire::encode_response(&response);
        if write_response(&mut socket, &nonce, &body).is_err() {
            std::process::exit(1);
        }
    }
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

/// 親側: 解析を子に依頼する。失敗は原因で分ける（REQ-039・A23）。
impl ParserRuntime {
    pub(super) fn request_parse(
        &mut self,
        input: &str,
        budget: Option<&mut budget::Budget>,
    ) -> Result<Outcome, Failure> {
        match self.exchange(wire::MODE_PARSE, input, budget) {
            Ok(Response::Parsed { failures, script }) => Ok(Outcome {
                script: script.unwrap_or_default(),
                failures,
            }),
            Ok(Response::Stripped(_)) => Err(Failure::Internal),
            Err(ExchangeError::Limit) => Err(Failure::Limit),
            Err(ExchangeError::Internal) => Err(Failure::Internal),
        }
    }

    /// 親側: 引用を外した本文を子に依頼する。失敗は原因で分ける（REQ-039・A23）。
    pub(super) fn request_strip_quotes(
        &mut self,
        input: &str,
        budget: Option<&mut budget::Budget>,
    ) -> Result<String, Failure> {
        match self.exchange(wire::MODE_STRIP_QUOTES, input, budget) {
            Ok(Response::Stripped(Ok(text))) => Ok(text),
            Ok(Response::Stripped(Err(failure))) => Err(failure),
            Ok(Response::Parsed { .. }) => Err(Failure::Internal),
            Err(ExchangeError::Limit) => Err(Failure::Limit),
            Err(ExchangeError::Internal) => Err(Failure::Internal),
        }
    }
}

/// やり取りが失敗した帰属（REQ-039・A23）。
pub(crate) enum ExchangeError {
    /// 上限の超過か、入力に帰せる死（スタックオーバーフロー、時間の上限の
    /// 超過）。判定は block（Failure::Limit）。
    Limit,
    /// 自分に帰せる失敗（起動・書き込み・プロトコルの失敗、帰せない死）。
    /// 判定は ask（Failure::Internal）。
    Internal,
}

impl ParserRuntime {
    fn exchange(
        &mut self,
        mode: u8,
        input: &str,
        mut budget: Option<&mut budget::Budget>,
    ) -> Result<Response, ExchangeError> {
        // 判定の予算を先に確かめる（REQ-039）。構文解析の回数は構文解析だけが
        // 消費し、引用の除去は時間の上限だけを見る。
        let allowed = budget.as_mut().is_none_or(|budget| {
            if mode == wire::MODE_PARSE {
                budget.take()
            } else {
                budget.within_time()
            }
        });
        if !allowed {
            return Err(ExchangeError::Limit);
        }
        if self.worker.is_none() {
            self.worker = spawn_worker(&self.executable, budget.as_deref());
        }
        let Some(worker) = self.worker.as_mut() else {
            // 子を起こせないのは自分に帰せる失敗（A23）。
            return Err(if budget.as_deref().is_some_and(|b| !b.within_time()) {
                ExchangeError::Limit
            } else {
                ExchangeError::Internal
            });
        };
        let result = exchange_with(
            &mut worker.socket,
            &worker.nonce,
            mode,
            input,
            budget.as_deref(),
        )
        .map_err(|interrupted| classify(interrupted, &mut worker.child));
        match result {
            Ok(response) => Ok(response),
            Err(error) => {
                // 子が死んだか時間切れ。次の要求のために始末する。
                self.worker.take();
                Err(error)
            }
        }
    }
}

/// やり取りが途中で終わった形。死の帰属を決める材料（REQ-039・A23）。
enum Interrupted {
    /// 要求の書き込みに失敗した。
    Write,
    /// 応答の読み取りが時間切れになった。
    Timeout,
    /// 応答が途中で終わったか、壊れていた。
    Protocol,
}

/// やり取りの失敗を、子の終わり方で帰属する（REQ-039・A23）。
fn classify(interrupted: Interrupted, child: &mut Child) -> ExchangeError {
    match interrupted {
        // 読み取りの時間切れは親が子を止める。時間の上限の超過は入力に帰せる。
        Interrupted::Timeout => ExchangeError::Limit,
        // 書き込みとプロトコルの失敗は、子の終わり方を確かめてから決める。
        Interrupted::Write | Interrupted::Protocol => match wait_for_exit(child) {
            Some(status) => status_error(status),
            None => ExchangeError::Internal,
        },
    }
}

/// 子が終わっていればその終わり方を返す。ソケットの終わりを見てから状態が
/// 見えるまでの隙間を短い間だけ待つ。
fn wait_for_exit(child: &mut Child) -> Option<ExitStatus> {
    let deadline = Instant::now() + DEATH_GRACE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(1)),
            _ => return None,
        }
    }
}

/// 入力に帰せる死（スタックオーバーフロー）は block、それ以外の死は ask
/// （REQ-039・A23）。
fn status_error(status: ExitStatus) -> ExchangeError {
    use std::os::unix::process::ExitStatusExt;
    match status.signal() {
        Some(SIGILL) | Some(SIGABRT) | Some(SIGBUS) | Some(SIGSEGV) => ExchangeError::Limit,
        _ => ExchangeError::Internal,
    }
}

/// 子を 1 つ起こし、解析を始められる印まで待つ。
fn spawn_worker(exe: &std::path::Path, budget: Option<&budget::Budget>) -> Option<Worker> {
    let nonce = make_nonce();
    let (mut parent_end, child_end) = UnixStream::pair().ok()?;
    let mut child = Command::new(exe)
        .env(WORKER_ENV, hex(&nonce))
        .stdin(Stdio::from(OwnedFd::from(child_end)))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // 起動待ちも判定の残り時間の内側に収める。
    let startup = budget
        .map(budget::Budget::remaining)
        .map(|remaining| remaining.min(WORKER_STARTUP_TIMEOUT))
        .unwrap_or(WORKER_STARTUP_TIMEOUT);
    if startup.is_zero()
        || parent_end.set_read_timeout(Some(startup)).is_err()
        || read_ready(&mut parent_end, &nonce).is_none()
    {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }
    Some(Worker {
        child,
        socket: parent_end,
        nonce,
    })
}

fn exchange_with(
    socket: &mut UnixStream,
    nonce: &[u8; 16],
    mode: u8,
    input: &str,
    budget: Option<&budget::Budget>,
) -> Result<Response, Interrupted> {
    // 待ち時間は、判定の残り時間と 1 回の解析の上限の短い方にする。
    let timeout = budget
        .map(budget::Budget::remaining)
        .map(|remaining| remaining.min(CHILD_TIMEOUT))
        .unwrap_or(CHILD_TIMEOUT);
    if timeout.is_zero() {
        return Err(Interrupted::Timeout);
    }
    // 相手が読まないまま書き込みが詰まる場合にも時間で切れるようにする。
    let _ = socket.set_write_timeout(Some(timeout));
    let mut request = Vec::with_capacity(REQUEST_HEADER + input.len());
    request.extend_from_slice(nonce);
    request.push(mode);
    request.extend_from_slice(&(input.len() as u32).to_le_bytes());
    request.extend_from_slice(input.as_bytes());
    socket.write_all(&request).map_err(|_| Interrupted::Write)?;
    let _ = socket.set_write_timeout(None);
    let remaining = budget
        .map(budget::Budget::remaining)
        .unwrap_or(CHILD_TIMEOUT)
        .min(CHILD_TIMEOUT);
    if remaining.is_zero() {
        return Err(Interrupted::Timeout);
    }
    socket
        .set_read_timeout(Some(remaining))
        .map_err(|_| Interrupted::Protocol)?;
    let (got, body) = read_response(socket)?;
    if got != *nonce {
        return Err(Interrupted::Protocol);
    }
    wire::decode_response(&body).map_err(|_| Interrupted::Protocol)
}

fn read_ready(socket: &mut UnixStream, nonce: &[u8; 16]) -> Option<()> {
    let mut frame = [0u8; 17];
    socket.read_exact(&mut frame).ok()?;
    if &frame[..16] != nonce || frame[16] != READY_MARKER {
        return None;
    }
    Some(())
}

fn read_response(socket: &mut UnixStream) -> Result<([u8; 16], Vec<u8>), Interrupted> {
    let mut header = [0u8; RESPONSE_HEADER];
    read_exact(socket, &mut header)?;
    let mut nonce = [0u8; 16];
    nonce.copy_from_slice(&header[..16]);
    let len = u32::from_le_bytes([header[16], header[17], header[18], header[19]]) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(Interrupted::Protocol);
    }
    let mut body = vec![0u8; len];
    read_exact(socket, &mut body)?;
    Ok((nonce, body))
}

/// 読み取りの時間切れと、途中で終わった応答を分ける。
fn read_exact(socket: &mut UnixStream, buf: &mut [u8]) -> Result<(), Interrupted> {
    socket.read_exact(buf).map_err(|error| match error.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => Interrupted::Timeout,
        _ => Interrupted::Protocol,
    })
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
