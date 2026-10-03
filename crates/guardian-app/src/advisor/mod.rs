pub mod context;
pub mod control;
mod json;
pub mod log;
pub mod service;
pub mod wire;
pub mod worker;

#[derive(Debug)]
pub struct Evaluation {
    pub outcome: guardian_advisor::Outcome,
    pub reply: Option<wire::Reply>,
    pub failure: Option<guardian_advisor::Failure>,
    pub unreaped: bool,
    pub elapsed_ms: u128,
}
