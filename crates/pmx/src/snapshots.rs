//! The snapshot stage (plan §4 step 5): for every week-end and month-end, the commit each repo was
//! at; for every distinct commit, each available tool's raw result (cached per tool and commit);
//! then summaries pooled per date, the Security drill-down and the hotspots.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::{Days, NaiveDate};
use pm_classify::Classifier;
use pm_config::{SnapshotCadence, Workspace};
use pm_git::{Dialect, Git, LogRange};
use pm_metrics::model::{Hotspot, RepoHotspots, SecurityDetail, SnapshotInput};
use pm_metrics::period::{months, weeks};
use pm_metrics::snapshot::{RepoSummary, pool, pooled_packages};
use pm_progress::Progress;
use pm_snapshot::raw::{Duplication, LockfileScan, SastFinding, SccTotals, SecretHit};
use pm_snapshot::run;
use pm_snapshot::summarize::{SecurityRules, Triage, kloc};
use pm_snapshot::{RawScan, ToolId, Tools, summarize};
use sha2::{Digest, Sha256};

use crate::cache::Cache;

pub const SNAPSHOTS: &str = "snapshots";
const HOTSPOT_TOP: usize = 15;

/// What to snapshot and how.
pub struct SnapshotSettings<'a> {
    pub ws: &'a Workspace,
    pub as_of: NaiveDate,
    pub tools: &'a Tools,
    pub dialect: Dialect,
    /// Classifier for security test paths; `None` = the prototype's rule (parity harness).
    pub classifier: Option<Classifier>,
    pub use_cache: bool,
    /// Replay recorded osv-scanner output from `<dir>/<repo>/<lockfile, / → __>.json` instead of
    /// scanning (tests and the parity harness: the vulnerability database drifts).
    pub osv_replay: Option<PathBuf>,
    pub progress: Progress,
}

/// One repo to snapshot: its name, where it is, and the tip being measured.
#[derive(Clone, Debug)]
pub struct SnapshotRepo {
    pub name: String,
    pub dir: PathBuf,
    pub tip: String,
    pub lockfiles: Option<Vec<String>>,
}

/// The dates measured: every bucket end up to `as_of` (week-ends only with the weekly cadence).
pub fn snapshot_dates(range_start: NaiveDate, as_of: NaiveDate, cadence: SnapshotCadence) -> Vec<NaiveDate> {
    let mut dates: BTreeSet<NaiveDate> = months(range_start, as_of).iter().map(|b| b.end.min(as_of)).collect();
    if cadence == SnapshotCadence::Weekly {
        dates.extend(weeks(range_start, as_of).iter().map(|b| b.end.min(as_of)));
    }
    dates.into_iter().collect()
}

