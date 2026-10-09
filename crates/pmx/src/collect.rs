//! `pmx collect` (plan §4, stages 1–4 and 8): plan → fetch → git ingest → rework walk → roll-up.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use chrono::{Days as ChronoDays, NaiveDate};
use pm_classify::{Classifier, TestRules};
use pm_config::{Identity, RepoConfig, Workspace};
use pm_git::{Dialect, Git, LogRange, ReworkCounts};
use pm_metrics::model::{BuildInput, ProjectInfo, RepoInfo};
use pm_metrics::{Days, LeadsFile, ProjectFile, merge_days};
use pm_progress::{Progress, fmt_n};
use sha2::{Digest, Sha256};

use crate::cache::Cache;
use crate::ingest::{IngestContext, RepoIngest, add_rework, ingest, splice_rework};

/// Lines deleted within this many days of being added are rework (spec §3.1).
pub const REWORK_YOUNG_DAYS: i64 = 21;

pub const FETCH: &str = "fetch";
pub const INGEST: &str = "ingest";
pub const REWORK: &str = "rework";

/// Seconds per unit before any run has been timed (one worker; measured on an Apple M-series
/// laptop, generous on purpose).
const DEFAULT_COST: [(&str, f64); 3] = [(FETCH, 2.0), (INGEST, 0.001), (REWORK, 0.0015)];

#[derive(Clone)]
pub struct CollectOptions {
    /// Replaces the run date everywhere (spec §1.1).
    pub as_of: NaiveDate,
    pub fetch: bool,
    pub use_cache: bool,
    /// Continue from the cached SHA when the new tip descends from it (default). `false`
    /// recomputes every repo but still updates the cache.
    pub incremental: bool,
    /// Read git and detect tests the way the private prototype did. For the parity harness only
    /// (parity.md); never set by the CLI.
    pub prototype_compat: bool,
    pub progress: Progress,
}

impl CollectOptions {
    pub fn new(as_of: NaiveDate) -> Self {
        CollectOptions {
            as_of,
            fetch: true,
            use_cache: true,
            incremental: true,
            prototype_compat: false,
            progress: Progress::off(),
        }
    }

    fn dialect(&self) -> Dialect {
        if self.prototype_compat {
            Dialect::Prototype
        } else {
            Dialect::Spec
        }
    }
}

pub struct Collected {
    pub project: ProjectFile,
    pub leads: LeadsFile,
    pub warnings: Vec<String>,
}

/// What a stage has to do for one repo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Work {
    /// The cache holds the result for this tip.
    Cached,
    /// Continue from the cached result at `from`, an ancestor of the tip.
    Incremental {
        from: String,
        units: u64,
    },
    Full {
        units: u64,
    },
    /// A URL repo that is not cloned yet.
    Unknown,
}

impl Work {
    pub fn units(&self) -> u64 {
        match self {
            Work::Incremental { units, .. } | Work::Full { units } => *units,
            _ => 0,
        }
    }

    pub fn describe(&self, unit: &str) -> String {
        match self {
            Work::Cached => "cached".into(),
            Work::Incremental { units, .. } => format!("{} new {unit}", fmt_n(*units)),
            Work::Full { units } => format!("{} {unit}", fmt_n(*units)),
            Work::Unknown => "not cloned yet".into(),
        }
    }
}

/// The plan for one repo (plan §5.1).
#[derive(Clone, Debug)]
pub struct RepoPlan {
    pub name: String,
    pub dir: PathBuf,
    pub rev: Option<String>,
    pub tip: Option<String>,
    pub needs_fetch: bool,
    pub ingest: Work,
    /// Commits already in the cached ingest result.
    pub ingest_cached: u64,
    pub rework: Work,
    /// Commits the cached rework walk covered.
    pub rework_cached: u64,
}

/// A repo's rework walk result as cached.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ReworkResult {
    pub counts: ReworkCounts,
    /// Commits walked to produce it (cumulative over incremental runs).
    pub walked: u64,
}

struct Settings {
    ingest: String,
    rework: String,
}

