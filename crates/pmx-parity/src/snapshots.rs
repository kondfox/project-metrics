//! Snapshot metrics against the golden set (parity.md §3: duplication, complexity, SAST, secrets,
//! dependency vulnerabilities).
//!
//! Replay: the golden set's recorded per-commit scans and month-end scc/jscpd numbers go through
//! pmx's own summarize → pool path, so the counting is checked exactly and quickly.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use pm_classify::Classifier;
use pm_metrics::snapshot::{SnapshotPoint, pool, sast_diff, security_score};
use pm_snapshot::raw::{Duplication, LockfileScan, SastFinding, SccTotals, SecretHit};
use pm_snapshot::summarize::SecurityRules;
use pm_snapshot::{RawScan, run, summarize};
use serde_json::Value;

/// Monthly values the replay produces: metric → month → value.
pub type Values = BTreeMap<String, BTreeMap<String, Option<f64>>>;

fn read(path: &Path) -> Result<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?)
        .with_context(|| format!("parsing {}", path.display()))
}

/// One recorded own-code scan → the parts of a raw scan it holds.
fn recorded(scan: &Value) -> (Option<f64>, Vec<SastFinding>, Vec<SecretHit>, u64) {
    let sast = scan["sast"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| SastFinding {
            rule: f["rule"].as_str().unwrap_or_default().into(),
            severity: f["sev"].as_str().unwrap_or_default().into(),
            file: f["file"].as_str().unwrap_or_default().into(),
            line: f["line"].as_u64().unwrap_or(0),
            cwe: f["cwe"].as_str().map(String::from),
        })
        .collect();
    let secrets = scan["live_secrets"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|h| SecretHit {
            rule: h["rule"].as_str().unwrap_or_default().into(),
            file: h["file"].as_str().unwrap_or_default().into(),
            line: h["line"].as_u64().unwrap_or(0),
        })
        .collect();
    (
        scan["kloc"].as_f64(),
        sast,
        secrets,
        scan["suppressions"].as_u64().unwrap_or(0),
    )
}

pub struct Replay {
    pub values: Values,
    pub notes: Vec<String>,
}

/// `lockfiles`: repo → configured lockfiles (the golden configs list them).
/// `2026-05` → `2026-06-01`: the first day after the month.
fn next_day(month: &str) -> String {
    let d = chrono::NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d").expect("month label");
    (pm_metrics::period::month_end(d) + chrono::Days::new(1)).to_string()
}

