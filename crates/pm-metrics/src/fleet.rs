//! The local fleet (plan §3.4, M6): several projects' `project.json` on one page. Each project
//! keeps its own metrics; the fleet row pools numerators and denominators across projects (never
//! averaging their ratios). No I/O.
//!
//! Pooling rules (plan D33):
//! - **Flow ratios** (Quality and its constituents, multi-stack, active devs) pool every project.
//!   People are pseudonymous per project, so the same person in two projects counts twice.
//! - **AI-assisted** pools only the projects that attribute AI (stripped trailers would read as 0).
//! - **Peer review** pools only the projects whose PRs were measured.
//! - **Duplication** joins the fleet's Quality only when every project measured it at the range end.
//! - **Security** is a gate, so the fleet shows its worst project (quality-index.md §2), not a pool.
//! - **Velocity** pools points ÷ FTE-days only when every scored project used the same rubric and
//!   classifiers and the same denominator (all FTE, or all engineer-days); else it is not comparable.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use pm_classify::Role;
use serde::{Deserialize, Serialize};

use crate::day::{Day, Days, aggregate};
use crate::metrics::{MetricOptions, Metrics, metrics};
use crate::model::{LOW_N_THRESHOLD, ProjectFile, mix_roles};
use crate::period::{Bucket, last_complete_month, months, partial_month, partial_week, weeks};
use crate::snapshot::{SnapshotPoint, at_or_before, s_dup};
use crate::stats::pct;

pub const FLEET_SCHEMA: &str = "pmx.fleet/1";

/// One project as the fleet needs it: everything to compute its metrics over any range.
#[derive(Clone, Debug)]
pub struct Member {
    /// Unique within the fleet (the project's folder name).
    pub key: String,
    pub days: Days,
    pub snapshots: BTreeMap<NaiveDate, SnapshotPoint>,
    pub opts: MetricOptions,
    /// Rubric and classifiers that scored its Velocity (`None`: not measured). Velocity pools only
    /// across projects with the same basis.
    pub velocity_basis: Option<String>,
}

impl Member {
    pub fn from_project(key: &str, p: &ProjectFile) -> Member {
        Member {
            key: key.to_string(),
            opts: MetricOptions {
                breadth_roles: p.project.breadth_roles.clone(),
                ai_attribution: p.project.ai_attribution,
                mix_roles: mix_roles(&p.days),
                pull_requests: p.meta.pull_requests.is_some(),
                pr_size: p.meta.pull_requests.as_ref().is_some_and(|x| x.sizes),
                velocity: p.velocity.as_ref().map(|v| v.options.clone()),
            },
            days: p.days.clone(),
            snapshots: p.snapshots.clone(),
            velocity_basis: p.velocity.as_ref().map(|v| velocity_basis(&v.rubric, &v.classifiers)),
        }
    }

    /// This project's own metrics over `[start, end]`.
    pub fn metrics(&self, start: NaiveDate, end: NaiveDate) -> Metrics {
        metrics(
            &aggregate(&self.days, start, end),
            &self.opts,
            at_or_before(&self.snapshots, end),
            start,
            end,
        )
    }
}

pub fn velocity_basis(rubric: &str, classifiers: &[String]) -> String {
    let mut c = classifiers.to_vec();
    c.sort();
    format!("{rubric} · {}", c.join(", "))
}

/// The fleet row of one range: pooled metrics plus which projects fed each headline.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Pooled {
    pub values: BTreeMap<String, Option<f64>>,
    pub n: BTreeMap<String, Option<u64>>,
    /// Headline (`quality`, `duplication`, `ai`, `peer_review`, `security`, `velocity`) → the
    /// project keys that fed it.
    pub fed: BTreeMap<String, Vec<String>>,
    /// Headline → why the fleet has no value although projects do.
    pub notes: BTreeMap<String, String>,
}

