//! The LLM classifier (plan §6, spec §10.3): Jev's question/answer schema as the one contract for
//! every provider, the six-axis rubric as a versioned question set, and the providers.
//!
//! Counting and arithmetic stay in code ([`QuestionSet::axes`]); a model only answers questions.

pub mod prompt;
pub mod provider;
pub mod rubric;
pub mod schema;

pub use provider::{Provider, build, is_local, is_loopback};
pub use rubric::{Axes, Facts, QuestionSet};
pub use schema::{Answer, ConfidenceSource, Question, Questions, Request, Response, Usage};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("rubric: {0}")]
    Rubric(String),
    #[error("classifier config: {0}")]
    Config(String),
    #[error("cannot run the classifier: {0}")]
    Spawn(std::io::Error),
    #[error("classifier timed out after {0} s")]
    Timeout(u64),
    #[error("{0}")]
    Provider(String),
    #[error("bad answer: {0}")]
    Answer(String),
}

pub type Result<T> = std::result::Result<T, Error>;
