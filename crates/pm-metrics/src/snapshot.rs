//! Snapshot metrics (spec §3.4–3.5, §7): measured on the tree at each week-end and month-end, pooled
//! across repos. `pm-snapshot` turns tool output into one [`RepoSummary`] per repo and commit; this
//! module pools them and scores the result. No I/O.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Advisory severity (spec §7.1), ordered.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    #[default]
    Unknown,
    Low,
    Moderate,
    High,
    Critical,
}

/// Own-code SAST of one tree: non-test findings only (spec §7.4).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SastSummary {
    pub high: u64,
    pub medium: u64,
    pub low: u64,
    /// `rule\tfile` → findings, for new/fixed between snapshots.
    pub keys: BTreeMap<String, u32>,
}

/// Live secrets of one tree (spec §7.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretsSummary {
    /// High confidence, untriaged: the Security gate.
    pub high: u64,
    pub low: u64,
}

/// One vulnerable package entry as osv-scanner reported it.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VulnPackage {
    pub name: String,
    pub version: String,
    /// Advisories against it.
    pub count: u64,
    pub max_severity: Severity,
}

/// Dependency vulnerabilities of one repo (spec §7.1).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VulnSummary {
    /// Advisory id → highest severity seen in this repo.
    pub advisories: BTreeMap<String, Severity>,
    /// Package entries with at least one advisory, in scan order.
    pub packages: Vec<VulnPackage>,
    /// Every package entry in the scan results.
    pub total_packages: u64,
}

/// One repo at one commit. `None` = not measured (tool missing, failed or incomplete).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RepoSummary {
    pub repo: String,
    pub sha: String,
    /// scc code lines and complexity, with the duplication excludes (spec §3.5).
    pub code: Option<u64>,
    pub complexity: Option<u64>,
    /// jscpd duplicated and scanned lines (spec §3.4).
    pub dup_lines: Option<u64>,
    pub dup_total: Option<u64>,
    /// scc KLOC of the whole tree, to 0.1 (the SAST density denominator).
    pub kloc_all: Option<f64>,
    pub sast: Option<SastSummary>,
    pub secrets: Option<SecretsSummary>,
    pub suppressions: Option<u64>,
    pub vulns: Option<VulnSummary>,
}

/// The pooled snapshot of a project on one date.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SnapshotPoint {
    pub values: BTreeMap<String, Option<f64>>,
    pub n: BTreeMap<String, Option<u64>>,
    /// Repo → measured commit.
    pub repos: BTreeMap<String, String>,
}

/// Duplication score (spec §3.4): 3% → 100, 15% → 0.
pub fn s_dup(x: f64) -> f64 {
    100.0 * ((15.0 - x) / 12.0).clamp(0.0, 1.0)
}

/// Security score (spec §7.3).
pub fn security_score(critical: u64, high: u64, moderate: u64, low: u64, live_secrets: u64) -> f64 {
    let sev = 10.0 * critical as f64 + 3.0 * high as f64 + moderate as f64 + 0.2 * low as f64;
    let mut score = 100.0 * libm::pow(0.5, sev / 250.0);
    if critical > 0 || live_secrets > 0 {
        score = score.min(40.0);
    }
    score.max(1.0)
}

/// Every repo has the measurement, or the pooled metric is not measured.
fn all<T: Copy>(repos: &[RepoSummary], f: impl Fn(&RepoSummary) -> Option<T>) -> Option<Vec<T>> {
    repos.iter().map(f).collect()
}

/// Packages pooled by name across repos: counts add up, the highest severity wins, the last
/// version seen is kept. Ordered by count, then first appearance.
pub fn pooled_packages(repos: &[RepoSummary]) -> Vec<VulnPackage> {
    let mut order: Vec<String> = Vec::new();
    let mut by_name: BTreeMap<String, VulnPackage> = BTreeMap::new();
    for p in repos.iter().filter_map(|r| r.vulns.as_ref()).flat_map(|v| &v.packages) {
        match by_name.get_mut(&p.name) {
            Some(e) => {
                e.count += p.count;
                e.max_severity = e.max_severity.max(p.max_severity);
                e.version = p.version.clone();
            }
            None => {
                order.push(p.name.clone());
                by_name.insert(p.name.clone(), p.clone());
            }
        }
    }
    let mut out: Vec<VulnPackage> = order.into_iter().map(|n| by_name.remove(&n).expect("seen")).collect();
    out.sort_by_key(|p| std::cmp::Reverse(p.count));
    out
}

/// SAST findings new and fixed between two snapshots, by `(repo, rule, file)` multiset (spec §7.4).
pub fn sast_diff(current: &BTreeMap<String, u32>, previous: &BTreeMap<String, u32>) -> (f64, f64) {
    let diff = |a: &BTreeMap<String, u32>, b: &BTreeMap<String, u32>| -> f64 {
        a.iter()
            .map(|(k, n)| n.saturating_sub(*b.get(k).unwrap_or(&0)) as f64)
            .sum()
    };
    (diff(current, previous), diff(previous, current))
}