/// Ids the fleet row carries; everything else `metrics` computes is project-level detail.
const FLEET_IDS: &[&str] = &[
    "commits",
    "added",
    "active_devs",
    "multi_stack_pct",
    "breadth_index",
    "techs_per_dev",
    "rework_pct",
    "s_rework",
    "test_discipline_pct",
    "s_tests",
    "doc_discipline_pct",
    "s_docs",
    "dup_pct",
    "s_dup",
    "quality",
    "quality_constituents",
    "pr_count",
    "review_coverage_pct",
];

/// Pool `members` over `[start, end]`.
pub fn pool(members: &[&Member], start: NaiveDate, end: NaiveDate) -> Pooled {
    let mut out = Pooled::default();
    let mut all = Day::default();
    let mut ai_commits = (0u64, 0u64);
    let mut ai_lines = (0u64, 0u64);
    let mut fed = |id: &str, key: &str| {
        out.fed.entry(id.to_string()).or_default().push(key.to_string());
    };
    let mut aggs = Vec::with_capacity(members.len());
    for m in members {
        let mut agg = aggregate(&m.days, start, end).rename_people(|p| format!("{}/{p}", m.key));
        // Each project's own breadth roles decide who is an active dev there.
        for roles in agg.lines.values_mut() {
            roles.retain(|r, _| m.opts.breadth_roles.contains(r));
        }
        agg.lines.retain(|_, roles| !roles.is_empty());
        fed("quality", &m.key);
        if m.opts.ai_attribution {
            fed("ai", &m.key);
            ai_commits.0 += agg.ai.commits as u64;
            ai_commits.1 += agg.commits() as u64;
            ai_lines.0 += agg.ai.added;
            ai_lines.1 += agg.added();
        }
        if m.opts.pull_requests {
            fed("peer_review", &m.key);
        } else {
            agg.pr = Default::default();
        }
        all.merge(&agg);
        aggs.push(agg);
    }

    let dup = pooled_duplication(members, end);
    if dup.is_some() {
        for m in members {
            fed("duplication", &m.key);
        }
    }
    let opts = MetricOptions {
        breadth_roles: Role::ALL.to_vec(),
        pull_requests: members.iter().any(|m| m.opts.pull_requests),
        ..Default::default()
    };
    let m = metrics(&all, &opts, dup.as_ref(), start, end);
    for id in FLEET_IDS {
        if let Some(v) = m.values.get(*id) {
            out.values.insert(id.to_string(), *v);
            out.n.insert(id.to_string(), m.n.get(*id).copied().flatten());
        }
    }
    if members.iter().any(|m| m.opts.ai_attribution) {
        out.values
            .insert("ai_assist_commit_pct".into(), pct(ai_commits.0, ai_commits.1));
        out.n.insert("ai_assist_commit_pct".into(), Some(ai_commits.1));
        out.values
            .insert("ai_assist_line_pct".into(), pct(ai_lines.0, ai_lines.1));
        out.n.insert("ai_assist_line_pct".into(), Some(ai_lines.1));
    }

    // Security: the worst project at the range end.
    let mut worst: Option<f64> = None;
    for m in members {
        if let Some(s) = at_or_before(&m.snapshots, end).and_then(|p| p.values.get("security_score").copied().flatten())
        {
            fed("security", &m.key);
            worst = Some(worst.map_or(s, |w: f64| w.min(s)));
        }
    }
    if out.fed.contains_key("security") {
        out.values.insert("security_score".into(), worst);
        out.n.insert("security_score".into(), None);
    }

    pool_velocity(&mut out, members, &aggs, start, end);
    pool_fte(&mut out, members, start, end);
    out
}

/// The fleet's FTE: the projects' mean FTE over the range, summed, only when every project has it
/// (a partial sum would understate the fleet).
fn pool_fte(out: &mut Pooled, members: &[&Member], start: NaiveDate, end: NaiveDate) {
    let fte: Vec<Option<f64>> = members
        .iter()
        .map(|m| m.opts.velocity.as_ref().and_then(|v| v.fte_mean(start, end)))
        .collect();
    let sum = fte.iter().all(Option::is_some).then(|| fte.iter().flatten().sum());
    out.values.insert("fte".into(), sum);
    out.n.insert("fte".into(), None);
    if sum.is_none() && fte.iter().any(Option::is_some) {
        out.notes
            .insert("fte".into(), "some projects have no FTE for this range".into());
    }
}