fn hash(parts: &[&str]) -> String {
    let d = Sha256::digest(parts.join("\u{1f}").as_bytes());
    d.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// Cache keys per tool: a different tool version or ruleset is a different result.
struct Keys {
    scc: Option<String>,
    jscpd: Option<String>,
    gitleaks: Option<String>,
    semgrep: Option<String>,
    osv_version: Option<String>,
}

impl Keys {
    fn new(tools: &Tools, rules: &[PathBuf]) -> Keys {
        let v = |id: ToolId| tools.get(id).map(|t| format!("{}@{}", id.name(), t.version));
        let rules_hash = {
            let mut h = Sha256::new();
            for f in run::rule_files(rules) {
                h.update(
                    f.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                );
                h.update(std::fs::read(&f).unwrap_or_default());
            }
            h.finalize()
                .iter()
                .take(8)
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        Keys {
            scc: v(ToolId::Scc),
            jscpd: v(ToolId::Jscpd),
            gitleaks: v(ToolId::Gitleaks),
            semgrep: (!rules.is_empty())
                .then(|| v(ToolId::Semgrep))
                .flatten()
                .map(|k| format!("{k}+rules:{rules_hash}")),
            osv_version: v(ToolId::OsvScanner),
        }
    }
}

/// Everything known about one commit of one repo, from the cache or the tools.
#[derive(Default)]
struct Parts {
    scc: Option<SccTotals>,
    scc_all: Option<u64>,
    dup: Option<Duplication>,
    secrets: Option<Vec<SecretHit>>,
    sast: Option<Vec<SastFinding>>,
    suppressions: Option<u64>,
    lockfiles: Option<Vec<String>>,
}

fn get<T: serde::de::DeserializeOwned>(
    cache: Option<&Cache>,
    repo: &str,
    sha: &str,
    key: &Option<String>,
) -> Option<T> {
    let (c, k) = (cache?, key.as_ref()?);
    c.tool_result(repo, sha, k).ok().flatten()
}

fn put<T: serde::Serialize>(cache: Option<&Cache>, repo: &str, sha: &str, key: &Option<String>, v: &T) -> Result<()> {
    if let (Some(c), Some(k)) = (cache, key) {
        c.put_tool_result(repo, sha, k, v)?;
    }
    Ok(())
}

/// Measure one commit: reuse cached tool results, export the tree only if something is missing.
fn measure(
    st: &SnapshotSettings,
    keys: &Keys,
    rules: &[PathBuf],
    repo: &SnapshotRepo,
    sha: &str,
    cache: Option<&Cache>,
) -> Result<Parts> {
    let t = st.tools;
    let name = repo.name.as_str();
    let scc_all_key = keys.scc.as_ref().map(|k| format!("{k}+all"));
    // The lockfile list depends on the repo's override (or auto-discovery).
    let lockfiles_key = Some(format!(
        "lockfiles@1+{}",
        repo.lockfiles
            .as_ref()
            .map(|l| hash(&l.iter().map(String::as_str).collect::<Vec<_>>()))
            .unwrap_or_else(|| "auto".into())
    ));
    let mut p = Parts {
        scc: get(cache, name, sha, &keys.scc),
        scc_all: get(cache, name, sha, &scc_all_key.clone()),
        dup: get(cache, name, sha, &keys.jscpd),
        secrets: get(cache, name, sha, &keys.gitleaks),
        sast: get(cache, name, sha, &keys.semgrep),
        suppressions: get(cache, name, sha, &Some("suppressions@1".into())),
        lockfiles: get(cache, name, sha, &lockfiles_key),
    };
    let need = |have: bool, tool: bool| tool && !have;
    let missing = need(p.scc.is_some() && p.scc_all.is_some(), t.scc.is_some())
        || need(p.dup.is_some(), t.jscpd.is_some())
        || need(p.secrets.is_some(), t.gitleaks.is_some())
        || need(p.sast.is_some(), keys.semgrep.is_some())
        || p.suppressions.is_none()
        || p.lockfiles.is_none();
    if !missing {
        return Ok(p);
    }
    let tree = tempfile::tempdir()?;
    Git::open(&repo.dir)
        .archive_to(sha, tree.path())
        .with_context(|| format!("{name}: exporting {sha}"))?;
    let dir = tree.path();
    let detail = |tool: &str| st.progress.detail(SNAPSHOTS, format!("{tool} {name} @ {}", &sha[..8]));
    let warn = |tool: &str, e: &dyn std::fmt::Display| st.progress.warn(format!("{name} @ {}: {tool}: {e}", &sha[..8]));
    if let (Some(tool), true) = (&t.scc, p.scc.is_none() || p.scc_all.is_none()) {
        detail("scc");
        match (run::scc_totals(tool, dir, true), run::scc_totals(tool, dir, false)) {
            (Ok(a), Ok(b)) => {
                put(cache, name, sha, &keys.scc, &a)?;
                put(cache, name, sha, &scc_all_key, &b.code)?;
                p.scc = Some(a);
                p.scc_all = Some(b.code);
            }
            (Err(e), _) | (_, Err(e)) => warn("scc", &e),
        }
    }
    if let (Some(tool), true) = (&t.jscpd, p.dup.is_none()) {
        detail("jscpd");
        match run::jscpd(tool, dir) {
            Ok(d) => {
                put(cache, name, sha, &keys.jscpd, &d)?;
                p.dup = Some(d);
            }
            Err(e) => warn("jscpd", &e),
        }
    }
    if let (Some(tool), true) = (&t.gitleaks, p.secrets.is_none()) {
        detail("gitleaks");
        match run::gitleaks(tool, dir) {
            Ok(h) => {
                put(cache, name, sha, &keys.gitleaks, &h)?;
                p.secrets = Some(h);
            }
            Err(e) => warn("gitleaks", &e),
        }
    }
    if let (Some(tool), true) = (&t.semgrep, keys.semgrep.is_some() && p.sast.is_none()) {
        detail("semgrep");
        match run::semgrep(tool, dir, rules) {
            Ok(Some(f)) => {
                put(cache, name, sha, &keys.semgrep, &f)?;
                p.sast = Some(f);
            }
            // Timeouts persisted: not measured, and not cached so the next run tries again.
            Ok(None) => warn("semgrep", &"rule timeouts persisted; SAST not measured for this commit"),
            Err(e) => warn("semgrep", &e),
        }
    }
    if p.suppressions.is_none() {
        let n = run::suppressions(dir);
        put(cache, name, sha, &Some("suppressions@1".into()), &n)?;
        p.suppressions = Some(n);
    }
    if p.lockfiles.is_none() {
        let found = match &repo.lockfiles {
            Some(list) => list.iter().filter(|l| dir.join(l).is_file()).cloned().collect(),
            None => run::discover_lockfiles(dir),
        };
        put(cache, name, sha, &lockfiles_key, &found)?;
        p.lockfiles = Some(found);
    }
    Ok(p)
}

/// osv-scanner per lockfile, keyed by its content: identical lockfiles across week-ends are
/// scanned once a day.
fn scan_lockfiles(
    st: &SnapshotSettings,
    keys: &Keys,
    repo: &SnapshotRepo,
    sha: &str,
    lockfiles: &[String],
    cache: Option<&Cache>,
    today: &str,
) -> Result<Option<Vec<LockfileScan>>> {
    let git = Git::open(&repo.dir);
    let mut out = Vec::new();
    for lf in lockfiles {
        let raw = if let Some(dir) = &st.osv_replay {
            let file = dir.join(&repo.name).join(format!("{}.json", lf.replace('/', "__")));
            match std::fs::read_to_string(&file) {
                Ok(t) => t,
                Err(_) => continue,
            }
        } else {
            let (Some(tool), Some(version)) = (&st.tools.osv, &keys.osv_version) else {
                return Ok(None);
            };
            let blob = git.blob_id(sha, lf).unwrap_or_default();
            let key = format!("{blob}|{lf}|{version}|{today}");
            match cache.and_then(|c| c.osv(&key).ok().flatten()) {
                Some(r) => r,
                None => {
                    st.progress.detail(SNAPSHOTS, format!("osv-scanner {} {lf}", repo.name));
                    let content = git.show_bytes(sha, lf)?;
                    match run::osv(tool, lf, &content) {
                        Ok(r) => {
                            if let Some(c) = cache {
                                c.put_osv(&key, &r)?;
                            }
                            r
                        }
                        Err(e) => {
                            st.progress.warn(format!("{} {lf}: {e}", repo.name));
                            return Ok(None);
                        }
                    }
                }
            }
        };
        out.push(run::lockfile_scan(lf, &raw).map_err(anyhow::Error::msg)?);
    }
    Ok(Some(out))
}

/// Churn × complexity at the as-of commit (spec §3.5).
fn hotspots(st: &SnapshotSettings, repo: &SnapshotRepo) -> Result<Option<RepoHotspots>> {
    let Some(scc) = &st.tools.scc else { return Ok(None) };
    // The 365 days ending on as_of.
    let since = st.as_of - Days::new(364);
    let git = Git::open(&repo.dir).with_dialect(st.dialect);
    let Some(sha) = git.sha_before(&repo.tip, st.as_of + Days::new(1))? else {
        return Ok(None);
    };
    let tree = tempfile::tempdir()?;
    git.archive_to(&sha, tree.path())?;
    let cx: BTreeMap<String, (u64, u64)> = run::scc_by_file(scc, tree.path())?
        .into_iter()
        .map(|(f, code, c)| (f, (code, c)))
        .collect();
    let commits = git.commits(&LogRange::new(&sha).committed_since(since), &mut |_| {})?;
    // path → (revisions, churn, order of first appearance: newest commit first).
    let mut revs: BTreeMap<String, (u64, u64, usize)> = BTreeMap::new();
    for c in commits.iter().filter(|c| c.author_date >= since) {
        for f in &c.files {
            // Binary files (numstat `-`) carry no line counts.
            if (f.added, f.deleted) == (0, 0) || f.path.split('/').any(|s| run::EXCLUDE_DIRS.contains(&s)) {
                continue;
            }
            let next = revs.len();
            let e = revs.entry(f.path.clone()).or_insert((0, 0, next));
            e.0 += 1;
            e.1 += f.added + f.deleted;
        }
    }
    let mut files: Vec<(usize, Hotspot)> = revs
        .iter()
        .filter_map(|(path, (r, churn, seen))| {
            let (code, complexity) = *cx.get(path)?;
            (complexity > 0).then(|| {
                (
                    *seen,
                    Hotspot {
                        file: path.clone(),
                        score: r * complexity,
                        revisions: *r,
                        complexity,
                        code,
                        churn: *churn,
                    },
                )
            })
        })
        .collect();
    // Equal scores: the most recently changed file first.
    files.sort_by(|a, b| b.1.score.cmp(&a.1.score).then(a.0.cmp(&b.0)));
    let files: Vec<Hotspot> = files.into_iter().take(HOTSPOT_TOP).map(|(_, h)| h).collect();
    Ok(Some(RepoHotspots {
        repo: repo.name.clone(),
        sha,
        since,
        files,
        files_changed: revs.len() as u64,
    }))
}

fn not_measured(tools: &Tools, rules: &[PathBuf]) -> Vec<String> {
    let mut out = Vec::new();
    if tools.jscpd.is_none() {
        out.push("duplication".into());
    }
    if tools.scc.is_none() {
        out.extend(["complexity".into(), "hotspots".into()]);
    }
    if tools.osv.is_none() {
        out.push("dependency_vulnerabilities".into());
    }
    if tools.gitleaks.is_none() {
        out.push("secrets".into());
    }
    if tools.semgrep.is_none() || rules.is_empty() {
        out.push("sast".into());
    }
    out
}

/// Run the stage. Errors in one tool become warnings and "not measured"; only git failures stop it.
pub fn run_stage(
    st: &SnapshotSettings,
    repos: &[SnapshotRepo],
    cadence: SnapshotCadence,
    jobs: usize,
) -> Result<SnapshotInput> {
    let ws = st.ws;
    let config = &ws.config;
    let rules: Vec<PathBuf> = config
        .snapshots
        .sast_rules
        .iter()
        .map(|r| pm_config::expand_path(r, &ws.root))
        .collect();
    let keys = Keys::new(st.tools, &rules);
    let from = config
        .snapshots
        .since
        .as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .unwrap_or(config.project.range_start)
        .max(config.project.range_start);
    let dates = snapshot_dates(from, st.as_of, cadence);
    let today = chrono::Local::now().date_naive().to_string();

    // Commit per (repo, date), and the distinct commits to measure.
    let mut at: BTreeMap<NaiveDate, Vec<(usize, String)>> = BTreeMap::new();
    let mut units: BTreeSet<(usize, String)> = BTreeSet::new();
    for (i, r) in repos.iter().enumerate() {
        let git = Git::open(&r.dir);
        for d in &dates {
            if let Some(sha) = git.sha_before(&r.tip, *d + Days::new(1))? {
                at.entry(*d).or_default().push((i, sha.clone()));
                units.insert((i, sha));
            }
        }
    }
    let units: Vec<(usize, String)> = units.into_iter().collect();
    let p = &st.progress;
    p.plan(SNAPSHOTS, units.len() as u64, 0, false);
    p.set_stage_workers(SNAPSHOTS, jobs.clamp(1, units.len().max(1)));
    p.start(SNAPSHOTS);

    let next = AtomicUsize::new(0);
    let results: Mutex<BTreeMap<(usize, String), RawScan>> = Mutex::new(BTreeMap::new());
    let failure: Mutex<Option<anyhow::Error>> = Mutex::new(None);
    std::thread::scope(|s| {
        for _ in 0..jobs.clamp(1, units.len().max(1)) {
            s.spawn(|| {
                let cache = if st.use_cache {
                    Cache::open(&ws.state_dir().join("cache.sqlite")).ok()
                } else {
                    None
                };
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some((ri, sha)) = units.get(i) else { break };
                    let repo = &repos[*ri];
                    let started = Instant::now();
                    let outcome = measure(st, &keys, &rules, repo, sha, cache.as_ref()).and_then(|parts| {
                        let lockfiles = match &parts.lockfiles {
                            Some(l) => scan_lockfiles(st, &keys, repo, sha, l, cache.as_ref(), &today)?,
                            None => None,
                        };
                        Ok(RawScan {
                            sha: sha.clone(),
                            scc: parts.scc,
                            kloc_all: parts.scc_all.map(kloc),
                            duplication: parts.dup,
                            secrets: parts.secrets,
                            sast: parts.sast,
                            suppressions: parts.suppressions,
                            lockfiles,
                        })
                    });
                    match outcome {
                        Ok(raw) => {
                            results
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .insert((*ri, sha.clone()), raw);
                        }
                        Err(e) => {
                            failure.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert(e);
                        }
                    }
                    if let Some(c) = &cache {
                        let _ = c.record_timing(SNAPSHOTS, 1, started.elapsed().as_secs_f64());
                    }
                    p.advance(SNAPSHOTS, 1);
                }
            });
        }
    });
    if let Some(e) = failure.into_inner().unwrap_or_else(|e| e.into_inner()) {
        return Err(e);
    }
    let results = results.into_inner().unwrap_or_else(|e| e.into_inner());

    let rules_cls = SecurityRules {
        classifier: st.classifier.clone(),
        triage: config
            .secrets_triage
            .iter()
            .map(|t| Triage {
                repo: t.repo.clone(),
                file: t.file.clone(),
                rule: t.rule.clone(),
            })
            .collect(),
    };
    let summaries: BTreeMap<(usize, String), RepoSummary> = results
        .iter()
        .map(|((i, sha), raw)| ((*i, sha.clone()), summarize(&repos[*i].name, raw, &rules_cls)))
        .collect();
    let mut input = SnapshotInput {
        not_measured: not_measured(st.tools, &rules),
        ..Default::default()
    };
    for (date, list) in &at {
        let repo_summaries: Vec<RepoSummary> = list.iter().filter_map(|k| summaries.get(k).cloned()).collect();
        let (point, keys) = pool(&repo_summaries);
        input.points.insert(*date, point);
        input.sast_keys.insert(*date, keys);
    }
    if let Some((date, list)) = at.iter().next_back() {
        let latest: Vec<RepoSummary> = list.iter().filter_map(|k| summaries.get(k).cloned()).collect();
        if latest.iter().all(|r| r.vulns.is_some()) && !latest.is_empty() {
            let mut top = pooled_packages(&latest);
            top.truncate(8);
            input.security = Some(SecurityDetail {
                date: *date,
                top_packages: top,
                lockfiles: list
                    .iter()
                    .filter_map(|(i, sha)| {
                        let l = results.get(&(*i, sha.clone()))?.lockfiles.as_ref()?;
                        Some((repos[*i].name.clone(), l.iter().map(|x| x.path.clone()).collect()))
                    })
                    .collect(),
            });
        }
    }
    p.finish(SNAPSHOTS);
    for r in repos {
        match hotspots(st, r) {
            Ok(Some(h)) => input.hotspots.push(h),
            Ok(None) => {}
            Err(e) => p.warn(format!("{}: hotspots: {e}", r.name)),
        }
    }
    Ok(input)
}

/// Rule files that exist, for `pmx check`/`doctor`.
pub fn sast_rules(ws: &Workspace) -> Vec<PathBuf> {
    run::rule_files(
        &ws.config
            .snapshots
            .sast_rules
            .iter()
            .map(|r| pm_config::expand_path(r, &ws.root))
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let monthly = snapshot_dates(d("2026-08-01"), d("2026-10-09"), SnapshotCadence::Monthly);
        assert_eq!(monthly, vec![d("2026-08-31"), d("2026-09-30"), d("2026-10-09")]);
        let weekly = snapshot_dates(d("2026-09-01"), d("2026-09-30"), SnapshotCadence::Weekly);
        assert_eq!(
            weekly,
            vec![
                d("2026-09-06"),
                d("2026-09-13"),
                d("2026-09-20"),
                d("2026-09-27"),
                d("2026-09-30")
            ]
        );
    }
}
