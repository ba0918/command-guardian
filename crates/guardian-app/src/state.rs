use rustix::fs::{mkdirat, open, openat, unlinkat, AtFlags, Mode, OFlags};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs::{DirBuilder, File};
use std::hash::{Hash, Hasher};
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::Path;
use std::time::Instant;

pub struct AdvisorState {
    directory: File,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheFrame {
    version: u8,
    session: String,
    saved_at: u64,
    payload: Vec<u8>,
}

fn check_time(deadline: Instant) -> io::Result<()> {
    if Instant::now() >= deadline {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Advisor state deadline",
        ))
    } else {
        Ok(())
    }
}

fn verify(file: &File, directory: bool) -> io::Result<()> {
    let metadata = file.metadata()?;
    let expected_mode = if directory { 0o700 } else { 0o600 };
    if metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o777 != expected_mode
        || (directory && !metadata.is_dir())
        || (!directory && (!metadata.is_file() || metadata.nlink() != 1))
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Unsafe advisor state",
        ));
    }
    Ok(())
}

fn child_directory(parent: &File, name: &str, create: bool) -> io::Result<File> {
    if create {
        match mkdirat(parent, name, Mode::from_bits_truncate(0o700)) {
            Ok(()) | Err(rustix::io::Errno::EXIST) => {}
            Err(error) => return Err(error.into()),
        }
    }
    let file = File::from(openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    verify(&file, true)?;
    Ok(file)
}

fn cache_name(session: &str) -> String {
    let mut hash = DefaultHasher::new();
    session.hash(&mut hash);
    format!("{:016x}.json", hash.finish())
}

impl AdvisorState {
    pub fn append_advisor_log(
        &self,
        record: &serde_json::Value,
        deadline: Instant,
    ) -> io::Result<()> {
        check_time(deadline)?;
        let mut bytes = serde_json::to_vec(record)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid advisor log"))?;
        bytes.push(b'\n');
        let mut file = File::from(openat(
            &self.directory,
            "advisor.jsonl",
            OFlags::WRONLY
                | OFlags::CREATE
                | OFlags::APPEND
                | OFlags::NOFOLLOW
                | OFlags::NONBLOCK
                | OFlags::CLOEXEC,
            Mode::from_bits_truncate(0o600),
        )?);
        verify(&file, false)?;
        check_time(deadline)?;
        file.try_lock()
            .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "Advisor log busy"))?;
        check_time(deadline)?;
        file.write_all(&bytes)?;
        check_time(deadline)
    }

    pub fn save_context_with_limit(
        &self,
        session: &str,
        payload: &[u8],
        saved_at: u64,
        max_bytes: usize,
        deadline: Instant,
    ) -> io::Result<()> {
        check_time(deadline)?;
        let frame = serde_json::to_vec(&CacheFrame {
            version: 1,
            session: session.into(),
            saved_at,
            payload: payload.into(),
        })
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid advisor cache"))?;
        if frame.len() > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Advisor cache too large",
            ));
        }
        self.save_context(session, payload, saved_at, deadline)
    }

    pub fn remove_context(&self, session: &str, deadline: Instant) -> io::Result<()> {
        check_time(deadline)?;
        let directory = match child_directory(&self.directory, "advisor-context", false) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            result => result?,
        };
        match unlinkat(&directory, cache_name(session), AtFlags::empty()) {
            Ok(()) | Err(rustix::io::Errno::NOENT) => check_time(deadline),
            Err(error) => Err(error.into()),
        }
    }

    pub fn open(base: &Path, deadline: Instant) -> io::Result<Self> {
        check_time(deadline)?;
        DirBuilder::new().recursive(true).mode(0o700).create(base)?;
        let base = File::from(open(
            base,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        let directory = child_directory(&base, "command-guardian", true)?;
        check_time(deadline)?;
        Ok(Self { directory })
    }

    pub fn save_context(
        &self,
        session: &str,
        payload: &[u8],
        saved_at: u64,
        deadline: Instant,
    ) -> io::Result<()> {
        check_time(deadline)?;
        let frame = serde_json::to_vec(&CacheFrame {
            version: 1,
            session: session.into(),
            saved_at,
            payload: payload.into(),
        })
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid advisor cache"))?;
        let directory = child_directory(&self.directory, "advisor-context", true)?;
        let mut file = File::from(openat(
            &directory,
            cache_name(session),
            OFlags::WRONLY | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::from_bits_truncate(0o600),
        )?);
        verify(&file, false)?;
        check_time(deadline)?;
        file.set_len(0)?;
        file.write_all(&frame)?;
        check_time(deadline)
    }

    pub fn load_context(
        &self,
        session: &str,
        now: u64,
        ttl: u64,
        max_bytes: usize,
        deadline: Instant,
    ) -> io::Result<Option<Vec<u8>>> {
        check_time(deadline)?;
        let directory = match child_directory(&self.directory, "advisor-context", false) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            result => result?,
        };
        let file = match openat(
            &directory,
            cache_name(session),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            result => File::from(result?),
        };
        verify(&file, false)?;
        let read_limit = (max_bytes as u64).checked_add(1).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Advisor cache size overflow")
        })?;
        if file.metadata()?.len() > max_bytes as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Advisor cache too large",
            ));
        }
        let mut bytes = Vec::new();
        file.take(read_limit).read_to_end(&mut bytes)?;
        check_time(deadline)?;
        if bytes.len() > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Advisor cache too large",
            ));
        }
        let frame: CacheFrame = serde_json::from_slice(&bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid advisor cache"))?;
        if frame.version != 1 || frame.session != session || frame.payload.len() > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid advisor cache",
            ));
        }
        let Some(age) = now.checked_sub(frame.saved_at) else {
            return Ok(None);
        };
        if age >= ttl {
            let _ = unlinkat(&directory, cache_name(session), AtFlags::empty());
            check_time(deadline)?;
            return Ok(None);
        }
        Ok(Some(frame.payload))
    }
}
