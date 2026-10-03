use guardian_advisor::{ContentBlock, ContextMessage, ContextWindow, Role, bounded_context};
use serde_json::Value;
use std::fs::File;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::time::Instant;

mod claude;
pub use claude::{
    CacheLimits, acquire_claude, claude_context, load_claude_cache, process_claude_event,
    update_claude_cache,
};

pub(super) fn read_transcript(
    hook: &Value,
    max_bytes: usize,
    deadline: Instant,
) -> std::io::Result<Vec<u8>> {
    use std::io::{Error, ErrorKind};
    if Instant::now() >= deadline {
        return Err(Error::from(ErrorKind::TimedOut));
    }
    let path = hook["transcript_path"]
        .as_str()
        .ok_or_else(|| Error::from(ErrorKind::InvalidInput))?;
    let file = File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.len() > max_bytes as u64
    {
        return Err(Error::from(ErrorKind::InvalidData));
    }
    let read_limit = (max_bytes as u64)
        .checked_add(1)
        .ok_or_else(|| Error::from(ErrorKind::InvalidInput))?;
    let mut bytes = Vec::new();
    file.take(read_limit).read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(Error::from(ErrorKind::InvalidData));
    }
    if Instant::now() >= deadline {
        return Err(Error::from(ErrorKind::TimedOut));
    }
    Ok(bytes)
}

pub fn acquire_codex(
    hook: &Value,
    max_frame_bytes: usize,
    exchanges: usize,
    deadline: Instant,
) -> ContextWindow {
    let acquire = || -> Option<ContextWindow> {
        if exchanges == 0 || Instant::now() >= deadline {
            return None;
        }
        let path = hook["transcript_path"].as_str()?;
        let file = File::from(
            rustix::fs::open(
                path,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::NONBLOCK
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )
            .ok()?,
        );
        let metadata = file.metadata().ok()?;
        if !metadata.is_file()
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.len() > max_frame_bytes as u64
        {
            return None;
        }
        let read_limit = (max_frame_bytes as u64).checked_add(1)?;
        let mut bytes = Vec::new();
        file.take(read_limit).read_to_end(&mut bytes).ok()?;
        if bytes.len() > max_frame_bytes || Instant::now() >= deadline {
            return None;
        }
        let context = codex_context(hook, &bytes, max_frame_bytes, exchanges);
        if Instant::now() >= deadline {
            return None;
        }
        Some(context)
    };
    acquire().unwrap_or_default()
}

pub fn codex_context(
    hook: &Value,
    bytes: &[u8],
    max_frame_bytes: usize,
    exchanges: usize,
) -> ContextWindow {
    parse_codex(hook, bytes, max_frame_bytes, exchanges).unwrap_or_default()
}

fn parse_codex(
    hook: &Value,
    bytes: &[u8],
    max_frame_bytes: usize,
    exchanges: usize,
) -> Option<ContextWindow> {
    if exchanges == 0 || bytes.len() > max_frame_bytes || hook["hook_event_name"] != "PreToolUse" {
        return None;
    }
    let session = hook["session_id"].as_str()?;
    let turn = hook["turn_id"].as_str()?;
    let call = hook["tool_use_id"].as_str()?;
    if session.is_empty() || turn.is_empty() || call.is_empty() {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let records = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(super::json::from_str)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let header = records.first()?;
    let source = &header["payload"];
    if header["type"] != "session_meta"
        || source["session_id"] != session
        || source["cli_version"] != "0.160.0"
        || source["source"] != "exec"
        || source["thread_source"] != "user"
    {
        return None;
    }
    let mut last_order = None;
    for record in &records {
        let order = record["ordinal"].as_u64()?;
        if last_order.is_some_and(|last| last >= order) {
            return None;
        }
        last_order = Some(order);
    }
    let mut boundaries = records.iter().enumerate().filter(|(_, record)| {
        let payload = &record["payload"];
        record["type"] == "response_item"
            && payload["type"] == "function_call"
            && payload["call_id"] == call
            && payload["internal_chat_message_metadata_passthrough"]["turn_id"] == turn
    });
    let before = boundaries.next()?.0;
    if boundaries.next().is_some() {
        return None;
    }
    let mut messages = Vec::new();
    for (index, record) in records[..before].iter().enumerate() {
        let payload = &record["payload"];
        if record["type"] != "response_item" || payload["type"] != "message" {
            continue;
        }
        let role = match payload["role"].as_str()? {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            _ => continue,
        };
        let metadata = &payload["internal_chat_message_metadata_passthrough"];
        let known_synthetic = role == Role::User
            && !metadata["content_item_kinds"]
                .as_array()
                .is_some_and(|kinds| {
                    !kinds.is_empty() && kinds.iter().all(|kind| kind == "user.text")
                });
        let blocks = payload["content"]
            .as_array()?
            .iter()
            .map(|block| match block["type"].as_str() {
                Some("input_text" | "output_text") => block["text"]
                    .as_str()
                    .map(|text| ContentBlock::Text(text.into())),
                Some("tool_use" | "tool_result") => Some(ContentBlock::Tool(String::new())),
                _ => Some(ContentBlock::File(String::new())),
            })
            .collect::<Option<Vec<_>>>()?;
        messages.push(ContextMessage {
            session: session.into(),
            message_id: payload["id"].as_str()?.into(),
            order: index as u64,
            role,
            blocks,
            known_synthetic,
        });
    }
    Some(bounded_context(
        &messages,
        session,
        before as u64,
        true,
        exchanges,
    ))
}