/// Duplication over every project's latest snapshot on or before `end`, or `None` when any
/// project lacks it (a partial pool would move the fleet's Quality by who measured it).
fn pooled_duplication(members: &[&Member], end: NaiveDate) -> Option<SnapshotPoint> {
    let (mut dup, mut total) = (0u64, 0u64);
    for m in members {
        let p = at_or_before(&m.snapshots, end)?;
        let pct = p.values.get("dup_pct").copied().flatten()?;
        let n = p.n.get("dup_pct").copied().flatten()?;
        // dup_pct = 100 · duplicated / scanned, so this recovers the line count.
        dup += (pct * n as f64 / 100.0).round() as u64;
        total += n;
    }
    let dup_pct = pct(dup, total)?;
    let mut p = SnapshotPoint::default();
    p.values.insert("dup_pct".into(), Some(dup_pct));
    p.n.insert("dup_pct".into(), Some(total));
    p.values.insert("s_dup".into(), Some(s_dup(dup_pct)));
    p.n.insert("s_dup".into(), Some(total));
    Some(p)
}

fn pool_velocity(out: &mut Pooled, members: &[&Member], aggs: &[Day], start: NaiveDate, end: NaiveDate) {
    let scored: Vec<(&Member, &Day)> = members
        .iter()
        .zip(aggs)
        .filter(|(m, _)| m.opts.velocity.is_some())
        .map(|(m, a)| (*m, a))
        .collect();
    if scored.is_empty() {
        return;
    }
    for (m, _) in &scored {
        out.fed.entry("velocity".into()).or_default().push(m.key.clone());
    }
    let put = |out: &mut Pooled, id: &str, v: Option<f64>, n: Option<u64>| {
        out.values.insert(id.into(), v);
        out.n.insert(id.into(), n);
    };
    let packages: u64 = scored.iter().map(|(_, a)| a.velocity.packages as u64).sum();
    put(out, "velocity_packages", Some(packages as f64), None);
    let not_comparable = |out: &mut Pooled, why: &str| {
        put(out, "velocity", None, Some(packages));
        out.notes.insert("velocity".into(), why.into());
    };
    let basis = scored[0].0.velocity_basis.as_deref();
    if scored.iter().any(|(m, _)| m.velocity_basis.as_deref() != basis) {
        return not_comparable(out, "projects are scored by different classifiers or rubrics");
    }
    let fte: Vec<Option<f64>> = scored
        .iter()
        .map(|(m, _)| m.opts.velocity.as_ref().and_then(|v| v.fte_days(start, end)))
        .collect();
    let approx = fte.iter().all(Option::is_none);
    if !approx && fte.iter().any(Option::is_none) {
        return not_comparable(out, "some projects have FTE and some don't");
    }
    // Same rule as one project: a range before scoring started, or with an unscored package, has
    // no Velocity.
    let complete = scored
        .iter()
        .all(|(m, a)| a.velocity.unscored == 0 && m.opts.velocity.as_ref().is_some_and(|v| start >= v.since));
    let points: Option<f64> = complete.then(|| scored.iter().map(|(_, a)| a.velocity.points as f64).sum());
    let denominator: f64 = if approx {
        scored.iter().map(|(_, a)| a.person_days as f64).sum()
    } else {
        fte.iter().flatten().sum()
    };
    put(
        out,
        "velocity",
        points.filter(|_| denominator > 0.0).map(|p| p / denominator),
        Some(packages),
    );
    put(out, "velocity_approx", points.map(|_| approx as u8 as f64), None);
    put(out, "velocity_points", points, Some(packages));
}