/// Pool one date's repo summaries. Also returns the SAST keys (`None` if SAST is not measured) for
/// [`sast_diff`] against the previous snapshot of the same cadence.
pub fn pool(repos: &[RepoSummary]) -> (SnapshotPoint, Option<BTreeMap<String, u32>>) {
    let mut p = SnapshotPoint {
        repos: repos.iter().map(|r| (r.repo.clone(), r.sha.clone())).collect(),
        ..Default::default()
    };
    let mut put = |id: &str, v: Option<f64>, n: Option<u64>| {
        p.values.insert(id.into(), v);
        p.n.insert(id.into(), n);
    };

    let dup = all(repos, |r| Some((r.dup_lines?, r.dup_total?)));
    let (d, t) = dup
        .as_ref()
        .map(|v| v.iter().fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1)))
        .unzip();
    let dup_pct = t.filter(|t| *t > 0).map(|t| 100.0 * d.unwrap_or(0) as f64 / t as f64);
    put("dup_pct", dup_pct, t);
    put("s_dup", dup_pct.map(s_dup), t);

    let cx = all(repos, |r| Some((r.code?, r.complexity?)));
    let (code, complexity) = cx
        .as_ref()
        .map(|v| v.iter().fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1)))
        .unzip();
    put(
        "cx_per_kloc",
        code.filter(|c| *c > 0)
            .map(|c| 1000.0 * complexity.unwrap_or(0) as f64 / c as f64),
        code,
    );
    put("kloc", code.map(|c| c as f64 / 1000.0), None);

    let mut keys = BTreeMap::new();
    let sast = all(repos, |r| r.sast.as_ref().map(|_| ())).map(|_| {
        repos
            .iter()
            .filter_map(|r| r.sast.as_ref().map(|s| (r, s)))
            .collect::<Vec<_>>()
    });
    let kloc_all = all(repos, |r| r.kloc_all).map(|v| v.iter().sum::<f64>());
    put("sast_kloc", kloc_all, None);
    match &sast {
        Some(list) => {
            let (h, m, l) = list
                .iter()
                .fold((0, 0, 0), |a, (_, s)| (a.0 + s.high, a.1 + s.medium, a.2 + s.low));
            for (r, s) in list {
                for (k, n) in &s.keys {
                    *keys.entry(format!("{}\t{k}", r.repo)).or_insert(0) += n;
                }
            }
            put("sast_high", Some(h as f64), None);
            put("sast_medium", Some(m as f64), None);
            put("sast_low", Some(l as f64), None);
            let total = (h + m + l) as f64;
            put("sast_per_kloc", kloc_all.filter(|k| *k > 0.0).map(|k| total / k), None);
        }
        None => {
            for id in ["sast_high", "sast_medium", "sast_low", "sast_per_kloc"] {
                put(id, None, None);
            }
        }
    }

    let secrets = all(repos, |r| r.secrets);
    let gate = secrets.as_ref().map(|v| v.iter().map(|s| s.high).sum::<u64>());
    put("secrets_high", gate.map(|x| x as f64), None);
    put(
        "secrets_low",
        secrets.as_ref().map(|v| v.iter().map(|s| s.low).sum::<u64>() as f64),
        None,
    );
    put(
        "suppressions",
        all(repos, |r| r.suppressions).map(|v| v.iter().sum::<u64>() as f64),
        None,
    );

    let vulns = all(repos, |r| r.vulns.as_ref().map(|_| ()));
    if vulns.is_some() {
        let mut counts = BTreeMap::new();
        let mut advisories = 0u64;
        for v in repos.iter().filter_map(|r| r.vulns.as_ref()) {
            for sev in v.advisories.values() {
                advisories += 1;
                *counts.entry(*sev).or_insert(0u64) += 1;
            }
        }
        let c = |s: Severity| counts.get(&s).copied().unwrap_or(0);
        for (id, s) in [
            ("vuln_critical", Severity::Critical),
            ("vuln_high", Severity::High),
            ("vuln_moderate", Severity::Moderate),
            ("vuln_low", Severity::Low),
        ] {
            put(id, Some(c(s) as f64), None);
        }
        put("vuln_advisories", Some(advisories as f64), None);
        put("vuln_packages", Some(pooled_packages(repos).len() as f64), None);
        let total: u64 = repos
            .iter()
            .filter_map(|r| r.vulns.as_ref())
            .map(|v| v.total_packages)
            .sum();
        put("vuln_total_packages", Some(total as f64), None);
        let score = security_score(
            c(Severity::Critical),
            c(Severity::High),
            c(Severity::Moderate),
            c(Severity::Low),
            gate.unwrap_or(0),
        );
        put("security_score", Some(score), None);
    } else {
        for id in [
            "vuln_critical",
            "vuln_high",
            "vuln_moderate",
            "vuln_low",
            "vuln_advisories",
            "vuln_packages",
            "vuln_total_packages",
            "security_score",
        ] {
            put(id, None, None);
        }
    }
    let keys = sast.is_some().then_some(keys);
    (p, keys)
}