fn digest(v: serde_json::Value) -> String {
    Sha256::digest(v.to_string().as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Everything besides the tip SHA that changes a stage's result. People and roles don't affect
/// the rework walk, so editing `[people]` does not redo it.
fn settings(ws: &Workspace, repo: &RepoConfig, opts: &CollectOptions) -> Settings {
    let c = &ws.config;
    let common = serde_json::json!({
        "pmx": env!("CARGO_PKG_VERSION"),
        "range_start": c.project.range_start,
        "prototype_compat": opts.prototype_compat,
    });
    Settings {
        ingest: digest(serde_json::json!({
            "common": common,
            "role": repo.role.to_string(),
            "role_rules": c.role_rules,
            "people": c.people,
        })),
        rework: digest(common),
    }
}

fn cache_path(ws: &Workspace) -> PathBuf {
    ws.state_dir().join("cache.sqlite")
}

fn open_cache(ws: &Workspace, opts: &CollectOptions) -> Result<Option<Cache>> {
    if opts.use_cache {
        Cache::open(&cache_path(ws)).map(Some)
    } else {
        Ok(None)
    }
}

fn rework_since(range_start: NaiveDate) -> NaiveDate {
    range_start - ChronoDays::new(REWORK_YOUNG_DAYS as u64)
}

/// Plan one repo from what is on disk now (no fetch, no clone).
pub fn plan_repo(ws: &Workspace, repo: &RepoConfig, opts: &CollectOptions, cache: Option<&Cache>) -> Result<RepoPlan> {
    let name = repo.display_name();
    let dir = ws.repo_dir(repo);
    let needs_fetch = (repo.url.is_some() && !dir.exists()) || (opts.fetch && repo.rev.is_none());
    let mut plan = RepoPlan {
        name: name.clone(),
        dir: dir.clone(),
        rev: repo.revision().map(String::from),
        tip: None,
        needs_fetch,
        ingest: Work::Unknown,
        ingest_cached: 0,
        rework: Work::Unknown,
        rework_cached: 0,
    };
    if !dir.exists() {
        if repo.url.is_some() {
            return Ok(plan);
        }
        return Err(anyhow!("repo `{name}`: {} does not exist", dir.display()));
    }
    let git = Git::open(&dir).with_dialect(opts.dialect());
    if !git.is_repo() {
        return Err(anyhow!("repo `{name}`: {} is not a git repository", dir.display()));
    }
    let rev = plan.rev.clone().unwrap_or_else(|| git.default_revision());
    let tip = git
        .rev_parse(&rev)
        .with_context(|| format!("repo `{name}`: cannot resolve `{rev}`"))?;
    plan.rev = Some(rev);
    let range_start = ws.config.project.range_start;
    let keys = settings(ws, repo, opts);

    // `--full` plans as if nothing were cached.
    let cache = cache.filter(|_| opts.incremental);
    let ingested = match cache {
        Some(c) => c.get::<RepoIngest>(&name, INGEST, &keys.ingest)?,
        None => None,
    };
    plan.ingest = match &ingested {
        Some((sha, _)) if *sha == tip => Work::Cached,
        Some((sha, _)) if opts.incremental && git.is_ancestor(sha, &tip) => Work::Incremental {
            from: sha.clone(),
            units: git.count_commits(&LogRange::new(&tip).since_commit(sha).committed_since(range_start))?,
        },
        _ => Work::Full {
            units: git.count_commits(&LogRange::new(&tip).committed_since(range_start))?,
        },
    };
    if matches!(plan.ingest, Work::Cached | Work::Incremental { .. }) {
        plan.ingest_cached = ingested.map(|(_, i)| i.commits_read).unwrap_or(0);
    }

    let reworked = match cache {
        Some(c) => c.get::<ReworkResult>(&name, REWORK, &keys.rework)?,
        None => None,
    };
    plan.rework = match &reworked {
        Some((sha, _)) if *sha == tip => Work::Cached,
        Some((sha, _)) if opts.incremental && git.is_ancestor(sha, &tip) => {
            // Estimate: the walk restarts 21 days before the old tip.
            let old = git.run(&["show", "-s", "--date=short", "--format=%ad", sha])?;
            let since = NaiveDate::parse_from_str(old.trim(), "%Y-%m-%d")
                .map(rework_since)
                .unwrap_or(range_start);
            Work::Incremental {
                from: sha.clone(),
                units: git.count_commits(&LogRange::new(&tip).committed_since(since.max(rework_since(range_start))))?,
            }
        }
        _ => Work::Full {
            units: git.count_commits(&LogRange::new(&tip).committed_since(rework_since(range_start)))?,
        },
    };
    if matches!(plan.rework, Work::Cached | Work::Incremental { .. }) {
        plan.rework_cached = reworked.map(|(_, r)| r.walked).unwrap_or(0);
    }
    plan.tip = Some(tip);
    Ok(plan)
}

pub fn plan(ws: &Workspace, opts: &CollectOptions) -> Result<Vec<RepoPlan>> {
    let cache = open_cache(ws, opts)?;
    ws.config
        .repos
        .iter()
        .map(|r| plan_repo(ws, r, opts, cache.as_ref()))
        .collect()
}

/// Register the stages and their planned work on `progress`.
pub fn plan_progress(ws: &Workspace, opts: &CollectOptions, plans: &[RepoPlan]) -> Result<()> {
    let p = &opts.progress;
    let cache = open_cache(ws, opts)?;
    let cost = |stage: &str| -> Result<f64> {
        let learned = match &cache {
            Some(c) => c.cost(stage)?,
            None => None,
        };
        Ok(learned.unwrap_or_else(|| {
            DEFAULT_COST
                .iter()
                .find(|(s, _)| *s == stage)
                .map(|(_, c)| *c)
                .unwrap_or(0.0)
        }))
    };
    p.add_stage(FETCH, "fetch", "repos", cost(FETCH)?);
    p.add_stage(INGEST, "git ingest", "commits", cost(INGEST)?);
    p.add_stage(REWORK, "rework walk", "commits", cost(REWORK)?);
    let fetches = plans.iter().filter(|r| r.needs_fetch).count() as u64;
    if fetches == 0 {
        p.skip(FETCH, "skipped (--no-fetch or pinned revs)");
    } else {
        p.plan(FETCH, fetches, 0, false);
    }
    update_progress(p, plans);
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    p.set_workers(workers.min(plans.len().max(1)));
    Ok(())
}

/// Ingest and rework work, re-run after fetching.
fn update_progress(p: &Progress, plans: &[RepoPlan]) {
    let unknown = plans.iter().any(|r| r.ingest == Work::Unknown);
    p.plan(
        INGEST,
        plans.iter().map(|r| r.ingest.units()).sum(),
        plans.iter().map(|r| r.ingest_cached).sum(),
        unknown,
    );
    p.plan(
        REWORK,
        plans.iter().map(|r| r.rework.units()).sum(),
        plans.iter().map(|r| r.rework_cached).sum(),
        true,
    );
}

struct Shared<'a> {
    ws: &'a Workspace,
    opts: &'a CollectOptions,
    classifier: Classifier,
    identity: Identity,
}

struct RepoResult {
    info: RepoInfo,
    ingest: RepoIngest,
    rework: ReworkResult,
}

fn fetch_repo(repo: &RepoConfig, plan: &RepoPlan, p: &Progress) -> Result<()> {
    if let Some(url) = &repo.url {
        if !plan.dir.exists() {
            p.detail(FETCH, format!("cloning {}", plan.name));
            Git::clone_bare(url, &plan.dir).with_context(|| format!("cloning `{}`", plan.name))?;
            return Ok(());
        }
    }
    p.detail(FETCH, plan.name.clone());
    if let Err(e) = Git::open(&plan.dir).fetch() {
        p.warn(format!("{}: fetch failed, measuring local refs ({e})", plan.name));
    }
    Ok(())
}

fn run_repo(
    sh: &Shared,
    repo: &RepoConfig,
    plan: &RepoPlan,
    done: &[AtomicUsize; 2],
    n_repos: usize,
) -> Result<RepoResult> {
    let (ws, opts, p) = (sh.ws, sh.opts, &sh.opts.progress);
    let name = &plan.name;
    let tip = plan.tip.clone().ok_or_else(|| anyhow!("repo `{name}`: no tip"))?;
    let git = Git::open(&plan.dir).with_dialect(opts.dialect());
    let cache = open_cache(ws, opts)?;
    let keys = settings(ws, repo, opts);
    let range_start = ws.config.project.range_start;
    let cx = IngestContext {
        role: repo.role,
        classifier: &sh.classifier,
        identity: &sh.identity,
        range_start,
    };
    let finish_stage = |i: usize, stage: &str| {
        if done[i].fetch_add(1, Ordering::SeqCst) + 1 == n_repos {
            p.finish(stage);
        }
    };

    // Git ingest.
    p.start(INGEST);
    p.detail(INGEST, name.clone());
    let started = Instant::now();
    let cached = match &cache {
        Some(c) => c.get::<RepoIngest>(name, INGEST, &keys.ingest)?,
        None => None,
    };
    let mut advance = |_: &pm_git::Commit| p.advance(INGEST, 1);
    let ingested = match (&plan.ingest, cached) {
        (Work::Cached, Some((_, i))) => i,
        (Work::Incremental { from, .. }, Some((sha, mut old))) if *from == sha => {
            let range = LogRange::new(&tip).since_commit(from).committed_since(range_start);
            let commits = git
                .commits(&range, &mut advance)
                .with_context(|| format!("repo `{name}`: git log"))?;
            old.extend(ingest(&commits, &cx));
            old
        }
        _ => {
            let range = LogRange::new(&tip).committed_since(range_start);
            let commits = git
                .commits(&range, &mut advance)
                .with_context(|| format!("repo `{name}`: git log"))?;
            ingest(&commits, &cx)
        }
    };
    if plan.ingest != Work::Cached {
        if let Some(c) = &cache {
            c.put(name, INGEST, &keys.ingest, &tip, &ingested)?;
            c.record_timing(INGEST, plan.ingest.units(), started.elapsed().as_secs_f64())?;
        }
    }
    finish_stage(0, INGEST);

    // Rework walk.
    p.start(REWORK);
    p.detail(REWORK, name.clone());
    let started = Instant::now();
    let cached = match &cache {
        Some(c) => c.get::<ReworkResult>(name, REWORK, &keys.rework)?,
        None => None,
    };
    let is_source = |path: &str| sh.classifier.is_source(path);
    let mut walked = 0u64;
    let mut on_commit = || {
        walked += 1;
        p.advance(REWORK, 1);
    };
    let floor = rework_since(range_start);
    let mut previous = 0;
    let counts = match (&plan.rework, cached) {
        (Work::Cached, Some((_, r))) => {
            previous = r.walked;
            r.counts
        }
        (Work::Incremental { from, .. }, Some((sha, old))) if *from == sha => {
            previous = old.walked;
            let new_dates = git.author_dates(&LogRange::new(&tip).since_commit(from))?;
            match new_dates.iter().min() {
                // Only merges since the cached tip: nothing to redo.
                None => old.counts,
                Some(d0) => {
                    let since = rework_since(*d0).max(floor);
                    let fresh = git
                        .rework(&tip, since, REWORK_YOUNG_DAYS, &is_source, &mut on_commit)
                        .with_context(|| format!("repo `{name}`: rework walk"))?;
                    splice_rework(&old.counts, &fresh, (*d0).max(floor))
                }
            }
        }
        _ => git
            .rework(&tip, floor, REWORK_YOUNG_DAYS, &is_source, &mut on_commit)
            .with_context(|| format!("repo `{name}`: rework walk"))?,
    };
    let rework = ReworkResult {
        counts,
        walked: previous + walked,
    };
    if plan.rework != Work::Cached {
        if let Some(c) = &cache {
            c.put(name, REWORK, &keys.rework, &tip, &rework)?;
            c.record_timing(REWORK, walked, started.elapsed().as_secs_f64())?;
        }
    }
    finish_stage(1, REWORK);

    Ok(RepoResult {
        info: RepoInfo {
            name: name.clone(),
            role: repo.role.to_string(),
            tip_date: git.commit_date(&tip)?,
            sha: tip,
        },
        ingest: ingested,
        rework,
    })
}

/// Run `f` over `items` on up to `workers` threads, keeping the input order in the output.
fn pool<T: Sync, R: Send>(items: &[T], workers: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|s| {
        for _ in 0..workers.clamp(1, items.len().max(1)) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    if i >= items.len() {
                        break;
                    }
                    let r = f(&items[i]);
                    out.lock().unwrap_or_else(|e| e.into_inner())[i] = Some(r);
                }
            });
        }
    });
    out.into_inner()
        .unwrap_or_else(|e| e.into_inner())
        .into_iter()
        .map(|r| r.expect("every item ran"))
        .collect()
}

