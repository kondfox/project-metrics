//! Metrics of one bucket or range, computed from its summed components.

use std::collections::BTreeMap;

use chrono::{Days as ChronoDays, NaiveDate};
use pm_classify::Role;
use serde::{Deserialize, Serialize};

use crate::day::{Day, Days, Group, aggregate};
use crate::pr::hours;
use crate::snapshot::SnapshotPoint;
use crate::stats::{entropy_norm, median, median_f64, pct, percentile};
use crate::velocity::VelocityOptions;

/// A role or technology counts for a person from this many added lines (spec §5).
pub const MULTISTACK_THRESHOLD: u64 = 40;

#[derive(Clone, Debug, Default)]
pub struct MetricOptions {
    pub breadth_roles: Vec<Role>,
    pub ai_attribution: bool,
    /// Roles that get a `mix_<role>` series (those with lines anywhere in the project).
    pub mix_roles: Vec<Role>,
    /// Code-host data was collected (spec §8); else the PR metrics are left out (not measured).
    pub pull_requests: bool,
    /// The code host reports PR line counts (GitHub).
    pub pr_size: bool,
    /// Work packages were scored (spec §4); else Velocity is left out (not measured).
    pub velocity: Option<VelocityOptions>,
}

/// One person's multi-stack line-up in a window (lead-only drill-down).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Leader {
    pub name: String,
    pub stacks: Vec<Role>,
    pub techs: Vec<String>,
    pub breadth: f64,
    pub lines: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MultiStack {
    /// People with any added lines in breadth roles (also "active devs", spec §2).
    pub authors: u64,
    pub multi_pct: Option<f64>,
    pub breadth_index: Option<f64>,
    pub techs_per_dev: Option<f64>,
    pub leaders: Vec<Leader>,
}

/// Multi-stack metrics over summed components (spec §5).
pub fn multistack(agg: &Day, breadth_roles: &[Role]) -> MultiStack {
    let mut multi = 0u64;
    let mut breadth_sum = 0.0;
    let mut techs_sum = 0u64;
    let mut leaders = Vec::new();
    for (person, roles) in &agg.lines {
        let mut role_lines: BTreeMap<Role, u64> = BTreeMap::new();
        let mut tech_lines: BTreeMap<&str, u64> = BTreeMap::new();
        for (role, techs) in roles.iter().filter(|(r, _)| breadth_roles.contains(r)) {
            for (tech, n) in techs {
                *role_lines.entry(*role).or_default() += n;
                *tech_lines.entry(tech.as_str()).or_default() += n;
            }
        }
        let lines: u64 = role_lines.values().sum();
        if lines == 0 {
            continue;
        }
        let counting: Vec<(Role, u64)> = role_lines
            .iter()
            .filter(|(_, n)| **n >= MULTISTACK_THRESHOLD)
            .map(|(r, n)| (*r, *n))
            .collect();
        if counting.len() >= 2 {
            multi += 1;
        }
        let b = entropy_norm(&counting.iter().map(|(_, n)| *n).collect::<Vec<_>>());
        breadth_sum += b;
        let techs: Vec<String> = tech_lines
            .iter()
            .filter(|(_, n)| **n >= MULTISTACK_THRESHOLD)
            .map(|(t, _)| t.to_string())
            .collect();
        techs_sum += techs.len() as u64;
        leaders.push(Leader {
            name: person.clone(),
            stacks: counting.iter().map(|(r, _)| *r).collect(),
            techs,
            breadth: b,
            lines,
        });
    }
    leaders.sort_by(|a, b| {
        b.stacks
            .len()
            .cmp(&a.stacks.len())
            .then(b.lines.cmp(&a.lines))
            .then(a.name.cmp(&b.name))
    });
    let authors = leaders.len() as u64;
    let per_author = |x: f64| (authors > 0).then(|| x / authors as f64);
    MultiStack {
        authors,
        multi_pct: pct(multi, authors),
        breadth_index: per_author(100.0 * breadth_sum),
        techs_per_dev: per_author(techs_sum as f64),
        leaders,
    }
}

/// Rework score: distance from the 15–25% band (spec §3.1).
pub fn s_rework(x: f64) -> f64 {
    if x < 15.0 {
        (100.0 - 4.0 * (15.0 - x)).max(0.0)
    } else if x > 25.0 {
        (100.0 - 4.0 * (x - 25.0)).max(0.0)
    } else {
        100.0
    }
}

pub fn s_tests(x: f64) -> f64 {
    100.0 * (x / 80.0).min(1.0)
}

pub fn s_docs(x: f64) -> f64 {
    100.0 * (x / 40.0).min(1.0)
}

