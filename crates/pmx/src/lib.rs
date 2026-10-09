//! The `pmx` pipeline as a library (used by the CLI and the parity harness).

pub mod cache;
pub mod check;
pub mod collect;
pub mod dashboard;
pub mod demo;
pub mod export;
pub mod ingest;
pub mod people;
pub mod setup;

pub use collect::{CollectOptions, Collected, collect, write_outputs};
