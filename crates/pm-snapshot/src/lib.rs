//! Snapshot metrics tools (spec §3.4–3.5, §7, §10.5): find them, run them on a materialized tree,
//! parse their output into [`raw`] results, and summarize those into a
//! [`pm_metrics::snapshot::RepoSummary`]. Raw results don't depend on classification settings, so
//! they can be cached per tool and commit.

pub mod raw;
pub mod run;
pub mod summarize;
pub mod tools;

pub use raw::RawScan;
pub use summarize::{SecurityRules, summarize};
pub use tools::{Tool, ToolId, Tools};

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("cannot run {tool}: {source}")]
    Spawn { tool: String, source: std::io::Error },
    #[error("{tool} failed: {message}")]
    Failed { tool: String, message: String },
    #[error("{tool}: unexpected output: {message}")]
    Parse { tool: String, message: String },
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SnapshotError>;
