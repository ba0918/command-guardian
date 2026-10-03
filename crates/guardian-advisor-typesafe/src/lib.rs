use guardian_advisor::{AdvisorClient, Assessment, Failure, RawDistribution};
use guardian_advisor::{Role, ScopeEvidence, SendPermit, Skip, State};
use serde_json::{Map, Value, json};
use std::io::Read;
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;

const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const RESPONSE_LIMIT: usize = 65536;

#[derive(Clone)]
struct Response {
    status: u16,
    body: Vec<u8>,
}

trait Transport {
    fn post(
        &mut self,
        body: &[u8],
        authorization: &str,
        remaining: Duration,
    ) -> Result<Response, Failure>;
}

struct HttpTransport {
    endpoint: String,
    https_only: bool,
}

impl Transport for HttpTransport {
    fn post(
        &mut self,
        body: &[u8],
        authorization: &str,
        remaining: Duration,
    ) -> Result<Response, Failure> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .https_only(self.https_only)
            .proxy(None)
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_global(Some(remaining))
            .build()
            .into();
        let mut response = agent
            .post(&self.endpoint)
            .header("Authorization", authorization)
            .header("Content-Type", "application/json")
            .send(body)
            .map_err(map_error)?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Ok(Response {
                status,
                body: Vec::new(),
            });
        }
        let mut body = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(RESPONSE_LIMIT as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::TimedOut {
                    Failure::Timeout
                } else {
                    Failure::Communication
                }
            })?;
        Ok(Response { status, body })
    }
}

fn map_error(error: ureq::Error) -> Failure {
    match error {
        ureq::Error::Timeout(_) => Failure::Timeout,
        _ => Failure::Communication,
    }
}

struct Client<T> {
    transport: T,
    key: Option<String>,
    sent: bool,
}

impl<T: Transport> AdvisorClient for Client<T> {
    fn assess(&mut self, request: &SendPermit, remaining: Duration) -> Result<Assessment, Failure> {
        if remaining.is_zero() {
            return Err(Failure::Timeout);
        }
        if self.sent {
            return Err(Failure::ServiceResponse);
        }
        let key = self
            .key
            .as_deref()
            .filter(|k| !k.is_empty() && !k.chars().any(char::is_control))
            .ok_or(Failure::Authentication)?;
        let deadline = Instant::now()
            .checked_add(remaining)
            .ok_or(Failure::Timeout)?;
        self.sent = true;
        let response = self
            .transport
            .post(request.body(), &format!("Bearer {key}"), remaining)?;
        if Instant::now() >= deadline {
            return Err(Failure::Timeout);
        }
        if response.status == 401 || response.status == 403 {
            return Err(Failure::Authentication);
        }
        if !(200..300).contains(&response.status) {
            return Err(Failure::ServiceResponse);
        }
        let assessment = decode(&response.body)?;
        if Instant::now() >= deadline {
            return Err(Failure::Timeout);
        }
        Ok(assessment)
    }
}

pub struct TypeSafeClient {
    client: Client<HttpTransport>,
}

impl TypeSafeClient {
    pub fn from_env() -> Self {
        Self {
            client: Client {
                transport: HttpTransport {
                    endpoint: ENDPOINT.into(),
                    https_only: true,
                },
                key: std::env::var("TYPESAFE_API_KEY").ok(),
                sent: false,
            },
        }
    }
}

impl AdvisorClient for TypeSafeClient {
    fn assess(&mut self, request: &SendPermit, remaining: Duration) -> Result<Assessment, Failure> {
        self.client.assess(request, remaining)
    }
}

fn decode(body: &[u8]) -> Result<Assessment, Failure> {
    if body.len() > RESPONSE_LIMIT {
        return Err(Failure::InvalidResponse);
    }
    let text = std::str::from_utf8(body).map_err(|_| Failure::InvalidResponse)?;
    let value = guardian_advisor::parse_unique_json(text).map_err(|_| Failure::InvalidResponse)?;
    let answers = value["answers"]
        .as_object()
        .ok_or(Failure::InvalidResponse)?;
    if answers.len() != 2 {
        return Err(Failure::InvalidResponse);
    }
    Assessment::validate(
        distribution(&value["answers"]["risk"])?,
        distribution(&value["answers"]["scope"])?,
    )
}

fn distribution(value: &Value) -> Result<RawDistribution, Failure> {
    if value["type"] != "choice" {
        return Err(Failure::InvalidResponse);
    }
    let selected = value["choice"]
        .as_str()
        .ok_or(Failure::InvalidResponse)?
        .into();
    let probabilities = value["probabilities"]
        .as_object()
        .ok_or(Failure::InvalidResponse)?
        .iter()
        .map(|(key, value)| {
            value
                .as_f64()
                .map(|p| (key.clone(), p))
                .ok_or(Failure::InvalidResponse)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RawDistribution {
        selected,
        probabilities,
    })
}

pub fn prepare(state: &State, model: &str, max_bytes: usize) -> Result<SendPermit, Skip> {
    let questions = state.questions();
    let risk: Map<String, Value> = guardian_advisor::Risk::LABELS
        .iter()
        .map(|s| ((*s).into(), Value::Null))
        .collect();
    let scope: Map<String, Value> = guardian_advisor::Scope::LABELS
        .iter()
        .map(|s| ((*s).into(), Value::Null))
        .collect();
    let context = &state.context;
    let messages: Vec<_> = context
        .messages
        .iter()
        .map(|m| {
            json!({
                "role": if m.role == Role::User { "user" } else { "assistant" },
                "relative_order": m.relative_order,
                "text": m.text,
            })
        })
        .collect();
    let body = json!({
        "model": model,
        "questions": {
            "risk": { "type": "choice", "instructions": questions.risk, "criteria": risk },
            "scope": { "type": "choice", "instructions": questions.scope, "criteria": scope },
        },
        "state": {
            "command": state.command, "cwd": state.cwd, "machine": state.machine.as_str(), "reasons": state.reasons,
            "context": { "messages": messages,
                "source_verified": context.evidence == ScopeEvidence::Confirmed,
                "excluded_reference_material": context.excluded_reference_material,
                "window_may_omit_constraints": context.window_may_omit_constraints,
                "human_origin_not_proven": true,
            },
        },
    });
    let body = serde_json::to_vec(&body).map_err(|_| Skip::Encoding)?;
    SendPermit::checked(body, model.into(), max_bytes)
}