/// Geometric mean of the present sub-scores, each floored at 1 (spec §3).
pub fn quality(subscores: &[Option<f64>]) -> Option<f64> {
    let present: Vec<f64> = subscores.iter().flatten().map(|s| s.max(1.0)).collect();
    if present.is_empty() {
        return None;
    }
    // libm for bit-identical results across platforms and WASM (see stats::entropy_norm).
    Some(libm::exp(
        present.iter().map(|s| libm::log(*s)).sum::<f64>() / present.len() as f64,
    ))
}

/// Every metric of one bucket or range (also the dashboard engine's `range` response).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, Serialize)]
pub struct Metrics {
    pub values: BTreeMap<String, Option<f64>>,
    pub n: BTreeMap<String, Option<u64>>,
    pub stack_mix: BTreeMap<Role, f64>,
    pub leaders: Vec<Leader>,
}

impl Metrics {
    fn put(&mut self, id: &str, v: Option<f64>, n: Option<u64>) {
        self.values.insert(id.to_string(), v);
        self.n.insert(id.to_string(), n);
    }
}

fn combined(agg: &Day) -> Group {
    let mut g = agg.human.clone();
    g.merge(&agg.ai);
    g
}

/// Velocity of `[start, end]` (spec §4): points per FTE-day, else per active engineer-day with
/// `velocity_approx` = 1. Null for a range that starts before scoring did or holds an unscored
/// package.
fn velocity(m: &mut Metrics, agg: &Day, v: &VelocityOptions, start: NaiveDate, end: NaiveDate) {
    let c = &agg.velocity;
    let packages = Some(c.packages as u64);
    m.put("velocity_packages", Some(c.packages as f64), None);
    let points = (start >= v.since && c.unscored == 0).then_some(c.points as f64);
    let (denominator, approx) = match v.fte_days(start, end) {
        Some(d) => (d, false),
        None => (agg.person_days as f64, true),
    };
    let per = |d: f64| points.filter(|_| d > 0.0).map(|p| p / d);
    m.put("fte", v.fte_mean(start, end), None);
    m.put("velocity", per(denominator), packages);
    m.put("velocity_approx", points.map(|_| approx as u8 as f64), None);
    m.put("velocity_points", points, packages);
    m.put("points_per_commit", per(c.commits as f64), Some(c.commits as u64));
    m.put("points_per_kloc", per(c.added as f64 / 1000.0), Some(c.added));
    m.put("verification_debt_pts", points.map(|_| c.debt as f64), packages);
}

