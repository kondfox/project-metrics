//! The output data model (spec §10.6): `project.json` (team level, shareable) and
//! `private/leads.json` (per-person drill-downs, lead only).

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use pm_classify::Role;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::day::{Day, Days, aggregate};
use crate::metrics::{AiCompare, Leader, MetricOptions, ai_compare, metrics};
use crate::period::{Bucket, last_complete_month, months, partial_month, partial_week, weeks};
use crate::snapshot::{SnapshotPoint, VulnPackage, at_or_before, sast_diff};

pub const PROJECT_SCHEMA: &str = "pmx.project/1";
pub const LEADS_SCHEMA: &str = "pmx.leads/1";
/// Buckets with fewer denominator events are flagged low-n (spec §1.1).
pub const LOW_N_THRESHOLD: u64 = 10;

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RepoInfo {
    pub name: String,
    /// A stack role or `per-file`.
    pub role: String,
    /// The measured tip.
    pub sha: String,
    pub tip_date: String,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub range_start: NaiveDate,
    pub breadth_roles: Vec<Role>,
    pub ai_attribution: bool,
    pub repos: Vec<RepoInfo>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meta {
    pub generated_at: String,
    pub as_of: NaiveDate,
    pub partial_week: bool,
    pub partial_month: bool,
    /// The default headline window: the last complete month.
    pub headline_month: String,
    /// Latest commit date on or before `as_of`, by any author (the AI comparison's window end).
    pub latest_commit: Option<NaiveDate>,
    pub low_n_threshold: u64,
    pub tool_versions: BTreeMap<String, String>,
    /// `spec`, or `prototype` for a parity-harness run.
    pub dialect: String,
    /// Quality constituents that can feed the score in this file.
    pub quality_constituents: Vec<String>,
    /// Inputs this version does not measure yet; shown as "not measured".
    pub not_measured: Vec<String>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Detail {
    pub stack_mix: BTreeMap<Role, f64>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectFile {
    pub schema: String,
    pub project: ProjectInfo,
    /// Sparse per-day components; people are pseudonymous ids (`p1`, `p2`, …).
    /// Opaque to the dashboard: only the WASM engine reads it.
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub days: Days,
    /// Pooled snapshot per measured date (week-ends, month-ends, the as-of tip).
    pub snapshots: BTreeMap<NaiveDate, SnapshotPoint>,
    pub weeks: Vec<String>,
    pub months: Vec<String>,
    pub series_weekly: BTreeMap<String, Vec<Option<f64>>>,
    pub n_weekly: BTreeMap<String, Vec<Option<u64>>>,
    pub series_monthly: BTreeMap<String, Vec<Option<f64>>>,
    pub n_monthly: BTreeMap<String, Vec<Option<u64>>>,
    pub detail: BTreeMap<String, Detail>,
    pub ai_compare: Option<AiCompare>,
    pub security: Option<SecurityDetail>,
    pub hotspots: Vec<RepoHotspots>,
    pub velocity: Option<Value>,
    pub meta: Meta,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeadsFile {
    pub schema: String,
    pub project: String,
    /// Pseudonymous id in `project.json` → person.
    pub people: BTreeMap<String, String>,
    /// Bucket label → multi-stack leaders.
    pub leaders: BTreeMap<String, Vec<Leader>>,
}

/// Dependency-vulnerability drill-down at the latest snapshot (spec §7.1).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SecurityDetail {
    pub date: NaiveDate,
    /// The 8 packages with the most advisories.
    pub top_packages: Vec<VulnPackage>,
    /// Repo → scanned lockfiles.
    pub lockfiles: BTreeMap<String, Vec<String>>,
}

/// Churn × complexity per file at the as-of commit, ranked within the repo (spec §3.5).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotspot {
    pub file: String,
    pub score: u64,
    pub revisions: u64,
    pub complexity: u64,
    pub code: u64,
    pub churn: u64,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoHotspots {
    pub repo: String,
    pub sha: String,
    /// Revisions and churn count from this date.
    pub since: NaiveDate,
    pub files: Vec<Hotspot>,
    pub files_changed: u64,
}

/// Snapshot results for the roll-up.
#[derive(Clone, Debug, Default)]
pub struct SnapshotInput {
    pub points: BTreeMap<NaiveDate, SnapshotPoint>,
    /// SAST keys per date, for new/fixed (`None` where SAST isn't measured).
    pub sast_keys: BTreeMap<NaiveDate, Option<BTreeMap<String, u32>>>,
    pub security: Option<SecurityDetail>,
    pub hotspots: Vec<RepoHotspots>,
    /// Inputs that could not be measured (missing tools, no SAST rules).
    pub not_measured: Vec<String>,
}

pub struct BuildInput {
    pub project: ProjectInfo,
    /// Per-day components with real person names, already limited to `[range_start, as_of]`.
    pub days: Days,
    pub as_of: NaiveDate,
    pub latest_commit: Option<NaiveDate>,
    pub generated_at: String,
    pub tool_versions: BTreeMap<String, String>,
    pub dialect: String,
    pub snapshots: SnapshotInput,
}

pub struct Series {
    pub labels: Vec<String>,
    pub values: BTreeMap<String, Vec<Option<f64>>>,
    pub n: BTreeMap<String, Vec<Option<u64>>>,
    pub detail: BTreeMap<String, Detail>,
    pub leaders: BTreeMap<String, Vec<Leader>>,
}

/// Compute every metric for each bucket.
pub fn series(
    days: &Days,
    buckets: &[Bucket],
    opts: &MetricOptions,
    snaps: &SnapshotInput,
    as_of: NaiveDate,
) -> Series {
    let mut s = Series {
        labels: buckets.iter().map(|b| b.label.clone()).collect(),
        values: BTreeMap::new(),
        n: BTreeMap::new(),
        detail: BTreeMap::new(),
        leaders: BTreeMap::new(),
    };
    let mut previous_keys: Option<&BTreeMap<String, u32>> = None;
    for (i, b) in buckets.iter().enumerate() {
        let end = b.end.min(as_of);
        let mut m = metrics(&aggregate(days, b.start, b.end), opts, at_or_before(&snaps.points, end));
        // New / fixed SAST findings against the previous bucket's snapshot (spec §7.4).
        let keys = snaps.sast_keys.range(..=end).next_back().and_then(|(_, k)| k.as_ref());
        let (new, fixed) = match (keys, previous_keys) {
            (Some(k), Some(p)) if i > 0 => {
                let (n, f) = sast_diff(k, p);
                (Some(n), Some(f))
            }
            _ => (None, None),
        };
        if !snaps.points.is_empty() {
            m.values.insert("sast_new".into(), new);
            m.values.insert("sast_fixed".into(), fixed);
            m.n.insert("sast_new".into(), None);
            m.n.insert("sast_fixed".into(), None);
        }
        previous_keys = keys;
        // Every metric gets exactly one value per bucket: a metric a bucket lacks (e.g. a
        // snapshot metric before the first snapshot) is null there, so series stay aligned.
        for (id, v) in m.values {
            let col = s.values.entry(id).or_default();
            col.resize(i, None);
            col.push(v);
        }
        for (id, n) in m.n {
            let col = s.n.entry(id).or_default();
            col.resize(i, None);
            col.push(n);
        }
        s.detail.insert(b.label.clone(), Detail { stack_mix: m.stack_mix });
        if !m.leaders.is_empty() {
            s.leaders.insert(b.label.clone(), m.leaders);
        }
    }
    for col in s.values.values_mut() {
        col.resize(buckets.len(), None);
    }
    for col in s.n.values_mut() {
        col.resize(buckets.len(), None);
    }
    s
}

/// Roles with added lines anywhere in the project, in the spec's order.
pub fn mix_roles(days: &Days) -> Vec<Role> {
    let present: BTreeSet<Role> = days
        .values()
        .flat_map(|d| d.lines.values())
        .flat_map(|roles| {
            roles
                .iter()
                .filter(|(_, t)| t.values().any(|n| *n > 0))
                .map(|(r, _)| *r)
        })
        .collect();
    Role::ALL.into_iter().filter(|r| present.contains(r)).collect()
}

/// Stable pseudonyms: people sorted by name get `p1`, `p2`, …
pub fn pseudonyms(days: &Days) -> BTreeMap<String, String> {
    let names: BTreeSet<&String> = days
        .values()
        .flat_map(|d| d.person_commits.keys().chain(d.lines.keys()))
        .collect();
    names
        .into_iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), format!("p{}", i + 1)))
        .collect()
}

