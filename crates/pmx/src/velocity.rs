//! The velocity stage (spec §4, plan §6.4): commits → work packages → filtered diffs → a classifier
//! answers the six-axis questions → R1–R6 and points, computed in code.
//!
//! Answers are cached by package content, rubric and classifier, so a run only sends new or
//! changed packages, and `--full` never pays for a package twice.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use chrono::{Datelike, NaiveDate};
use pm_classify::{Classifier, FileClass};
use pm_config::{DataPolicy, FteSource, Identity, RepoRole, Workspace, expand_path};
use pm_git::{Commit, Git, LogRange};
use pm_llm::{Facts, Provider, QuestionSet, Response};
use pm_metrics::model::VelocityDetail;
use pm_metrics::velocity::{FtePeriod, Package, VelocityOptions};
use pm_progress::Progress;
use regex::Regex;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::cache::Cache;

pub const VELOCITY: &str = "velocity";
/// The diff sent per package is cut here (spec §4: ≈ 8k tokens).
pub const DIFF_CAP: usize = 24_000;
/// At most this many files are listed in the state.
const FILES_CAP: usize = 200;
/// At most this many commit subjects are listed in the state.
const SUBJECTS_CAP: usize = 50;
/// After this many failures in a row, a classifier gets no more packages in this run (an expired
/// login or a used-up quota fails every call the same way).
const MAX_FAILURES_IN_A_ROW: usize = 3;
/// Packages classified at the same time unless the classifier sets `jobs`.
const DEFAULT_JOBS: usize = 4;

/// One repo to read, as planned.
pub struct VelocityRepo {
    pub name: String,
    pub dir: PathBuf,
    pub tip: String,
    pub role: RepoRole,
    pub classifier: Option<String>,
    pub local_only: bool,
}

pub struct VelocitySettings<'a> {
    pub ws: &'a Workspace,
    pub as_of: NaiveDate,
    pub classifier: &'a Classifier,
    pub identity: &'a Identity,
    pub use_cache: bool,
    pub progress: Progress,
}

/// Is any repo set up for Velocity?
pub fn configured(ws: &Workspace) -> bool {
    ws.config.repos.iter().any(|r| ws.config.classifier_for(r).is_some())
}

pub fn repos(ws: &Workspace, plans: &[crate::collect::RepoPlan]) -> Vec<VelocityRepo> {
    ws.config
        .repos
        .iter()
        .zip(plans)
        .filter_map(|(r, p)| {
            Some(VelocityRepo {
                name: p.name.clone(),
                dir: p.dir.clone(),
                tip: p.tip.clone()?,
                role: r.role,
                classifier: ws.config.classifier_for(r).map(String::from),
                local_only: r.data_policy == Some(DataPolicy::LocalOnly),
            })
        })
        .collect()
}

/// The first date scored: `[velocity] since`, but never before `range_start`.
pub fn since(ws: &Workspace) -> NaiveDate {
    let c = &ws.config;
    let start = c.project.range_start;
    c.velocity
        .as_ref()
        .and_then(|v| v.since)
        .map_or(start, |s| s.max(start))
}

/// `[fte]` as dated periods (spec §10.2); empty when unknown.
pub fn fte_periods(ws: &Workspace) -> Result<Vec<FtePeriod>> {
    let Some(f) = &ws.config.fte else {
        return Ok(Vec::new());
    };
    let period = |p: &pm_config::FtePeriod| FtePeriod {
        from: Some(p.from),
        to: p.to,
        fte: p.fte,
    };
    match f.source {
        FteSource::Static if !f.periods.is_empty() => Ok(f.periods.iter().map(period).collect()),
        FteSource::Static => Ok(f
            .value
            .map(|fte| FtePeriod {
                from: None,
                to: None,
                fte,
            })
            .into_iter()
            .collect()),
        FteSource::File => {
            let path = expand_path(f.file.as_deref().unwrap_or(""), &ws.root);
            let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
            let v: serde_json::Value = serde_json::from_str(&text).with_context(|| format!("{}", path.display()))?;
            fte_from_json(&v, &ws.config.project.name).with_context(|| format!("{}", path.display()))
        }
    }
}