/// One project in `fleet.json`.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FleetProject {
    pub key: String,
    pub name: String,
    pub as_of: NaiveDate,
    /// Collected before the fleet's `as_of`: its later days are missing, not zero.
    pub stale: bool,
    /// What fed its Velocity (`None`: not measured).
    pub velocity_basis: Option<String>,
}

/// `fleet.json`: the pooled fleet row per week and month. Team-level only; built from each
/// project's `project.json`, never from `private/`.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FleetFile {
    pub schema: String,
    pub name: String,
    pub generated_at: String,
    /// The latest `as_of` of its projects.
    pub as_of: NaiveDate,
    /// The earliest `range_start` of its projects.
    pub range_start: NaiveDate,
    pub partial_week: bool,
    pub partial_month: bool,
    pub headline_month: String,
    pub low_n_threshold: u64,
    pub projects: Vec<FleetProject>,
    pub weeks: Vec<String>,
    pub months: Vec<String>,
    pub series_weekly: BTreeMap<String, Vec<Option<f64>>>,
    pub n_weekly: BTreeMap<String, Vec<Option<u64>>>,
    pub series_monthly: BTreeMap<String, Vec<Option<f64>>>,
    pub n_monthly: BTreeMap<String, Vec<Option<u64>>>,
    /// Per week / month: which projects fed each headline, and notes.
    pub coverage_weekly: Vec<Coverage>,
    pub coverage_monthly: Vec<Coverage>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Coverage {
    pub fed: BTreeMap<String, Vec<String>>,
    pub notes: BTreeMap<String, String>,
}

struct FleetSeries {
    labels: Vec<String>,
    values: BTreeMap<String, Vec<Option<f64>>>,
    n: BTreeMap<String, Vec<Option<u64>>>,
    coverage: Vec<Coverage>,
}

fn fleet_series(members: &[&Member], buckets: &[Bucket], as_of: NaiveDate) -> FleetSeries {
    let mut s = FleetSeries {
        labels: buckets.iter().map(|b| b.label.clone()).collect(),
        values: BTreeMap::new(),
        n: BTreeMap::new(),
        coverage: Vec::new(),
    };
    for (i, b) in buckets.iter().enumerate() {
        let p = pool(members, b.start, b.end.min(as_of));
        for (id, v) in p.values {
            let col = s.values.entry(id).or_default();
            col.resize(i, None);
            col.push(v);
        }
        for (id, n) in p.n {
            let col = s.n.entry(id).or_default();
            col.resize(i, None);
            col.push(n);
        }
        s.coverage.push(Coverage {
            fed: p.fed,
            notes: p.notes,
        });
    }
    for col in s.values.values_mut() {
        col.resize(buckets.len(), None);
    }
    for col in s.n.values_mut() {
        col.resize(buckets.len(), None);
    }
    s
}

