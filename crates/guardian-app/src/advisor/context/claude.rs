use crate::state::AdvisorState;
use guardian_advisor::{ContentBlock, ContextMessage, ContextWindow, Role, bounded_context};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::time::Instant;

#[derive(Clone, Copy)]
pub struct CacheLimits {
    pub max_bytes: usize,
    pub exchanges: usize,
    pub ttl: u64,
    pub now: u64,
    pub deadline: Instant,
}

pub fn acquire_claude(hook: &Value, state: &AdvisorState, limits: CacheLimits) -> ContextWindow {
    if limits.exchanges == 0 || Instant::now() >= limits.deadline {
        return ContextWindow::default();
    }
    let context = match super::read_transcript(hook, limits.max_bytes, limits.deadline) {
        Ok(bytes) => claude_context(hook, &bytes, limits.max_bytes, limits.exchanges),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            load_claude_cache(state, hook, limits)
        }
        Err(_) => ContextWindow::default(),
    };
    if Instant::now() >= limits.deadline {
        ContextWindow::default()
    } else {
        context
    }
}

pub fn process_claude_event(state: &AdvisorState, hook: &Value, limits: CacheLimits) -> bool {
    if hook["hook_event_name"] != "UserPromptSubmit" && hook["hook_event_name"] != "MessageDisplay"
    {
        return false;
    }
    match super::read_transcript(hook, limits.max_bytes, limits.deadline) {
        Ok(bytes) => update_claude_cache(state, hook, &bytes, limits),
        Err(_) => {
            if let Some(session) = hook["session_id"].as_str() {
                let _ = state.remove_context(session, limits.deadline);
            }
            false
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedMessage {
    role: String,
    text: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedWindow {
    version: u8,
    source_version: String,
    session: String,
    path: String,
    prompt: String,
    display: Option<String>,
    excluded: bool,
    messages: Vec<CachedMessage>,
}

fn identity(hook: &Value) -> Option<(&str, &str, &str)> {
    Some((
        hook["session_id"].as_str().filter(|s| !s.is_empty())?,
        hook["prompt_id"].as_str().filter(|s| !s.is_empty())?,
        hook["transcript_path"].as_str().filter(|s| !s.is_empty())?,
    ))
}

fn read_cache(state: &AdvisorState, hook: &Value, limits: CacheLimits) -> Option<CachedWindow> {
    if limits.exchanges == 0 || Instant::now() >= limits.deadline {
        return None;
    }
    let (session, prompt, path) = identity(hook)?;
    let bytes = state
        .load_context(
            session,
            limits.now,
            limits.ttl,
            limits.max_bytes,
            limits.deadline,
        )
        .ok()??;
    let value = super::super::json::from_str(std::str::from_utf8(&bytes).ok()?).ok()?;
    let window: CachedWindow = serde_json::from_value(value).ok()?;
    if window.version != 1
        || window.source_version != "2.1.288"
        || window.session != session
        || window.prompt != prompt
        || window.path != path
        || window
            .messages
            .iter()
            .any(|m| m.role != "user" && m.role != "assistant")
    {
        return None;
    }
    Some(window)
}

pub fn load_claude_cache(state: &AdvisorState, hook: &Value, limits: CacheLimits) -> ContextWindow {
    if hook["hook_event_name"] != "PreToolUse"
        || hook["tool_use_id"].as_str().is_none_or(|s| s.is_empty())
    {
        return ContextWindow::default();
    }
    let Some(window) = read_cache(state, hook, limits) else {
        return ContextWindow::default();
    };
    window_context(&window, limits.exchanges)
}

fn window_context(window: &CachedWindow, exchanges: usize) -> ContextWindow {
    let messages: Vec<_> = window
        .messages
        .iter()
        .enumerate()
        .map(|(i, m)| ContextMessage {
            session: window.session.clone(),
            message_id: format!("cached-{i}"),
            order: i as u64,
            role: if m.role == "user" {
                Role::User
            } else {
                Role::Assistant
            },
            blocks: vec![ContentBlock::Text(m.text.clone())],
            known_synthetic: false,
        })
        .collect();
    let mut context = bounded_context(
        &messages,
        &window.session,
        messages.len() as u64,
        true,
        exchanges,
    );
    context.excluded_reference_material |= window.excluded;
    if context.excluded_reference_material {
        context.evidence = guardian_advisor::ScopeEvidence::Unavailable;
    }
    context
}

pub fn update_claude_cache(
    state: &AdvisorState,
    hook: &Value,
    bytes: &[u8],
    limits: CacheLimits,
) -> bool {
    let Some((session, _, _)) = identity(hook) else {
        return false;
    };
    // Invalidate before any new snapshot is adopted; a failed update never reads the previous window as fallback.
    let previous = if hook["hook_event_name"] == "MessageDisplay" {
        read_cache(state, hook, limits)
    } else {
        None
    };
    if state.remove_context(session, limits.deadline).is_err() {
        return false;
    }
    let update = || -> Option<()> {
        if bytes.len() > limits.max_bytes
            || limits.exchanges == 0
            || Instant::now() >= limits.deadline
        {
            return None;
        }
        let (session, prompt, path) = identity(hook)?;
        let source = records(bytes, session)?;
        if source.is_empty() {
            return None;
        }
        let mut window = match hook["hook_event_name"].as_str()? {
            "UserPromptSubmit" => {
                if source.iter().any(|r| r["promptId"] == prompt) {
                    return None;
                }
                let mut messages = Vec::new();
                for (order, record) in source.iter().enumerate() {
                    if let Some(value) = message(record, session, order as u64)? {
                        messages.push(value);
                    }
                }
                messages.push(ContextMessage {
                    session: session.into(),
                    message_id: prompt.into(),
                    order: source.len() as u64,
                    role: Role::User,
                    blocks: vec![ContentBlock::Text(hook["prompt"].as_str()?.into())],
                    known_synthetic: false,
                });
                let context = bounded_context(
                    &messages,
                    session,
                    source.len() as u64 + 1,
                    true,
                    limits.exchanges,
                );
                CachedWindow {
                    version: 1,
                    source_version: "2.1.288".into(),
                    session: session.into(),
                    path: path.into(),
                    prompt: prompt.into(),
                    display: None,
                    excluded: context.excluded_reference_material,
                    messages: context
                        .messages
                        .into_iter()
                        .map(|m| CachedMessage {
                            role: if m.role == Role::User {
                                "user"
                            } else {
                                "assistant"
                            }
                            .into(),
                            text: m.text,
                        })
                        .collect(),
                }
            }
            "MessageDisplay" => {
                let mut window = previous?;
                let id = hook["message_id"].as_str().filter(|s| !s.is_empty())?;
                hook["turn_id"].as_str().filter(|s| !s.is_empty())?;
                if hook["final"] != true || hook["index"] != 0 || window.display.is_some() {
                    return None;
                }
                window.messages.push(CachedMessage {
                    role: "assistant".into(),
                    text: hook["delta"].as_str()?.into(),
                });
                window.display = Some(id.into());
                window
            }
            _ => return None,
        };
        let context = window_context(&window, limits.exchanges);
        window.messages = context
            .messages
            .into_iter()
            .map(|m| CachedMessage {
                role: if m.role == Role::User {
                    "user"
                } else {
                    "assistant"
                }
                .into(),
                text: m.text,
            })
            .collect();
        let payload = serde_json::to_vec(&window).ok()?;
        state
            .save_context_with_limit(
                session,
                &payload,
                limits.now,
                limits.max_bytes,
                limits.deadline,
            )
            .ok()?;
        Some(())
    };
    update().is_some()
}

pub fn claude_context(
    hook: &Value,
    bytes: &[u8],
    max_bytes: usize,
    exchanges: usize,
) -> ContextWindow {
    parse(hook, bytes, max_bytes, exchanges).unwrap_or_default()
}

fn parse(hook: &Value, bytes: &[u8], max_bytes: usize, exchanges: usize) -> Option<ContextWindow> {
    if bytes.len() > max_bytes || exchanges == 0 || hook["hook_event_name"] != "PreToolUse" {
        return None;
    }
    let session = hook["session_id"].as_str().filter(|s| !s.is_empty())?;
    let prompt = hook["prompt_id"].as_str().filter(|s| !s.is_empty())?;
    let call = hook["tool_use_id"].as_str().filter(|s| !s.is_empty())?;
    let records = records(bytes, session)?;
    let mut messages = Vec::new();
    let mut current_prompt = false;
    let mut before = None;
    for (order, record) in records.iter().enumerate() {
        if record["type"] == "user" && record["message"]["content"].is_string() {
            current_prompt = record["promptId"] == prompt;
        }
        if current_prompt
            && record["type"] == "assistant"
            && record["message"]["content"]
                .as_array()
                .is_some_and(|blocks| {
                    blocks
                        .iter()
                        .any(|b| b["type"] == "tool_use" && b["id"] == call)
                })
            && before.replace(order).is_some()
        {
            return None;
        }
        if let Some(message) = message(record, session, order as u64)? {
            messages.push(message);
        }
    }
    Some(bounded_context(
        &messages,
        session,
        before? as u64,
        true,
        exchanges,
    ))
}

fn records(bytes: &[u8], session: &str) -> Option<Vec<Value>> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut records = Vec::new();
    let mut ids = HashSet::new();
    let mut parent = Value::Null;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let record = super::super::json::from_str(line).ok()?;
        if record.get("uuid").is_none() {
            continue;
        }
        let id = record["uuid"].as_str().filter(|s| !s.is_empty())?;
        if !ids.insert(id.to_owned())
            || record["sessionId"] != session
            || record["version"] != "2.1.288"
            || record["isSidechain"] != false
            || record["isMeta"] == true
            || record["parentUuid"] != parent
        {
            return None;
        }
        parent = record["uuid"].clone();
        records.push(record);
    }
    Some(records)
}

fn message(record: &Value, session: &str, order: u64) -> Option<Option<ContextMessage>> {
    let role = match record["type"].as_str()? {
        "user" if record["message"]["role"] == "user" => Role::User,
        "assistant" if record["message"]["role"] == "assistant" => Role::Assistant,
        _ => return Some(None),
    };
    let content = &record["message"]["content"];
    let blocks = if let Some(text) = content.as_str() {
        // This supported producer emits submitted prompts as strings; user arrays include tool results.
        record["promptId"].as_str().filter(|s| !s.is_empty())?;
        vec![ContentBlock::Text(text.into())]
    } else {
        content
            .as_array()?
            .iter()
            .map(|b| match b["type"].as_str() {
                Some("text") if role == Role::Assistant => {
                    b["text"].as_str().map(|t| ContentBlock::Text(t.into()))
                }
                Some("tool_use" | "tool_result") => Some(ContentBlock::Tool(String::new())),
                _ => Some(ContentBlock::File(String::new())),
            })
            .collect::<Option<Vec<_>>>()?
    };
    Some(Some(ContextMessage {
        session: session.into(),
        message_id: record["uuid"].as_str()?.into(),
        order,
        role,
        blocks,
        known_synthetic: role == Role::User && !content.is_string(),
    }))
}