pub fn replay(
    golden: &Path,
    key: &str,
    lockfiles: &BTreeMap<String, Vec<String>>,
    repo_dirs: &BTreeMap<String, std::path::PathBuf>,
    prototype_rules: bool,
) -> Result<Replay> {
    let quality = read(&golden.join("prototype_out").join(format!("{key}_quality_trend.json")))?;
    let months: Vec<String> = quality["months"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m.as_str().map(String::from))
        .collect();
    let scans_dir = golden.join("inputs/snapshots").join(key);
    let mut by_prefix: BTreeMap<String, Value> = BTreeMap::new();
    for e in std::fs::read_dir(&scans_dir)? {
        let p = e?.path();
        if let Some(name) = p.file_stem().and_then(|s| s.to_str()) {
            by_prefix.insert(name.to_string(), read(&p)?);
        }
    }
    let rules = SecurityRules {
        classifier: (!prototype_rules).then(Classifier::default),
        triage: vec![],
    };
    let mut values: Values = BTreeMap::new();
    let mut notes = Vec::new();
    let mut previous_keys: Option<BTreeMap<String, u32>> = None;
    let last = months.last().cloned().unwrap_or_default();
    let per_repo = quality["per_repo"].as_object().cloned().unwrap_or_default();
    for (i, month) in months.iter().enumerate() {
        let mut summaries = Vec::new();
        for (repo, by_month) in &per_repo {
            let Some(m) = by_month.get(month) else { continue };
            let sha = m["sha"].as_str().unwrap_or_default().to_string();
            let mut raw = RawScan {
                sha: sha.clone(),
                scc: Some(SccTotals {
                    code: m["code"].as_u64().unwrap_or(0),
                    complexity: m["complexity"].as_u64().unwrap_or(0),
                }),
                duplication: Some(Duplication {
                    duplicated_lines: m["dup_lines"].as_u64().unwrap_or(0),
                    lines: m["lines"].as_u64().unwrap_or(0),
                }),
                ..Default::default()
            };
            let recorded_scan = by_prefix
                .get(&format!("sec_{repo}_{}", &sha[..12.min(sha.len())]))
                .or_else(|| {
                    // The prototype's security run chose its own month-end commits; `--before` took the
                    // run's time of day, so it sometimes took a commit from the 1st (parity.md §6).
                    let dir = repo_dirs.get(repo)?;
                    let cutoff = format!("{}T23:59:59", next_day(month));
                    let mut best: Option<(String, &Value)> = None;
                    for (name, scan) in by_prefix.iter().filter(|(n, _)| n.starts_with(&format!("sec_{repo}_"))) {
                        let short = name.rsplit('_').next().unwrap_or_default();
                        let date = pm_git::Git::open(dir)
                            .run(&["show", "-s", "--format=%cI", short])
                            .ok()?;
                        let date = date.trim().to_string();
                        if date[..19] <= cutoff[..] && best.as_ref().is_none_or(|(d, _)| date > *d) {
                            best = Some((date, scan));
                        }
                    }
                    let (_, scan) = best?;
                    notes.push(format!(
                        "{month} {repo}: the prototype's security run used a different commit"
                    ));
                    Some(scan)
                });
            match recorded_scan {
                Some(scan) => {
                    let (kloc, sast, secrets, sup) = recorded(scan);
                    raw.kloc_all = kloc;
                    raw.sast = Some(sast);
                    raw.secrets = Some(secrets);
                    raw.suppressions = Some(sup);
                }
                None => notes.push(format!("{month} {repo}: no recorded security scan")),
            }
            // The golden set froze dependency scans once, at the pinned tips.
            if *month == last {
                let dir = golden.join("inputs/osv").join(key).join(repo);
                let scans: Vec<LockfileScan> = lockfiles
                    .get(repo)
                    .into_iter()
                    .flatten()
                    .filter_map(|lf| {
                        let text = std::fs::read_to_string(dir.join(format!("{}.json", lf.replace('/', "__")))).ok()?;
                        run::lockfile_scan(lf, &text).ok()
                    })
                    .collect();
                raw.lockfiles = Some(scans);
            }
            summaries.push(summarize(repo, &raw, &rules));
        }
        let (point, keys) = pool(&summaries);
        let mut put = |id: &str, v: Option<f64>| {
            values.entry(id.to_string()).or_default().insert(month.clone(), v);
        };
        for (id, v) in &point.values {
            put(id, *v);
        }
        put(
            "code",
            point
                .values
                .get("kloc")
                .copied()
                .flatten()
                .map(|k| (k * 1000.0).round()),
        );
        put("kloc", point.values.get("sast_kloc").copied().flatten());
        let (new, fixed) = match (&keys, &previous_keys) {
            (Some(k), Some(p)) if i > 0 => {
                let (n, f) = sast_diff(k, p);
                (Some(n), Some(f))
            }
            _ => (None, None),
        };
        put("sast_new", new);
        put("sast_fixed", fixed);
        previous_keys = keys;
    }
    Ok(Replay { values, notes })
}

/// The prototype's Security score from the golden counts (parity.md §3: recompute).
pub fn expected_security_score(rows: &BTreeMap<(String, String, String), f64>, key: &str, month: &str) -> Option<f64> {
    let g = |m: &str| rows.get(&(key.to_string(), m.to_string(), month.to_string())).copied();
    let secrets = g("secrets_high").unwrap_or(0.0) as u64;
    Some(security_score(
        g("vuln_critical")? as u64,
        g("vuln_high")? as u64,
        g("vuln_moderate")? as u64,
        g("vuln_low")? as u64,
        secrets,
    ))
}

/// Snapshot values a collect run produced, as metric → month → value.
pub fn from_points(points: &BTreeMap<chrono::NaiveDate, SnapshotPoint>, monthly: &Values) -> Values {
    let mut out: Values = monthly.clone();
    for (date, p) in points {
        let month = date.format("%Y-%m").to_string();
        out.entry("code".into()).or_default().insert(
            month.clone(),
            p.values.get("kloc").copied().flatten().map(|k| (k * 1000.0).round()),
        );
        out.entry("kloc".into())
            .or_default()
            .insert(month, p.values.get("sast_kloc").copied().flatten());
    }
    out
}
