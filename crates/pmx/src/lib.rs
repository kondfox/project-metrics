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
pub mod snapshots;
pub mod tools_install;

/// Where `pmx tools install` puts external tools (searched before PATH). `PMX_TOOLS_DIR` overrides.
pub fn tools_dir() -> std::path::PathBuf {
    if let Some(d) = std::env::var_os("PMX_TOOLS_DIR") {
        return d.into();
    }
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("pmx")
        .join("tools")
}

/// Detect the external snapshot tools, preferring pmx's own installs.
pub fn detect_tools() -> pm_snapshot::Tools {
    pm_snapshot::Tools::detect(&[tools_dir().join("bin")])
}

pub use collect::{CollectOptions, Collected, collect, write_outputs};