/// The snapshot a range ending on `end` shows: the latest one on or before it.
pub fn at_or_before(points: &BTreeMap<NaiveDate, SnapshotPoint>, end: NaiveDate) -> Option<&SnapshotPoint> {
    points.range(..=end).next_back().map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(name: &str) -> RepoSummary {
        RepoSummary {
            repo: name.into(),
            sha: "abc".into(),
            code: Some(8000),
            complexity: Some(400),
            dup_lines: Some(300),
            dup_total: Some(10000),
            kloc_all: Some(9.5),
            sast: Some(SastSummary {
                high: 1,
                medium: 2,
                low: 0,
                keys: [("eval\tsrc/a.ts".to_string(), 1), ("xss\tsrc/b.ts".to_string(), 2)].into(),
            }),
            secrets: Some(SecretsSummary { high: 0, low: 3 }),
            suppressions: Some(2),
            vulns: Some(VulnSummary {
                advisories: [
                    ("GHSA-1".to_string(), Severity::High),
                    ("GHSA-2".to_string(), Severity::Moderate),
                ]
                .into(),
                packages: vec![VulnPackage {
                    name: "lodash".into(),
                    version: "4.17.20".into(),
                    count: 2,
                    max_severity: Severity::High,
                }],
                total_packages: 3,
            }),
        }
    }

    #[test]
    fn scores() {
        assert_eq!(s_dup(3.0), 100.0);
        assert_eq!(s_dup(15.0), 0.0);
        assert_eq!(s_dup(9.0), 50.0);
        assert_eq!(security_score(0, 0, 0, 0, 0), 100.0);
        // 250 points of severity halve the score.
        assert!((security_score(0, 0, 250, 0, 0) - 50.0).abs() < 1e-9);
        assert_eq!(security_score(1, 0, 0, 0, 0), 40.0);
        assert_eq!(security_score(0, 0, 0, 0, 1), 40.0);
        assert_eq!(security_score(1000, 0, 0, 0, 0), 1.0);
    }

    #[test]
    fn pools_across_repos() {
        let (p, keys) = pool(&[repo("api"), repo("web")]);
        let keys = keys.unwrap();
        let v = |id: &str| p.values[id];
        assert_eq!(v("dup_pct"), Some(3.0));
        assert_eq!(v("s_dup"), Some(100.0));
        assert_eq!(v("cx_per_kloc"), Some(50.0));
        assert_eq!(v("kloc"), Some(16.0));
        assert_eq!(v("sast_high"), Some(2.0));
        assert_eq!(v("sast_per_kloc"), Some(6.0 / 19.0));
        assert_eq!(v("secrets_low"), Some(6.0));
        assert_eq!(v("vuln_advisories"), Some(4.0));
        assert_eq!(v("vuln_high"), Some(2.0));
        assert_eq!(v("vuln_packages"), Some(1.0));
        assert_eq!(pooled_packages(&[repo("api"), repo("web")])[0].count, 4);
        assert_eq!(keys.len(), 4);

        // One finding fixed in web, one new in api.
        let mut api = repo("api");
        api.sast.as_mut().unwrap().keys.insert("sqli\tsrc/c.ts".into(), 1);
        let mut web = repo("web");
        web.sast.as_mut().unwrap().keys.insert("xss\tsrc/b.ts".into(), 1);
        let (_, next) = pool(&[api, web]);
        assert_eq!(sast_diff(&next.unwrap(), &keys), (1.0, 1.0));
    }

    #[test]
    fn a_repo_without_a_measurement_makes_it_not_measured() {
        let mut web = repo("web");
        web.dup_lines = None;
        web.vulns = None;
        let (p, _) = pool(&[repo("api"), web]);
        assert_eq!(p.values["dup_pct"], None);
        assert_eq!(p.values["security_score"], None);
        assert_eq!(p.values["cx_per_kloc"], Some(50.0));
    }

    #[test]
    fn latest_snapshot_for_a_range_end() {
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let mut points = BTreeMap::new();
        points.insert(d("2026-09-27"), SnapshotPoint::default());
        let mut later = SnapshotPoint::default();
        later.repos.insert("api".into(), "x".into());
        points.insert(d("2026-09-30"), later.clone());
        assert_eq!(at_or_before(&points, d("2026-10-02")), Some(&later));
        assert!(at_or_before(&points, d("2026-09-01")).is_none());
    }
}