/// `{ "<project>": 4.5 }` or `{ "<project>": [{"from", "to"?, "fte"}, …] }`.
pub fn fte_from_json(v: &serde_json::Value, project: &str) -> Result<Vec<FtePeriod>> {
    let date = |x: &serde_json::Value| -> Result<Option<NaiveDate>> {
        match x.as_str() {
            None => Ok(None),
            Some(s) => Ok(Some(NaiveDate::parse_from_str(s, "%Y-%m-%d")?)),
        }
    };
    match &v[project] {
        serde_json::Value::Null => Ok(Vec::new()),
        serde_json::Value::Number(n) => Ok(vec![FtePeriod {
            from: None,
            to: None,
            fte: n.as_f64().unwrap_or(0.0),
        }]),
        serde_json::Value::Array(a) => a
            .iter()
            .map(|p| {
                Ok(FtePeriod {
                    from: date(&p["from"])?,
                    to: date(&p["to"])?,
                    fte: p["fte"].as_f64().ok_or_else(|| anyhow!("a period without `fte`"))?,
                })
            })
            .collect(),
        other => Err(anyhow!(
            "`{project}`: expected a number or a list of periods, got {other}"
        )),
    }
}

/// A commit with the repo it came from.
#[derive(Clone, Debug)]
struct RepoCommit {
    repo: usize,
    person: String,
    commit: Commit,
}

/// Commits grouped into one work package (spec §4.1).
#[derive(Clone, Debug)]
pub struct WorkPackage {
    /// Ticket id, or `<person>~<ISO week>`.
    pub id: String,
    pub ticket: Option<String>,
    /// The person of an author-week package.
    pub author: Option<String>,
    /// Oldest first.
    commits: Vec<RepoCommit>,
}

impl WorkPackage {
    pub fn date(&self) -> NaiveDate {
        self.commits
            .iter()
            .map(|c| c.commit.author_date)
            .max()
            .expect("not empty")
    }
}

fn ticket_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b[A-Z][A-Z0-9]{1,9}-\d+\b").expect("valid regex"))
}

/// The first ticket id in the subject (spec §4.1).
pub fn ticket_of(message: &str) -> Option<String> {
    let subject = message.lines().next().unwrap_or("");
    ticket_re().find(subject).map(|m| m.as_str().to_string())
}

pub fn iso_week(d: NaiveDate) -> String {
    let w = d.iso_week();
    format!("{}-W{:02}", w.year(), w.week())
}

fn group(commits: Vec<RepoCommit>) -> Vec<WorkPackage> {
    let mut by_id: BTreeMap<String, WorkPackage> = BTreeMap::new();
    for c in commits {
        let ticket = ticket_of(&c.commit.message);
        let id = match &ticket {
            Some(t) => t.clone(),
            None => format!("{}~{}", c.person, iso_week(c.commit.author_date)),
        };
        let author = ticket.is_none().then(|| c.person.clone());
        by_id
            .entry(id.clone())
            .or_insert_with(|| WorkPackage {
                id,
                ticket,
                author,
                commits: Vec::new(),
            })
            .commits
            .push(c);
    }
    let mut out: Vec<WorkPackage> = by_id.into_values().collect();
    for p in &mut out {
        // git log lists newest first; a stable sort keeps that order within a day, so reverse
        // first.
        p.commits.reverse();
        p.commits.sort_by_key(|c| c.commit.author_date);
    }
    out
}

/// Excluded from the diff on top of generated files (spec §4.2).
pub fn excluded_from_diff(classifier: &Classifier, path: &str) -> bool {
    let p = format!("/{path}");
    classifier.is_generated(path)
        || p.contains("/migrations/")
        || p.contains("/Pods/")
        || p.contains("/__generated__/")
        || path.ends_with(".min.js")
}

