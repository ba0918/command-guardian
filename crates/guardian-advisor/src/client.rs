use crate::{Assessment, Failure};
use std::time::Duration;

pub struct SendPermit {
    pub(crate) body: Vec<u8>,
    pub(crate) model: String,
}

impl SendPermit {
    pub fn checked(body: Vec<u8>, model: String, max_bytes: usize) -> Result<Self, crate::Skip> {
        let text = std::str::from_utf8(&body).map_err(|_| crate::Skip::Encoding)?;
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|_| crate::Skip::Encoding)?;
        if crate::secrets::contains_secret(text) || crate::secrets::json_contains_secret(&value) {
            return Err(crate::Skip::Secret);
        }
        if body.len() > max_bytes {
            return Err(crate::Skip::Size);
        }
        Ok(Self { body, model })
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn model(&self) -> &str {
        &self.model
    }
}

pub trait AdvisorClient {
    fn assess(&mut self, request: &SendPermit, remaining: Duration) -> Result<Assessment, Failure>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{combine, Failure, Mode, ScopeEvidence};
    use guardian_core::Verdict;
    use std::time::Duration;

    struct Unavailable;
    impl AdvisorClient for Unavailable {
        fn assess(
            &mut self,
            _request: &SendPermit,
            _remaining: Duration,
        ) -> Result<crate::Assessment, Failure> {
            Err(Failure::Communication)
        }
    }

    // @kotowari[REQ-advisor-012, EX-advisor-023]
    #[test]
    fn classified_connection_failure_keeps_the_machine_result() {
        let request = SendPermit {
            body: b"{}".to_vec(),
            model: "fixture-model".into(),
        };
        let result = Unavailable.assess(&request, Duration::from_millis(100));
        assert_eq!(result, Err(Failure::Communication));
        for machine in [Verdict::Allow, Verdict::Ask] {
            assert_eq!(
                combine(
                    Mode::Enforce,
                    machine,
                    result.as_ref().ok(),
                    ScopeEvidence::Unavailable,
                    0.9
                )
                .final_verdict,
                machine
            );
        }
    }
}
