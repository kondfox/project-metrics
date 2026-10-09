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

pub const PROJECT_SCHEMA: &str = "pmx.project/1";
pub const LEADS_SCHEMA: &str = "pmx.leads/1";
/// Buckets with fewer denominator events are flagged low-n (spec §1.1).
pub const LOW_N_THRESHOLD: u64 = 10;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RepoInfo {
    pub name: String,
    /// A stack role or `per-file`.
    pub role: String,
    /// The measured tip.
    pub sha: String,
    pub tip_date: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub range_start: NaiveDate,
    pub breadth_roles: Vec<Role>,
    pub ai_attribution: bool,
    pub repos: Vec<RepoInfo>,
}

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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Detail {
    pub stack_mix: BTreeMap<Role, f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectFile {
    pub schema: String,
    pub project: ProjectInfo,
    /// Sparse per-day components; people are pseudonymous ids (`p1`, `p2`, …).
    pub days: Days,
    pub snapshots: BTreeMap<String, Value>,
    pub weeks: Vec<String>,
    pub months: Vec<String>,
    pub series_weekly: BTreeMap<String, Vec<Option<f64>>>,
    pub n_weekly: BTreeMap<String, Vec<Option<u64>>>,
    pub series_monthly: BTreeMap<String, Vec<Option<f64>>>,
    pub n_monthly: BTreeMap<String, Vec<Option<u64>>>,
    pub detail: BTreeMap<String, Detail>,
    pub ai_compare: Option<AiCompare>,
    pub security: Option<Value>,
    pub hotspots: Option<Value>,
    pub velocity: Option<Value>,
    pub meta: Meta,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeadsFile {
    pub schema: String,
    pub project: String,
    /// Pseudonymous id in `project.json` → person.
    pub people: BTreeMap<String, String>,
    /// Bucket label → multi-stack leaders.
    pub leaders: BTreeMap<String, Vec<Leader>>,
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
}

pub struct Series {
    pub labels: Vec<String>,
    pub values: BTreeMap<String, Vec<Option<f64>>>,
    pub n: BTreeMap<String, Vec<Option<u64>>>,
    pub detail: BTreeMap<String, Detail>,
    pub leaders: BTreeMap<String, Vec<Leader>>,
}

/// Compute every metric for each bucket.
pub fn series(days: &Days, buckets: &[Bucket], opts: &MetricOptions) -> Series {
    let mut s = Series {
        labels: buckets.iter().map(|b| b.label.clone()).collect(),
        values: BTreeMap::new(),
        n: BTreeMap::new(),
        detail: BTreeMap::new(),
        leaders: BTreeMap::new(),
    };
    for b in buckets {
        let m = metrics(&aggregate(days, b.start, b.end), opts);
        for (id, v) in m.values {
            s.values.entry(id).or_default().push(v);
        }
        for (id, n) in m.n {
            s.n.entry(id).or_default().push(n);
        }
        s.detail.insert(b.label.clone(), Detail { stack_mix: m.stack_mix });
        if !m.leaders.is_empty() {
            s.leaders.insert(b.label.clone(), m.leaders);
        }
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
    } = input;
    let opts = MetricOptions {
        breadth_roles: project.breadth_roles.clone(),
        ai_attribution: project.ai_attribution,
        mix_roles: mix_roles(&days),
    };
    let monthly = series(&days, &months(project.range_start, as_of), &opts);
    let weekly = series(&days, &weeks(project.range_start, as_of), &opts);
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
        quality_constituents: vec!["rework".into(), "tests".into(), "docs".into()],
        not_measured: [
            "duplication",
            "complexity",
            "security",
            "sast",
            "secrets",
            "pull_requests",
            "velocity",
        ]
        .map(String::from)
        .to_vec(),
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
        snapshots: BTreeMap::new(),
        weeks: weekly.labels,
        months: monthly.labels,
        series_weekly: weekly.values,
        n_weekly: weekly.n,
        series_monthly: monthly.values,
        n_monthly: monthly.n,
        detail,
        ai_compare: ai,
        security: None,
        hotspots: None,
        velocity: None,
        meta,
    };
    (file, leads)
}
