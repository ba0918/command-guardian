use crate::ScopeEvidence;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentBlock {
    Text(String),
    Tool(String),
    File(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextMessage {
    pub session: String,
    pub message_id: String,
    pub order: u64,
    pub role: Role,
    pub blocks: Vec<ContentBlock>,
    pub known_synthetic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentMessage {
    pub role: Role,
    pub relative_order: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextWindow {
    pub messages: Vec<SentMessage>,
    pub evidence: ScopeEvidence,
    pub excluded_reference_material: bool,
    pub window_may_omit_constraints: bool,
}

impl Default for ContextWindow {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            evidence: ScopeEvidence::Unavailable,
            excluded_reference_material: false,
            window_may_omit_constraints: true,
        }
    }
}

pub fn bounded_context(
    input: &[ContextMessage],
    session: &str,
    before_order: u64,
    verified_origin: bool,
    exchanges: usize,
) -> ContextWindow {
    if !verified_origin || exchanges == 0 || session.is_empty() {
        return ContextWindow::default();
    }
    let mut eligible = Vec::new();
    for message in input {
        if message.session != session || message.order >= before_order || message.known_synthetic {
            continue;
        }
        if message.message_id.is_empty()
            || eligible.iter().any(|previous: &&ContextMessage| {
                previous.message_id == message.message_id || previous.order >= message.order
            })
        {
            return ContextWindow::default();
        }
        eligible.push(message);
    }
    let mut starts = Vec::new();
    for (index, message) in eligible.iter().enumerate() {
        let follows_user =
            index > 0 && eligible[index - 1].role == Role::User && message.role == Role::Assistant;
        if !follows_user {
            starts.push(index);
        }
    }
    let start = starts
        .get(starts.len().saturating_sub(exchanges))
        .copied()
        .unwrap_or(0);
    let mut result = ContextWindow::default();
    for message in &eligible[start..] {
        let mut text = String::new();
        for block in &message.blocks {
            match block {
                ContentBlock::Text(value) => {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(value);
                }
                ContentBlock::Tool(_) | ContentBlock::File(_) => {
                    result.excluded_reference_material = true;
                }
            }
        }
        if !text.is_empty() {
            result.messages.push(SentMessage {
                role: message.role,
                relative_order: result.messages.len(),
                text,
            });
        }
    }
    if !result.excluded_reference_material
        && result
            .messages
            .iter()
            .any(|message| message.role == Role::User)
    {
        result.evidence = ScopeEvidence::Confirmed;
    }
    result
}