/// `snapshot` is the snapshot at the range's end (spec §1.1: a range shows the snapshot value at its
/// end); its metrics are added and duplication joins Quality. `[start, end]` is the range `agg`
/// sums (Velocity counts its business days).
pub fn metrics(
    agg: &Day,
    opts: &MetricOptions,
    snapshot: Option<&SnapshotPoint>,
    start: NaiveDate,
    end: NaiveDate,
) -> Metrics {
    let mut m = Metrics::default();
    let all = combined(agg);
    let commits = all.commits as u64;

    m.put("commits", Some(commits as f64), None);
    m.put("added", Some(all.added as f64), None);
    let sized = Some(all.sizes.len() as u64);
    m.put("commit_med", percentile(&all.sizes, 0.5), sized);
    m.put("commit_p90", percentile(&all.sizes, 0.9), sized);
    let per_person: Vec<u32> = agg.person_commits.values().copied().collect();
    m.put(
        "commits_per_dev_med",
        median(&per_person),
        Some(per_person.len() as u64),
    );

    let mut role_lines: BTreeMap<Role, u64> = BTreeMap::new();
    for roles in agg.lines.values() {
        for (r, techs) in roles {
            *role_lines.entry(*r).or_default() += techs.values().sum::<u64>();
        }
    }
    let total: u64 = role_lines.values().sum();
    for r in &opts.mix_roles {
        let v = pct(role_lines.get(r).copied().unwrap_or(0), total);
        m.put(&format!("mix_{r}"), v, Some(total));
    }
    m.stack_mix = role_lines
        .iter()
        .filter_map(|(r, n)| pct(*n, total).map(|v| (*r, v)))
        .collect();

    let rework = pct(agg.reworked, agg.rework_added);
    m.put("rework_pct", rework, Some(agg.rework_added));
    let prod = Some(all.prod_commits as u64);
    let tests = pct(all.tests_with_prod as u64, all.prod_commits as u64);
    let docs = pct(all.docs_with_prod as u64, all.prod_commits as u64);
    m.put("test_discipline_pct", tests, prod);
    m.put("doc_discipline_pct", docs, prod);

    let ms = multistack(agg, &opts.breadth_roles);
    let authors = Some(ms.authors);
    m.put("active_devs", Some(ms.authors as f64), None);
    m.put("multi_stack_pct", ms.multi_pct, authors);
    m.put("breadth_index", ms.breadth_index, authors);
    m.put("techs_per_dev", ms.techs_per_dev, authors);
    m.leaders = ms.leaders;

    if opts.ai_attribution {
        m.put(
            "ai_assist_commit_pct",
            pct(agg.ai.commits as u64, commits),
            Some(commits),
        );
        m.put("ai_assist_line_pct", pct(agg.ai.added, all.added), Some(all.added));
    }

    if opts.pull_requests {
        let pr = &agg.pr;
        let merged = Some(pr.merged as u64);
        let reviewed = Some(pr.reviewed as u64);
        let h = |ms: &[u64]| median_f64(&ms.iter().map(|x| hours(*x)).collect::<Vec<_>>());
        m.put("pr_count", Some(pr.merged as f64), None);
        m.put("pr_cycle_median_h", h(&pr.cycle_ms), merged);
        m.put("review_wait_median_h", h(&pr.wait_ms), reviewed);
        m.put("review_coverage_pct", pct(pr.reviewed as u64, pr.merged as u64), merged);
        m.put("mentoring_pairs", Some(pr.mentored as f64), reviewed);
        if opts.pr_size {
            m.put(
                "pr_size_median",
                percentile(&pr.sizes, 0.5),
                Some(pr.sizes.len() as u64),
            );
        }
    }

    if let Some(v) = &opts.velocity {
        velocity(&mut m, agg, v, start, end);
    }

    if let Some(sp) = snapshot {
        for (id, v) in &sp.values {
            m.values.insert(id.clone(), *v);
        }
        for (id, n) in &sp.n {
            m.n.insert(id.clone(), *n);
        }
    }
    let s_dup = snapshot.and_then(|sp| sp.values.get("s_dup").copied().flatten());
    let subs = [rework.map(s_rework), tests.map(s_tests), docs.map(s_docs), s_dup];
    m.put("s_rework", subs[0], Some(agg.rework_added));
    m.put("s_tests", subs[1], prod);
    m.put("s_docs", subs[2], prod);
    let present = subs.iter().flatten().count() as u64;
    m.put("quality", quality(&subs), None);
    m.put("quality_constituents", Some(present as f64), None);
    m
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupSummary {
    pub commits: u64,
    pub added: u64,
    pub med_size: Option<f64>,
    pub test_pct: Option<f64>,
    pub doc_pct: Option<f64>,
}

impl GroupSummary {
    fn of(g: &Group) -> GroupSummary {
        GroupSummary {
            commits: g.commits as u64,
            added: g.added,
            med_size: percentile(&g.sizes, 0.5),
            test_pct: pct(g.tests_with_prod as u64, g.prod_commits as u64),
            doc_pct: pct(g.docs_with_prod as u64, g.prod_commits as u64),
        }
    }
}

/// AI vs human over the trailing 12 months (spec §6). Project-level only.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AiCompare {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub ai: GroupSummary,
    pub human: GroupSummary,
}

