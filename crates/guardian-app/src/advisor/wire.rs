use guardian_advisor::{Assessment, Failure, RawDistribution, ScopeEvidence, Skip};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::time::Instant;

const HEADER_BYTES: usize = 22;
pub const RESPONSE_LIMIT: usize = 65536;

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum FrameKind {
    Request = 1,
    Reply = 2,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reply {
    Assessment {
        risk: RawDistribution,
        scope: RawDistribution,
        evidence: ScopeEvidence,
    },
    Failure {
        failure: Failure,
    },
    Skipped {
        skip: Skip,
    },
}

impl Reply {
    pub fn validated(&self) -> Result<Option<(Assessment, ScopeEvidence)>, Failure> {
        match self {
            Self::Assessment {
                risk,
                scope,
                evidence,
            } => Assessment::validate(risk.clone(), scope.clone()).map(|a| Some((a, *evidence))),
            Self::Failure { failure } => Err(*failure),
            Self::Skipped { .. } => Ok(None),
        }
    }
}

pub fn request_limit(max_request_bytes: usize) -> Result<usize, Failure> {
    let limit = max_request_bytes
        .checked_add(65536)
        .ok_or(Failure::InvalidResponse)?;
    u32::try_from(limit).map_err(|_| Failure::InvalidResponse)?;
    Ok(limit)
}

fn remaining(deadline: Instant) -> Result<std::time::Duration, Failure> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        Err(Failure::Timeout)
    } else {
        Ok(remaining)
    }
}

fn io_failure(error: std::io::Error) -> Failure {
    match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => Failure::Timeout,
        _ => Failure::InvalidResponse,
    }
}

fn read_exact(
    stream: &mut UnixStream,
    mut bytes: &mut [u8],
    deadline: Instant,
) -> Result<(), Failure> {
    while !bytes.is_empty() {
        stream
            .set_read_timeout(Some(remaining(deadline)?))
            .map_err(io_failure)?;
        let count = stream.read(bytes).map_err(io_failure)?;
        if count == 0 {
            return Err(Failure::InvalidResponse);
        }
        bytes = &mut bytes[count..];
    }
    remaining(deadline)?;
    Ok(())
}

pub fn read_frame<T: serde::de::DeserializeOwned>(
    stream: &mut UnixStream,
    nonce: [u8; 16],
    kind: FrameKind,
    max_bytes: usize,
    deadline: Instant,
) -> Result<T, Failure> {
    let mut header = [0; HEADER_BYTES];
    read_exact(stream, &mut header, deadline)?;
    if header[0] != 1 || header[1..17] != nonce || header[17] != kind as u8 {
        return Err(Failure::InvalidResponse);
    }
    let length = u32::from_be_bytes(
        header[18..22]
            .try_into()
            .map_err(|_| Failure::InvalidResponse)?,
    ) as usize;
    if length > max_bytes {
        return Err(Failure::InvalidResponse);
    }
    let mut body = vec![0; length];
    read_exact(stream, &mut body, deadline)?;
    stream
        .set_read_timeout(Some(remaining(deadline)?))
        .map_err(io_failure)?;
    if stream.read(&mut [0; 1]).map_err(io_failure)? != 0 {
        return Err(Failure::InvalidResponse);
    }
    let text = std::str::from_utf8(&body).map_err(|_| Failure::InvalidResponse)?;
    let value = guardian_advisor::parse_unique_json(text).map_err(|_| Failure::InvalidResponse)?;
    let value = serde_json::from_value(value).map_err(|_| Failure::InvalidResponse)?;
    remaining(deadline)?;
    Ok(value)
}

pub fn write_frame<T: Serialize>(
    stream: &mut UnixStream,
    nonce: [u8; 16],
    kind: FrameKind,
    value: &T,
    max_bytes: usize,
    deadline: Instant,
) -> Result<(), Failure> {
    remaining(deadline)?;
    let body = serde_json::to_vec(value).map_err(|_| Failure::InvalidResponse)?;
    if body.len() > max_bytes {
        return Err(Failure::InvalidResponse);
    }
    let length = u32::try_from(body.len()).map_err(|_| Failure::InvalidResponse)?;
    let mut header = [0; HEADER_BYTES];
    header[0] = 1;
    header[1..17].copy_from_slice(&nonce);
    header[17] = kind as u8;
    header[18..22].copy_from_slice(&length.to_be_bytes());
    for mut bytes in [header.as_slice(), body.as_slice()] {
        while !bytes.is_empty() {
            stream
                .set_write_timeout(Some(remaining(deadline)?))
                .map_err(io_failure)?;
            let count = stream.write(bytes).map_err(io_failure)?;
            if count == 0 {
                return Err(Failure::InvalidResponse);
            }
            bytes = &bytes[count..];
        }
    }
    stream.shutdown(Shutdown::Write).map_err(io_failure)?;
    remaining(deadline)?;
    Ok(())
}

pub fn read_reply(
    stream: &mut UnixStream,
    nonce: [u8; 16],
    deadline: Instant,
) -> Result<Reply, Failure> {
    read_frame(stream, nonce, FrameKind::Reply, RESPONSE_LIMIT, deadline)
}

pub fn write_reply(
    stream: &mut UnixStream,
    nonce: [u8; 16],
    reply: &Reply,
    deadline: Instant,
) -> Result<(), Failure> {
    write_frame(
        stream,
        nonce,
        FrameKind::Reply,
        reply,
        RESPONSE_LIMIT,
        deadline,
    )
}