/// Build `fleet.json` from `(key, project.json)` pairs.
pub fn build_fleet(name: &str, projects: &[(String, ProjectFile)], generated_at: String) -> Option<FleetFile> {
    let as_of = projects.iter().map(|(_, p)| p.meta.as_of).max()?;
    let range_start = projects.iter().map(|(_, p)| p.project.range_start).min()?;
    let members: Vec<Member> = projects.iter().map(|(k, p)| Member::from_project(k, p)).collect();
    let refs: Vec<&Member> = members.iter().collect();
    let monthly = fleet_series(&refs, &months(range_start, as_of), as_of);
    let weekly = fleet_series(&refs, &weeks(range_start, as_of), as_of);
    Some(FleetFile {
        schema: FLEET_SCHEMA.into(),
        name: name.to_string(),
        generated_at,
        as_of,
        range_start,
        partial_week: partial_week(as_of),
        partial_month: partial_month(as_of),
        headline_month: last_complete_month(as_of).label,
        low_n_threshold: LOW_N_THRESHOLD,
        projects: projects
            .iter()
            .zip(&members)
            .map(|((key, p), m)| FleetProject {
                key: key.clone(),
                name: p.project.name.clone(),
                as_of: p.meta.as_of,
                stale: p.meta.as_of < as_of,
                velocity_basis: m.velocity_basis.clone(),
            })
            .collect(),
        weeks: weekly.labels,
        months: monthly.labels,
        series_weekly: weekly.values,
        n_weekly: weekly.n,
        series_monthly: monthly.values,
        n_monthly: monthly.n,
        coverage_weekly: weekly.coverage,
        coverage_monthly: monthly.coverage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::velocity::{FtePeriod, VelocityOptions};

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// A project with one day of commits by `p1` (backend + frontend lines).
    fn member(key: &str, commits: u32, ai: u32, prod: u32, tests: u32) -> Member {
        let mut day = Day::default();
        day.human.commits = commits - ai;
        day.ai.commits = ai;
        day.human.added = 100;
        day.human.prod_commits = prod;
        day.human.tests_with_prod = tests;
        day.rework_added = 1000;
        day.reworked = 200;
        day.add_lines("p1", Role::Backend, "Go", 60);
        day.add_lines("p1", Role::Frontend, "React", 50);
        day.person_commits.insert("p1".into(), commits);
        Member {
            key: key.into(),
            days: [(d("2026-08-03"), day)].into(),
            snapshots: BTreeMap::new(),
            opts: MetricOptions {
                breadth_roles: vec![Role::Backend, Role::Frontend],
                ai_attribution: true,
                ..Default::default()
            },
            velocity_basis: None,
        }
    }

    fn snapshot(dup_pct: f64, n: u64, security: f64) -> BTreeMap<NaiveDate, SnapshotPoint> {
        let mut p = SnapshotPoint::default();
        p.values.insert("dup_pct".into(), Some(dup_pct));
        p.n.insert("dup_pct".into(), Some(n));
        p.values.insert("security_score".into(), Some(security));
        [(d("2026-07-31"), p)].into()
    }

    #[test]
    fn pools_numerators_and_denominators() {
        let a = member("a", 10, 5, 10, 9);
        let b = member("b", 30, 0, 30, 3);
        let p = pool(&[&a, &b], d("2026-08-01"), d("2026-08-31"));
        // (9 + 3) / (10 + 30), not the mean of 90% and 10%.
        assert_eq!(p.values["test_discipline_pct"], Some(30.0));
        assert_eq!(p.n["test_discipline_pct"], Some(40));
        assert_eq!(p.values["ai_assist_commit_pct"], Some(12.5));
        // `p1` of a and `p1` of b are different people.
        assert_eq!(p.values["active_devs"], Some(2.0));
        assert_eq!(p.values["multi_stack_pct"], Some(100.0));
        assert_eq!(p.values["rework_pct"], Some(20.0));
        assert_eq!(p.fed["quality"], vec!["a", "b"]);
        assert!(!p.values.contains_key("security_score"), "nothing measured it");
        assert!(!p.values.contains_key("velocity"));
    }

    #[test]
    fn ai_and_peer_review_pool_only_projects_that_measure_them() {
        let a = member("a", 10, 5, 10, 9);
        let mut b = member("b", 30, 0, 30, 3);
        b.opts.ai_attribution = false;
        let p = pool(&[&a, &b], d("2026-08-01"), d("2026-08-31"));
        assert_eq!(p.values["ai_assist_commit_pct"], Some(50.0));
        assert_eq!(p.fed["ai"], vec!["a"]);
        assert!(!p.values.contains_key("review_coverage_pct"));
    }

    #[test]
    fn breadth_roles_stay_per_project() {
        let a = member("a", 10, 0, 10, 9);
        let mut b = member("b", 10, 0, 10, 9);
        b.opts.breadth_roles = vec![Role::Backend];
        let p = pool(&[&a, &b], d("2026-08-01"), d("2026-08-31"));
        // b's p1 has only one breadth role there.
        assert_eq!(p.values["multi_stack_pct"], Some(50.0));
    }

    #[test]
    fn duplication_needs_every_project_and_security_is_the_worst() {
        let mut a = member("a", 10, 0, 10, 8);
        let mut b = member("b", 10, 0, 10, 8);
        a.snapshots = snapshot(2.0, 1000, 90.0);
        let p = pool(&[&a, &b], d("2026-08-01"), d("2026-08-31"));
        assert_eq!(p.values["quality_constituents"], Some(3.0));
        assert_eq!(p.values["security_score"], Some(90.0));
        assert_eq!(p.fed["security"], vec!["a"]);

        b.snapshots = snapshot(8.0, 3000, 40.0);
        let p = pool(&[&a, &b], d("2026-08-01"), d("2026-08-31"));
        // (20 + 240) / 4000 lines.
        assert_eq!(p.values["dup_pct"], Some(6.5));
        assert_eq!(p.values["quality_constituents"], Some(4.0));
        assert_eq!(p.values["security_score"], Some(40.0));
    }

    fn scored(mut m: Member, basis: &str, points: u32, fte: Option<f64>) -> Member {
        let day = m.days.values_mut().next().unwrap();
        day.velocity.packages = 2;
        day.velocity.points = points;
        m.opts.velocity = Some(VelocityOptions {
            since: d("2026-01-01"),
            as_of: d("2026-09-30"),
            fte: fte
                .map(|fte| FtePeriod {
                    from: None,
                    to: None,
                    fte,
                })
                .into_iter()
                .collect(),
        });
        m.velocity_basis = Some(basis.into());
        m
    }

    #[test]
    fn velocity_pools_only_comparable_projects() {
        let (aug1, aug31) = (d("2026-08-01"), d("2026-08-31"));
        // August 2026 has 21 business days.
        let a = scored(member("a", 10, 0, 10, 9), "six-axis@2 · x", 42, Some(1.0));
        let b = scored(member("b", 10, 0, 10, 9), "six-axis@2 · x", 21, Some(2.0));
        let p = pool(&[&a, &b], aug1, aug31);
        assert_eq!(p.values["velocity"], Some(63.0 / 63.0));
        assert_eq!(p.values["velocity_approx"], Some(0.0));

        let c = scored(member("c", 10, 0, 10, 9), "six-axis@2 · y", 21, Some(2.0));
        let p = pool(&[&a, &c], aug1, aug31);
        assert_eq!(p.values["velocity"], None);
        assert!(p.notes["velocity"].contains("different classifiers"));

        let e = scored(member("e", 10, 0, 10, 9), "six-axis@2 · x", 21, None);
        let p = pool(&[&a, &e], aug1, aug31);
        assert!(p.notes["velocity"].contains("FTE"));

        // A project without a classifier doesn't block the others.
        let plain = member("f", 10, 0, 10, 9);
        let p = pool(&[&a, &plain], aug1, aug31);
        assert_eq!(p.values["velocity"], Some(42.0 / 21.0));
        assert_eq!(p.fed["velocity"], vec!["a"]);
    }

    #[test]
    fn fte_is_summed_only_when_every_project_has_it() {
        let (aug1, aug31) = (d("2026-08-01"), d("2026-08-31"));
        let a = scored(member("a", 10, 0, 10, 9), "six-axis@2 · x", 42, Some(1.0));
        let b = scored(member("b", 10, 0, 10, 9), "six-axis@2 · y", 21, Some(2.5));
        assert_eq!(a.metrics(aug1, aug31).values["fte"], Some(1.0));
        let p = pool(&[&a, &b], aug1, aug31);
        assert_eq!(p.values["fte"], Some(3.5), "classifiers may differ; FTE still sums");

        let e = scored(member("e", 10, 0, 10, 9), "six-axis@2 · x", 21, None);
        let p = pool(&[&a, &e], aug1, aug31);
        assert_eq!(p.values["fte"], None);
        assert!(p.notes["fte"].contains("no FTE"));
    }

    #[test]
    fn member_metrics_match_the_project_engine() {
        let a = member("a", 10, 5, 10, 9);
        let m = a.metrics(d("2026-08-01"), d("2026-08-31"));
        assert_eq!(m.values["test_discipline_pct"], Some(90.0));
    }
}
