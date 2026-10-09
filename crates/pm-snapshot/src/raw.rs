//! Tool results before classification: what a tool said about one tree.

use pm_metrics::snapshot::Severity;
use serde::{Deserialize, Serialize};

/// scc totals (spec §3.5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SccTotals {
    pub code: u64,
    pub complexity: u64,
}

/// jscpd totals (spec §3.4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Duplication {
    pub duplicated_lines: u64,
    pub lines: u64,
}

/// A gitleaks hit; the secret itself is never kept.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretHit {
    pub rule: String,
    /// Tree-relative.
    pub file: String,
    pub line: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SastFinding {
    /// Last segment of the semgrep check id.
    pub rule: String,
    /// As semgrep reports it (`ERROR`, `WARNING`, `INFO`, …).
    pub severity: String,
    pub file: String,
    pub line: u64,
    pub cwe: Option<String>,
}

/// One osv-scanner package entry with its advisories.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsvPackage {
    pub name: String,
    pub version: String,
    /// Advisory id → severity (spec §7.1).
    pub vulns: Vec<(String, Severity)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockfileScan {
    pub path: String,
    pub packages: Vec<OsvPackage>,
}

/// Everything measured on one repo at one commit. `None` = not measured.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RawScan {
    pub sha: String,
    /// With the duplication excludes (complexity / KLOC).
    pub scc: Option<SccTotals>,
    /// Whole tree, in KLOC to 0.1 (SAST density).
    pub kloc_all: Option<f64>,
    pub duplication: Option<Duplication>,
    pub secrets: Option<Vec<SecretHit>>,
    /// `None` also when the scan was incomplete (rule timeouts, spec §7.4).
    pub sast: Option<Vec<SastFinding>>,
    pub suppressions: Option<u64>,
    pub lockfiles: Option<Vec<LockfileScan>>,
}