pub fn build(input: BuildInput) -> (ProjectFile, LeadsFile) {
    let BuildInput {
        project,
        days,
        as_of,
        latest_commit,
        generated_at,
        tool_versions,
        dialect,
        snapshots,
    } = input;
    let opts = MetricOptions {
        breadth_roles: project.breadth_roles.clone(),
        ai_attribution: project.ai_attribution,
        mix_roles: mix_roles(&days),
    };
    let monthly = series(&days, &months(project.range_start, as_of), &opts, &snapshots, as_of);
    let weekly = series(&days, &weeks(project.range_start, as_of), &opts, &snapshots, as_of);
    let ai = match (project.ai_attribution, latest_commit) {
        (true, Some(end)) => Some(ai_compare(&days, end)),
        _ => None,
    };

    let ids = pseudonyms(&days);
    let pseudo_days: Days = days
        .iter()
        .map(|(d, day)| (*d, day.rename_people(|p| ids[p].clone())))
        .filter(|(_, day): &(NaiveDate, Day)| !day.is_empty())
        .collect();
    let mut detail = monthly.detail;
    detail.extend(weekly.detail);
    let mut leaders = monthly.leaders;
    leaders.extend(weekly.leaders);

    let meta = Meta {
        generated_at,
        as_of,
        partial_week: partial_week(as_of),
        partial_month: partial_month(as_of),
        headline_month: last_complete_month(as_of).label,
        latest_commit,
        low_n_threshold: LOW_N_THRESHOLD,
        tool_versions,
        dialect,
        quality_constituents: {
            let mut q = vec!["rework".to_string(), "tests".into(), "docs".into()];
            if !snapshots.not_measured.iter().any(|x| x == "duplication") && !snapshots.points.is_empty() {
                q.push("duplication".into());
            }
            q
        },
        not_measured: {
            let mut nm = snapshots.not_measured.clone();
            nm.extend(["pull_requests".to_string(), "velocity".into()]);
            nm
        },
    };
    let leads = LeadsFile {
        schema: LEADS_SCHEMA.into(),
        project: project.name.clone(),
        people: ids.iter().map(|(name, id)| (id.clone(), name.clone())).collect(),
        leaders,
    };
    let file = ProjectFile {
        schema: PROJECT_SCHEMA.into(),
        project,
        days: pseudo_days,
        snapshots: snapshots.points,
        weeks: weekly.labels,
        months: monthly.labels,
        series_weekly: weekly.values,
        n_weekly: weekly.n,
        series_monthly: monthly.values,
        n_monthly: monthly.n,
        detail,
        ai_compare: ai,
        security: snapshots.security,
        hotspots: snapshots.hotspots,
        velocity: None,
        meta,
    };
    (file, leads)
}