/// Split a patch into `(path, section)` per file.
pub fn split_patch(patch: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in patch.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let path = rest
                .trim_end()
                .rsplit_once(" b/")
                .map(|(_, b)| b.to_string())
                .unwrap_or_else(|| rest.trim_end().to_string());
            out.push((path, String::new()));
        }
        if let Some((_, s)) = out.last_mut() {
            s.push_str(line);
        }
    }
    // `-w` leaves whitespace-only changes as a header without hunks: drop those.
    out.retain(|(_, s)| {
        s.lines()
            .any(|l| (l.starts_with('+') && !l.starts_with("+++")) || (l.starts_with('-') && !l.starts_with("---")))
    });
    out
}

/// The module of a file: its repo and first two directories (R5, plan §6.4).
pub fn module_of(repo: &str, path: &str) -> String {
    let dirs: Vec<&str> = path.split('/').collect();
    let dirs = &dirs[..dirs.len().saturating_sub(1)];
    format!("{repo}:{}", dirs.iter().take(2).copied().collect::<Vec<_>>().join("/"))
}

/// What is sent for one package, and what the roll-up needs from it.
pub struct Prepared {
    pub state: String,
    pub facts: Facts,
    pub added: u64,
    pub tests: bool,
    pub truncated: bool,
    pub repos: Vec<String>,
    pub description: String,
}