/// Collect the workspace. Progress is reported on `opts.progress`; the caller starts nothing and
/// ends it (`Progress::end`) after writing the outputs.
pub fn collect(ws: &Workspace, opts: &CollectOptions) -> Result<Collected> {
    let config = &ws.config;
    let p = &opts.progress;
    let mut plans = plan(ws, opts)?;
    plan_progress(ws, opts, &plans)?;
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    p.begin();

    // Fetch (and clone URL repos), then re-plan against the new tips.
    let jobs: Vec<(&RepoConfig, &RepoPlan)> = config
        .repos
        .iter()
        .zip(&plans)
        .filter(|(_, pl)| pl.needs_fetch)
        .collect();
    if !jobs.is_empty() {
        p.start(FETCH);
        let results = pool(&jobs, workers, |(repo, pl)| {
            let r = fetch_repo(repo, pl, p);
            p.advance(FETCH, 1);
            r
        });
        p.finish(FETCH);
        results.into_iter().collect::<Result<Vec<_>>>()?;
        let mut replan_opts = opts.clone();
        replan_opts.fetch = false;
        plans = plan(ws, &replan_opts)?;
        update_progress(p, &plans);
    }

    let shared = Shared {
        ws,
        opts,
        classifier: config.classifier(if opts.prototype_compat {
            TestRules::Prototype
        } else {
            TestRules::Spec
        }),
        identity: Identity::new(&config.people),
    };
    let done = [AtomicUsize::new(0), AtomicUsize::new(0)];
    let n = plans.len();
    let jobs: Vec<(&RepoConfig, &RepoPlan)> = config.repos.iter().zip(&plans).collect();
    let results = pool(&jobs, workers, |(repo, pl)| run_repo(&shared, repo, pl, &done, n));

    let range_start = config.project.range_start;
    let mut days = Days::new();
    let mut dates = BTreeSet::new();
    let mut repos = Vec::new();
    for r in results {
        let r = r?;
        let mut repo_days = r.ingest.days;
        add_rework(&mut repo_days, &r.rework.counts, range_start);
        let window: Days = repo_days
            .range(range_start..=opts.as_of)
            .map(|(d, day)| (*d, day.clone()))
            .collect();
        merge_days(&mut days, &window);
        dates.extend(r.ingest.commit_dates.range(..=opts.as_of).copied());
        repos.push(r.info);
    }

    let mut tool_versions = BTreeMap::new();
    tool_versions.insert("pmx".to_string(), env!("CARGO_PKG_VERSION").to_string());
    tool_versions.insert("git".to_string(), Git::version()?);
    let (project, leads) = pm_metrics::build(BuildInput {
        project: ProjectInfo {
            name: config.project.name.clone(),
            range_start,
            breadth_roles: config.project.breadth_roles.clone(),
            ai_attribution: config.project.ai_attribution,
            repos,
        },
        days,
        as_of: opts.as_of,
        latest_commit: dates.last().copied(),
        generated_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        tool_versions,
        dialect: if opts.prototype_compat {
            "prototype".into()
        } else {
            "spec".into()
        },
    });
    Ok(Collected {
        project,
        leads,
        warnings: p.warnings(),
    })
}

/// Write `out/project.json` and `out/private/leads.json`.
pub fn write_outputs(out_dir: &Path, c: &Collected) -> Result<()> {
    let private = out_dir.join("private");
    std::fs::create_dir_all(&private)?;
    let write = |path: &Path, json: String| -> Result<()> {
        std::fs::write(path, json).with_context(|| format!("writing {}", path.display()))
    };
    write(&out_dir.join("project.json"), serde_json::to_string(&c.project)?)?;
    write(&private.join("leads.json"), serde_json::to_string_pretty(&c.leads)?)?;
    Ok(())
}