/// `cargo test -p pm-metrics --features ts` regenerates the dashboard's types; CI fails if the
/// committed ones differ.
#[cfg(all(test, feature = "ts"))]
mod ts_export {
    use ts_rs::{Config, TS};

    #[test]
    fn export_dashboard_types() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../pmx/web/src/generated");
        // serde writes 64-bit integers as JSON numbers, and every count fits in 2^53.
        let cfg = Config::new().with_large_int("number").with_out_dir(dir);
        super::ProjectFile::export_all(&cfg).unwrap();
        super::LeadsFile::export_all(&cfg).unwrap();
        crate::Metrics::export_all(&cfg).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::SnapshotPoint;

    #[test]
    fn series_stay_aligned_when_snapshots_start_late() {
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let mut point = SnapshotPoint::default();
        point.values.insert("dup_pct".into(), Some(2.5));
        let snaps = SnapshotInput {
            points: [(d("2025-03-31"), point)].into(),
            ..Default::default()
        };
        let opts = MetricOptions {
            breadth_roles: vec![Role::Backend],
            ai_attribution: true,
            mix_roles: vec![],
        };
        let buckets = months(d("2025-01-01"), d("2025-04-30"));
        let s = series(&Days::new(), &buckets, &opts, &snaps, d("2025-04-30"));
        assert_eq!(s.values["dup_pct"], vec![None, None, Some(2.5), Some(2.5)]);
        assert!(s.values.values().all(|v| v.len() == 4));
        assert!(s.n.values().all(|v| v.len() == 4));
    }
}
