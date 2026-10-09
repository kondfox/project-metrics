//! Roll-ups, ratios and scores (metrics spec §1–§6). No I/O.

pub mod day;
pub mod metrics;
pub mod model;
pub mod period;
pub mod snapshot;
pub mod stats;

pub use day::{Day, Days, Group, aggregate, merge_days};
pub use metrics::{AiCompare, Leader, MetricOptions, Metrics, MultiStack, metrics, multistack};
pub use model::{BuildInput, LeadsFile, ProjectFile, build};
pub use period::Bucket;