fn prepare(p: &WorkPackage, repos: &[VelocityRepo], gits: &[Git], cls: &Classifier) -> Result<Prepared> {
    struct FileRow {
        class: FileClass,
        role: String,
        added: u64,
        deleted: u64,
    }
    let mut files: BTreeMap<(String, String), FileRow> = BTreeMap::new();
    let mut excluded = BTreeSet::new();
    let (mut added, mut deleted, mut source_added) = (0u64, 0u64, 0u64);
    let mut tests = false;
    let mut diff = String::new();
    let mut truncated = false;
    for c in &p.commits {
        let repo = &repos[c.repo];
        for f in &c.commit.files {
            if excluded_from_diff(cls, &f.path) {
                excluded.insert((repo.name.clone(), f.path.clone()));
                continue;
            }
            let class = cls.classify(&f.path);
            tests |= cls.touches_test(&f.path);
            if matches!(class, FileClass::Prod | FileClass::Test) {
                source_added += f.added;
            }
            added += f.added;
            deleted += f.deleted;
            let role = match repo.role {
                RepoRole::Fixed(r) => r.to_string(),
                RepoRole::PerFile => cls.role_of_file(&f.path).to_string(),
            };
            let row = files.entry((repo.name.clone(), f.path.clone())).or_insert(FileRow {
                class,
                role,
                added: 0,
                deleted: 0,
            });
            row.added += f.added;
            row.deleted += f.deleted;
        }
        if truncated {
            continue;
        }
        let patch = gits[c.repo]
            .patch(&c.commit.sha)
            .with_context(|| format!("repo `{}`: git show {}", repo.name, c.commit.sha))?;
        let subject = c.commit.message.lines().next().unwrap_or("");
        let mut chunk = format!(
            "### {} {} {subject}\n",
            repo.name,
            &c.commit.sha[..c.commit.sha.len().min(10)]
        );
        for (path, section) in split_patch(&patch) {
            if !excluded_from_diff(cls, &path) {
                chunk.push_str(&section);
            }
        }
        if diff.len() + chunk.len() > DIFF_CAP {
            let mut cut = DIFF_CAP.saturating_sub(diff.len());
            while !chunk.is_char_boundary(cut) {
                cut -= 1;
            }
            diff.push_str(&chunk[..cut]);
            truncated = true;
        } else {
            diff.push_str(&chunk);
        }
    }
    let modules: BTreeSet<String> = files.keys().map(|(r, p)| module_of(r, p)).collect();
    let repo_names: Vec<String> = p
        .commits
        .iter()
        .map(|c| repos[c.repo].name.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let subjects: Vec<&str> = p
        .commits
        .iter()
        .map(|c| c.commit.message.lines().next().unwrap_or(""))
        .collect();
    let file_rows: Vec<serde_json::Value> = files
        .iter()
        .take(FILES_CAP)
        .map(|((repo, path), f)| {
            json!({"repo": repo, "path": path, "class": format!("{:?}", f.class).to_lowercase(),
                   "role": f.role, "added": f.added, "deleted": f.deleted})
        })
        .collect();
    // Names stay out of the state: the id goes only for a ticket.
    let state = json!({
        "ticket": p.ticket,
        "repos": repo_names,
        "n_commits": p.commits.len(),
        "subjects": subjects.iter().take(SUBJECTS_CAP).collect::<Vec<_>>(),
        "added": added,
        "deleted": deleted,
        "files": file_rows,
        "files_not_listed": files.len().saturating_sub(FILES_CAP),
        "generated_files_excluded": excluded.len(),
        "truncated": truncated,
        "diff": diff,
    });
    let first = subjects.first().copied().unwrap_or("");
    Ok(Prepared {
        state: serde_json::to_string_pretty(&state)?,
        facts: Facts {
            files: files.len() as u32,
            modules: modules.len() as u32,
        },
        added: source_added,
        tests,
        truncated,
        repos: repo_names,
        description: match &p.ticket {
            Some(t) if !first.contains(t.as_str()) => format!("{t} {first}"),
            _ => first.to_string(),
        },
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The answer cache key: project, package id, content, rubric and classifier (spec §10.3).
pub fn answer_key(project: &str, id: &str, state: &str, rubric: &str, classifier: &str) -> String {
    let v = json!({
        "project": project,
        "id": id,
        "state": hex(&Sha256::digest(state.as_bytes())),
        "rubric": rubric,
        "classifier": classifier,
    });
    hex(&Sha256::digest(v.to_string().as_bytes()))
}

/// The classifier a package goes to: a `local-only` repo's own if the package has one (its code
/// must not leave the machine), else the first repo's. `None` = no classifier for these repos.
fn pick_classifier(p: &WorkPackage, repos: &[VelocityRepo]) -> Option<String> {
    let rs: Vec<&VelocityRepo> = p.commits.iter().map(|c| &repos[c.repo]).collect();
    rs.iter()
        .find(|r| r.local_only)
        .or(rs.first())
        .and_then(|r| r.classifier.clone())
}

struct Job {
    index: usize,
    classifier: String,
    key: String,
    prepared: Prepared,
}

/// Read, group, prepare and classify. `Ok(None)` = not measured (no classifier configured).
pub fn run_stage(s: &VelocitySettings, repos: &[VelocityRepo]) -> Result<Option<VelocityDetail>> {
    let p = &s.progress;
    if !configured(s.ws) {
        return Ok(None);
    }
    let config = &s.ws.config;
    let set = QuestionSet::six_axis();
    let since = since(s.ws);
    p.start(VELOCITY);
    p.detail(VELOCITY, "reading commits".to_string());

    // Providers, once each.
    let mut providers: BTreeMap<String, Box<dyn Provider>> = BTreeMap::new();
    for name in repos.iter().filter_map(|r| r.classifier.as_ref()) {
        if !providers.contains_key(name) {
            let c = &config.classifiers[name];
            providers.insert(
                name.clone(),
                pm_llm::build(c).map_err(|e| anyhow!("[classifiers.{name}]: {e}"))?,
            );
        }
    }

    // Commits → packages.
    let gits: Vec<Git> = repos.iter().map(|r| Git::open(&r.dir)).collect();
    let mut commits = Vec::new();
    for (i, r) in repos.iter().enumerate() {
        let range = LogRange::new(&r.tip).committed_since(since);
        for c in gits[i]
            .commits(&range, &mut |_| {})
            .with_context(|| format!("repo `{}`: git log", r.name))?
        {
            if c.author_date < since || c.author_date > s.as_of {
                continue;
            }
            let a = s.identity.resolve(&c.author_email, &c.author_name);
            if a.excluded.is_some() {
                continue;
            }
            commits.push(RepoCommit {
                repo: i,
                person: a.person,
                commit: c,
            });
        }
    }
    let packages = group(commits);
    p.plan(VELOCITY, packages.len() as u64, 0, false);

    let cache = if s.use_cache {
        Some(Cache::open(&s.ws.state_dir().join("cache.sqlite"))?)
    } else {
        None
    };
    let mut rows: Vec<Package> = Vec::with_capacity(packages.len());
    let mut jobs = Vec::new();
    let mut used = BTreeSet::new();
    let mut refused = 0usize;
    let mut cached = 0u64;
    for (index, wp) in packages.iter().enumerate() {
        let prepared = prepare(wp, repos, &gits, s.classifier)?;
        rows.push(Package {
            id: wp.id.clone(),
            date: wp.date(),
            repos: prepared.repos.clone(),
            commits: wp.commits.len() as u32,
            added: prepared.added,
            description: prepared.description.clone(),
            axes: None,
            points: None,
            size: None,
            tests: prepared.tests,
            truncated: prepared.truncated,
            classifier: None,
            model: None,
            author: wp.author.clone(),
        });
        let Some(name) = pick_classifier(wp, repos) else {
            p.advance(VELOCITY, 1);
            continue;
        };
        let provider = &providers[&name];
        let local_only = wp.commits.iter().any(|c| repos[c.repo].local_only);
        if local_only && !provider.local() {
            refused += 1;
            p.advance(VELOCITY, 1);
            continue;
        }
        let key = answer_key(
            &config.project.name,
            &wp.id,
            &prepared.state,
            &set.id,
            &provider.identity(),
        );
        let hit = match &cache {
            Some(c) => c.answer::<Response>(&key)?,
            None => None,
        };
        match hit {
            Some(resp) => {
                score_row(&mut rows[index], &set, &resp, &prepared.facts, &provider.identity());
                used.insert(provider.identity());
                cached += 1;
                p.advance(VELOCITY, 1);
            }
            None => jobs.push(Job {
                index,
                classifier: name,
                key,
                prepared,
            }),
        }
    }
    p.plan(VELOCITY, jobs.len() as u64, cached, false);

    // Classify the misses, each classifier on its own pool.
    let started = Instant::now();
    let errors: Mutex<BTreeMap<String, (usize, String)>> = Mutex::new(BTreeMap::new());
    let results: Mutex<Vec<(usize, Response)>> = Mutex::new(Vec::new());
    let by_classifier: BTreeMap<&str, Vec<&Job>> = jobs.iter().fold(BTreeMap::new(), |mut m, j| {
        m.entry(j.classifier.as_str()).or_default().push(j);
        m
    });
    std::thread::scope(|scope| {
        for (name, list) in &by_classifier {
            let provider = &providers[*name];
            let workers = config.classifiers[*name].jobs.unwrap_or(DEFAULT_JOBS).clamp(1, 32);
            let next = AtomicUsize::new(0);
            let failures_in_a_row = AtomicUsize::new(0);
            let (errors, results, cache_path, set) = (&errors, &results, s.ws.state_dir().join("cache.sqlite"), &set);
            let use_cache = s.use_cache;
            scope.spawn(move || {
                std::thread::scope(|inner| {
                    for _ in 0..workers.min(list.len()) {
                        inner.spawn(|| {
                            let cache = if use_cache { Cache::open(&cache_path).ok() } else { None };
                            loop {
                                let i = next.fetch_add(1, Ordering::SeqCst);
                                let Some(job) = list.get(i) else { break };
                                if failures_in_a_row.load(Ordering::SeqCst) >= MAX_FAILURES_IN_A_ROW {
                                    p.advance(VELOCITY, 1);
                                    continue;
                                }
                                p.detail(VELOCITY, format!("{name}: package {}/{}", i + 1, list.len()));
                                let req = set.request(job.prepared.state.clone());
                                let answer = provider.classify(&set.context, &req).or_else(|e| {
                                    if matches!(e, pm_llm::Error::Answer(_) | pm_llm::Error::Timeout(_)) {
                                        std::thread::sleep(Duration::from_secs(2));
                                        provider.classify(&set.context, &req)
                                    } else {
                                        Err(e)
                                    }
                                });
                                // An answer that does not score is a failure too.
                                let answer = answer.and_then(|r| set.axes(&r.answers, &job.prepared.facts).map(|_| r));
                                match answer {
                                    Ok(r) => {
                                        failures_in_a_row.store(0, Ordering::SeqCst);
                                        if let Some(c) = &cache {
                                            if let Err(e) = c.put_answer(&job.key, &r) {
                                                p.warn(format!("cannot cache an answer: {e:#}"));
                                            }
                                        }
                                        results.lock().unwrap_or_else(|e| e.into_inner()).push((job.index, r));
                                    }
                                    Err(e) => {
                                        failures_in_a_row.fetch_add(1, Ordering::SeqCst);
                                        let mut errs = errors.lock().unwrap_or_else(|e| e.into_inner());
                                        let entry = errs.entry(name.to_string()).or_insert((0, e.to_string()));
                                        entry.0 += 1;
                                    }
                                }
                                p.advance(VELOCITY, 1);
                            }
                        });
                    }
                });
            });
        }
    });
    if let Some(c) = &cache {
        c.record_timing(VELOCITY, jobs.len() as u64, started.elapsed().as_secs_f64())?;
    }
    let by_index: BTreeMap<usize, &Job> = jobs.iter().map(|j| (j.index, j)).collect();
    for (index, r) in results.into_inner().unwrap_or_else(|e| e.into_inner()) {
        let job = by_index[&index];
        let identity = providers[&job.classifier].identity();
        score_row(&mut rows[index], &set, &r, &job.prepared.facts, &identity);
        used.insert(identity);
    }
    p.finish(VELOCITY);

    for (name, (n, first)) in errors.into_inner().unwrap_or_else(|e| e.into_inner()) {
        p.warn(format!("classifier `{name}`: {n} package(s) failed ({first})"));
    }
    let unscored = rows.iter().filter(|r| r.points.is_none()).count();
    if refused > 0 {
        p.warn(format!(
            "{refused} package(s) touch a local-only repo but their classifier is not local: not scored"
        ));
    }
    if unscored > 0 {
        p.warn(format!(
            "{unscored} of {} work packages not scored: Velocity is empty for the weeks and months that hold them (run collect again to retry)",
            rows.len()
        ));
    }
    if used.len() > 1 {
        p.warn(format!(
            "Velocity mixes classifiers ({}): their scores are not strictly comparable",
            used.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    rows.sort_by(|a, b| b.date.cmp(&a.date).then(a.id.cmp(&b.id)));
    Ok(Some(VelocityDetail {
        rubric: set.id.clone(),
        classifiers: used.into_iter().collect(),
        options: VelocityOptions {
            since,
            as_of: s.as_of,
            fte: fte_periods(s.ws)?,
        },
        packages: rows,
    }))
}

fn score_row(row: &mut Package, set: &QuestionSet, r: &Response, facts: &Facts, classifier: &str) {
    if let Ok(axes) = set.axes(&r.answers, facts) {
        row.axes = Some(axes.0);
        row.points = Some(axes.points());
        row.size = Some(axes.size().0.to_string());
        row.classifier = Some(classifier.to_string());
        row.model = Some(r.model.clone());
    }
}

/// Classify one fictional package, to check that a classifier works (`pmx classifier test`).
pub fn smoke_test(ws: &Workspace, name: &str) -> Result<(pm_llm::Axes, Response, Duration)> {
    let c = ws
        .config
        .classifiers
        .get(name)
        .ok_or_else(|| anyhow!("no [classifiers.{name}] in pmx.toml"))?;
    let provider = pm_llm::build(c)?;
    let set = QuestionSet::six_axis();
    let state = json!({
        "ticket": "SHOP-1",
        "repos": ["shop-api"],
        "n_commits": 1,
        "subjects": ["SHOP-1 Add an optional nickname to customers"],
        "files": [
            {"repo": "shop-api", "path": "src/customers/customer.ts", "class": "prod", "role": "backend", "added": 2, "deleted": 0},
            {"repo": "shop-api", "path": "src/customers/customer.dto.ts", "class": "prod", "role": "backend", "added": 1, "deleted": 0}
        ],
        "truncated": false,
        "diff": "### shop-api 0000000000 SHOP-1 Add an optional nickname to customers\n--- a/src/customers/customer.ts\n+++ b/src/customers/customer.ts\n@@ -4,4 +4,6 @@ export class Customer {\n   @Column() name: string;\n+  @Column({ nullable: true })\n+  nickname?: string;\n }\n--- a/src/customers/customer.dto.ts\n+++ b/src/customers/customer.dto.ts\n@@ -2,3 +2,4 @@ export interface CustomerDto {\n   name: string;\n+  nickname?: string;\n }\n",
    });
    let started = Instant::now();
    let r = provider.classify(&set.context, &set.request(serde_json::to_string_pretty(&state)?))?;
    let axes = set.axes(&r.answers, &Facts { files: 2, modules: 1 })?;
    Ok((axes, r, started.elapsed()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tickets_and_weeks() {
        assert_eq!(ticket_of("SHOP-12 add nickname").as_deref(), Some("SHOP-12"));
        assert_eq!(ticket_of("fix(api): AB-7, then X-1").as_deref(), Some("AB-7"));
        // The spec's pattern (§4.1) also takes `UTF-8`: a known false positive, kept for parity.
        assert_eq!(ticket_of("bump UTF-8 handling").as_deref(), Some("UTF-8"));
        assert_eq!(ticket_of("bump utf-8 handling"), None);
        assert_eq!(ticket_of("no ticket\n\nRefs SHOP-3"), None, "subject only");
        assert_eq!(iso_week(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()), "2026-W01");
        assert_eq!(iso_week(NaiveDate::from_ymd_opt(2027, 1, 1).unwrap()), "2026-W53");
    }

    #[test]
    fn splits_patches_and_drops_whitespace_only_files() {
        let patch = "diff --git a/src/a.ts b/src/a.ts\nindex 1..2 100644\n--- a/src/a.ts\n+++ b/src/a.ts\n@@ -1 +1,2 @@\n x\n+y\ndiff --git a/b.ts b/b.ts\nindex 3..4 100644\ndiff --git a/my dir/c d.ts b/my dir/c d.ts\n--- a/my dir/c d.ts\n+++ b/my dir/c d.ts\n@@ -1 +0,0 @@\n-gone\n";
        let s = split_patch(patch);
        assert_eq!(
            s.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
            ["src/a.ts", "my dir/c d.ts"]
        );
        assert!(s[0].1.starts_with("diff --git") && s[0].1.ends_with("+y\n"));
    }

    #[test]
    fn modules_and_exclusions() {
        assert_eq!(module_of("api", "src/orders/service/x.ts"), "api:src/orders");
        assert_eq!(module_of("api", "src/x.ts"), "api:src");
        assert_eq!(module_of("api", "README.md"), "api:");
        let c = Classifier::default();
        assert!(excluded_from_diff(&c, "db/migrations/0001_init.sql"));
        assert!(excluded_from_diff(&c, "ios/Pods/x.m"));
        assert!(excluded_from_diff(&c, "web/vendor.min.js"));
        assert!(excluded_from_diff(&c, "package-lock.json"));
        assert!(!excluded_from_diff(&c, "src/app.ts"));
    }

    #[test]
    fn fte_file_shapes() {
        let v = json!({"Acme Shop": 3.5, "Other": [{"from": "2026-08-01", "to": "2026-08-31", "fte": 4.5}, {"from": "2026-09-01", "fte": 4}]});
        assert_eq!(fte_from_json(&v, "Acme Shop").unwrap()[0].fte, 3.5);
        let o = fte_from_json(&v, "Other").unwrap();
        assert_eq!(o.len(), 2);
        assert_eq!(o[1].to, None);
        assert!(fte_from_json(&v, "Missing").unwrap().is_empty());
    }
}