/// `end` is the latest commit on or before `as_of`; the window is the 365 days before it.
pub fn ai_compare(days: &Days, end: NaiveDate) -> AiCompare {
    let from = end - ChronoDays::new(365);
    let agg = aggregate(days, from, end);
    AiCompare {
        from,
        to: end,
        ai: GroupSummary::of(&agg.ai),
        human: GroupSummary::of(&agg.human),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::velocity::{FtePeriod, VelocityDay};

    const D0: NaiveDate = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn velocity_per_fte_day_or_engineer_day() {
        let mut days = Days::new();
        for (date, people, pts) in [("2026-08-03", 2, 8u32), ("2026-08-04", 1, 5), ("2026-08-10", 3, 0)] {
            let day = days.entry(d(date)).or_default();
            for i in 0..people {
                day.person_commits.insert(format!("p{i}"), 1);
            }
            day.velocity = VelocityDay {
                packages: 1,
                points: pts,
                commits: 4,
                added: 500,
                debt: pts / 2,
                ..Default::default()
            };
        }
        let mut v = VelocityOptions {
            since: d("2026-08-01"),
            as_of: d("2026-08-31"),
            fte: vec![FtePeriod {
                from: None,
                to: None,
                fte: 2.0,
            }],
        };
        let opts = |v: &VelocityOptions| MetricOptions {
            velocity: Some(v.clone()),
            ..Default::default()
        };
        let (s, e) = (d("2026-08-01"), d("2026-08-31"));
        let agg = aggregate(&days, s, e);
        assert_eq!(agg.person_days, 6);
        let m = metrics(&agg, &opts(&v), None, s, e);
        // 13 points over 21 business days × 2 FTE.
        assert_eq!(m.values["velocity"], Some(13.0 / 42.0));
        assert_eq!(m.values["velocity_approx"], Some(0.0));
        assert_eq!(m.values["points_per_commit"], Some(13.0 / 12.0));
        assert_eq!(m.values["points_per_kloc"], Some(13.0 / 1.5));
        assert_eq!(m.values["verification_debt_pts"], Some(6.0));
        assert_eq!(m.n["velocity"], Some(3));

        v.fte.clear();
        let m = metrics(&agg, &opts(&v), None, s, e);
        assert_eq!(m.values["velocity"], Some(13.0 / 6.0), "per engineer-day");
        assert_eq!(m.values["velocity_approx"], Some(1.0));

        // Before scoring started, or with an unscored package: null, not a low number.
        let m = metrics(&agg, &opts(&v), None, d("2026-07-27"), e);
        assert_eq!(m.values["velocity"], None);
        assert_eq!(m.values["velocity_packages"], Some(3.0));
        days.get_mut(&d("2026-08-10")).unwrap().velocity.unscored = 1;
        let m = metrics(&aggregate(&days, s, e), &opts(&v), None, s, e);
        assert_eq!(m.values["velocity"], None);
        assert_eq!(m.values["velocity_points"], None);
        let m = metrics(
            &aggregate(&days, s, d("2026-08-09")),
            &opts(&v),
            None,
            s,
            d("2026-08-09"),
        );
        assert_eq!(m.values["velocity_points"], Some(13.0));
    }

    #[test]
    fn rework_band() {
        assert_eq!(s_rework(20.0), 100.0);
        assert_eq!(s_rework(15.0), 100.0);
        assert_eq!(s_rework(10.0), 80.0);
        assert_eq!(s_rework(0.0), 40.0);
        assert_eq!(s_rework(30.0), 80.0);
        assert_eq!(s_rework(60.0), 0.0);
    }

    #[test]
    fn quality_geomean() {
        assert_eq!(quality(&[None, None]), None);
        let q = quality(&[Some(100.0), Some(25.0), None]).unwrap();
        assert!((q - 50.0).abs() < 1e-9);
        // Floored at 1.
        let q = quality(&[Some(0.0), Some(100.0)]).unwrap();
        assert!((q - 10.0).abs() < 1e-9);
    }

    fn day_with(lines: &[(&str, Role, &str, u64)]) -> Day {
        let mut d = Day::default();
        for (p, r, t, n) in lines {
            d.add_lines(p, *r, t, *n);
        }
        d
    }

    #[test]
    fn multistack_counts() {
        let d = day_with(&[
            ("ann", Role::Frontend, "React", 100),
            ("ann", Role::Backend, "TypeScript", 100),
            ("bob", Role::Backend, "Kotlin", 500),
            ("bob", Role::Frontend, "React", 39),
            ("cy", Role::Docs, "Shell", 1000),
        ]);
        let ms = multistack(&d, &[Role::Frontend, Role::Backend]);
        assert_eq!(ms.authors, 2);
        assert_eq!(ms.multi_pct, Some(50.0));
        // ann: two equal roles → 1.0; bob: one counting role → 0.
        assert!((ms.breadth_index.unwrap() - 50.0).abs() < 1e-9);
        assert_eq!(ms.techs_per_dev, Some(1.5));
        assert_eq!(ms.leaders[0].name, "ann");
        assert_eq!(ms.leaders[0].stacks, vec![Role::Frontend, Role::Backend]);
    }

    #[test]
    fn no_data_is_null() {
        let opts = MetricOptions {
            breadth_roles: vec![Role::Backend],
            ai_attribution: true,
            mix_roles: vec![Role::Backend],
            pull_requests: true,
            pr_size: true,
            velocity: None,
        };
        let m = metrics(&Day::default(), &opts, None, D0, D0);
        assert_eq!(m.values["commits"], Some(0.0));
        assert_eq!(m.values["pr_count"], Some(0.0));
        let without = metrics(&Day::default(), &MetricOptions::default(), None, D0, D0);
        assert!(!without.values.contains_key("pr_count"), "not measured: left out");
        for id in [
            "rework_pct",
            "test_discipline_pct",
            "commit_med",
            "commits_per_dev_med",
            "multi_stack_pct",
            "ai_assist_commit_pct",
            "mix_backend",
            "quality",
            "pr_cycle_median_h",
            "review_wait_median_h",
            "review_coverage_pct",
            "pr_size_median",
        ] {
            assert_eq!(m.values[id], None, "{id}");
        }
    }
}
