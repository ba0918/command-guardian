use guardian_advisor::{eligible, Mode};
use guardian_core::Verdict;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

pub fn probe_deadline(sent: Instant, received: Instant, remaining_ms: u64) -> Option<Instant> {
    if remaining_ms == 0
        || received < sent
        || received.duration_since(sent) > Duration::from_millis(100)
    {
        return None;
    }
    sent.checked_add(Duration::from_millis(remaining_ms))
}

pub fn negotiation_deadline(
    finished: Instant,
    original: Instant,
    timeout_ms: u64,
) -> Option<(Instant, Instant)> {
    let advice = finished.checked_add(Duration::from_millis(timeout_ms))?;
    let notification = finished
        .checked_add(Duration::from_millis(100))?
        .min(original.checked_sub(Duration::from_millis(500))?)
        .min(advice);
    (notification > finished).then_some((notification, advice))
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Message {
    BudgetProbe {
        version: u8,
        nonce: String,
    },
    BudgetReply {
        version: u8,
        nonce: String,
        original_remaining_ms: u64,
    },
    AdvisoryStart {
        version: u8,
        nonce: String,
        timeout_ms: u64,
    },
    AdvisoryAck {
        version: u8,
        nonce: String,
        accepted: bool,
    },
}

enum Phase {
    New,
    Probed(Instant),
    Finished,
}

pub struct Control {
    socket: UnixStream,
    nonce: String,
    phase: Phase,
}

impl Control {
    pub fn inherited() -> Option<Self> {
        if std::env::var_os("COMMAND_GUARDIAN_ADVISOR_CONTROL").as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            return None;
        }
        use std::os::fd::{FromRawFd, OwnedFd};
        unsafe extern "C" {
            fn dup(fd: i32) -> i32;
            fn close(fd: i32) -> i32;
        }
        // dup validates the inherited integer before Rust takes ownership of a descriptor.
        let fd = unsafe { dup(3) };
        if fd < 0 {
            return None;
        }
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        rustix::io::fcntl_setfd(&owned, rustix::io::FdFlags::CLOEXEC).ok()?;
        let socket = UnixStream::from(owned);
        socket.peer_addr().ok()?;
        unsafe {
            close(3);
        }
        let mut nonce = [0; 16];
        std::fs::File::open("/dev/urandom")
            .ok()?
            .read_exact(&mut nonce)
            .ok()?;
        Some(Self::new(socket, nonce))
    }

    pub fn new(socket: UnixStream, nonce: [u8; 16]) -> Self {
        Self {
            socket,
            nonce: nonce.iter().map(|b| format!("{b:02x}")).collect(),
            phase: Phase::New,
        }
    }

    fn send(&mut self, value: &Message, deadline: Instant) -> Option<()> {
        let mut bytes = serde_json::to_vec(value).ok()?;
        bytes.push(b'\n');
        let mut bytes = bytes.as_slice();
        while !bytes.is_empty() {
            let remaining = deadline.checked_duration_since(Instant::now())?;
            if remaining.is_zero() {
                return None;
            }
            self.socket.set_write_timeout(Some(remaining)).ok()?;
            let count = self.socket.write(bytes).ok()?;
            if count == 0 {
                return None;
            }
            bytes = &bytes[count..];
        }
        (Instant::now() < deadline).then_some(())
    }

    fn receive(&mut self, deadline: Instant) -> Option<Message> {
        let mut bytes = Vec::new();
        while bytes.len() < 4096 {
            let remaining = deadline.checked_duration_since(Instant::now())?;
            if remaining.is_zero() {
                return None;
            }
            self.socket.set_read_timeout(Some(remaining)).ok()?;
            let mut byte = [0];
            self.socket.read_exact(&mut byte).ok()?;
            if byte[0] == b'\n' {
                if Instant::now() >= deadline {
                    return None;
                }
                let value =
                    guardian_advisor::parse_unique_json(std::str::from_utf8(&bytes).ok()?).ok()?;
                return serde_json::from_value(value).ok();
            }
            bytes.push(byte[0]);
        }
        None
    }

    pub fn probe(&mut self, mode: Mode) -> bool {
        if !matches!(self.phase, Phase::New) {
            return false;
        }
        self.phase = Phase::Finished;
        if mode == Mode::Off {
            return false;
        }
        let sent = Instant::now();
        let Some(deadline) = sent.checked_add(Duration::from_millis(100)) else {
            return false;
        };
        let message = Message::BudgetProbe {
            version: 1,
            nonce: self.nonce.clone(),
        };
        if self.send(&message, deadline).is_none() {
            return false;
        }
        match self.receive(deadline) {
            Some(Message::BudgetReply {
                version: 1,
                nonce,
                original_remaining_ms,
            }) if nonce == self.nonce => {
                match probe_deadline(sent, Instant::now(), original_remaining_ms) {
                    Some(original) => {
                        self.phase = Phase::Probed(original);
                        true
                    }
                    None => false,
                }
            }
            _ => false,
        }
    }

    pub fn start(
        &mut self,
        mode: Mode,
        machine: Verdict,
        finished: Instant,
        timeout_ms: u64,
    ) -> Option<Instant> {
        let phase = std::mem::replace(&mut self.phase, Phase::Finished);
        let Phase::Probed(original) = phase else {
            return None;
        };
        if !eligible(mode, machine) {
            return None;
        }
        let (notification, advice) = negotiation_deadline(finished, original, timeout_ms)?;
        self.send(
            &Message::AdvisoryStart {
                version: 1,
                nonce: self.nonce.clone(),
                timeout_ms,
            },
            notification,
        )?;
        match self.receive(notification) {
            Some(Message::AdvisoryAck {
                version: 1,
                nonce,
                accepted: true,
            }) if nonce == self.nonce && Instant::now() < notification => Some(advice),
            _ => None,
        }
    }
}
