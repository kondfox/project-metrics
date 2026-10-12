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
use pm_llm::{Facts, LayerFile, Provider, QuestionSet, Response};
use pm_metrics::model::{VelocityDetail, VelocityHealth};
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
/// The question set packages are scored with: `[velocity] rubric`, else the default.
pub fn rubric(ws: &Workspace) -> Result<QuestionSet> {
    let id = ws.config.velocity.as_ref().and_then(|v| v.rubric.as_deref());
    QuestionSet::bundled(id.unwrap_or(pm_llm::rubric::DEFAULT_RUBRIC)).map_err(|e| anyhow!("[velocity] rubric: {e}"))
}

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
    /// Ticket id, `<repo>#<n>` / `<repo>!<n>` for a PR, or `<person>~<ISO week>`.
    pub id: String,
    pub ticket: Option<String>,
    /// The PR of a PR package.
    pub pr: Option<String>,
    /// The person of an author-week package.
    pub author: Option<String>,
    /// Oldest first.
    commits: Vec<RepoCommit>,
}

impl WorkPackage {
    /// Other ids the same author-week package can have: `<git author name>~<week>` for each raw
    /// author name of its commits, as tools that group by git name (the prototype) write them.
    /// Empty for a ticket package.
    pub fn aliases(&self) -> std::collections::BTreeSet<String> {
        let Some(week) = self.author.as_ref().and(self.id.rsplit_once('~')).map(|(_, w)| w) else {
            return Default::default();
        };
        self.commits
            .iter()
            .map(|c| format!("{}~{week}", c.commit.author_name))
            .filter(|a| *a != self.id)
            .collect()
    }

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

/// Which merged PR each commit belongs to, per repo (spec §4 rule 2).
#[derive(Default)]
pub struct PrIndex {
    repos: BTreeMap<String, RepoPrIndex>,
    /// GitHub squash subjects end in `(#n)`: they name their PR even without code-host data.
    github: bool,
    /// Repos with a code host but no PR data: grouped by ticket and author-week only.
    pub missing: Vec<String>,
}

#[derive(Default)]
struct RepoPrIndex {
    /// Commit or landed (merge, squash) SHA → PR id.
    by_sha: BTreeMap<String, String>,
    /// Landed SHA → PR id: a merge commit with one of these brought that PR's commits in.
    landed: BTreeMap<String, String>,
    /// (author email, subject) → (authored date, PR id) of PR commits: rebased PRs land with new
    /// SHAs but keep these.
    by_commit: BTreeMap<(String, String), Vec<(NaiveDate, String)>>,
}

/// `See merge request <path>!<n>` in a GitLab merge commit's message.
fn gitlab_mr(message: &str) -> Option<(String, u64)> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"See merge request (\S+)!(\d+)").expect("valid regex"));
    let m = re.captures(message)?;
    Some((m[1].to_string(), m[2].parse().ok()?))
}

/// `Merge pull request #<n> from …` in a GitHub merge commit's subject.
fn github_pr(message: &str) -> Option<u64> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^Merge pull request #(\d+) from ").expect("valid regex"));
    re.captures(message.lines().next()?)?[1].parse().ok()
}

/// Is the merge's source a long-lived branch (`develop` into `main`, `main` into a feature, …)?
/// Such a merge carries whatever was pushed there, not one change.
fn from_long_lived_branch(message: &str) -> bool {
    static SOURCE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static LONG: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let source = SOURCE.get_or_init(|| {
        Regex::new(r"^Merge (?:remote-tracking )?branch '([^']+)'|^Merge pull request #\d+ from [^/\s]+/(\S+)")
            .expect("valid regex")
    });
    let long = LONG.get_or_init(|| {
        Regex::new(r"^(?:origin/)?(?:develop|dev|main|master|staging|production|release.*)$").expect("valid regex")
    });
    let subject = message.lines().next().unwrap_or("");
    source
        .captures(subject)
        .and_then(|c| c.get(1).or(c.get(2)))
        .is_some_and(|b| long.is_match(b.as_str()))
}

fn squash_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\(#(\d+)\)\s*$").expect("valid regex"))
}

impl PrIndex {
    pub fn new(ws: &Workspace, repos: &[crate::pulls::RepoPrs], missing: Vec<String>) -> PrIndex {
        let mut out = PrIndex {
            github: ws
                .config
                .code_host
                .as_ref()
                .is_some_and(|h| h.kind == pm_config::CodeHostKind::Github),
            missing,
            ..Default::default()
        };
        for r in repos {
            let idx = out.repos.entry(r.repo.clone()).or_default();
            for pr in &r.prs {
                for sha in &pr.landed {
                    idx.by_sha.insert(sha.clone(), pr.id.clone());
                    idx.landed.insert(sha.clone(), pr.id.clone());
                }
                for c in pr.commits.iter().flatten() {
                    idx.by_sha.entry(c.sha.clone()).or_insert_with(|| pr.id.clone());
                    if let (Some(email), Some(at)) = (&c.author_email, c.authored_at) {
                        idx.by_commit
                            .entry((email.trim().to_lowercase(), c.subject.trim().to_string()))
                            .or_default()
                            .push((at.date_naive(), pr.id.clone()));
                    }
                }
            }
        }
        out
    }

    /// Fill in what the host's commit lists miss from git (spec §4 rule 2.2): a merge commit whose
    /// SHA is a PR's landed SHA, or whose message names a PR, owns the commits it brought in. Oldest
    /// merge first, so a commit goes to the earliest; merges from long-lived branches are skipped.
    pub fn add_merges(&mut self, repos: &[VelocityRepo], since: NaiveDate) -> Result<()> {
        for r in repos {
            let git = Git::open(&r.dir);
            let range = LogRange::new(&r.tip).committed_since(since);
            let merges = git
                .merges(&range)
                .with_context(|| format!("repo `{}`: git log --merges", r.name))?;
            let idx = self.repos.entry(r.name.clone()).or_default();
            // The project's own path, as its merge messages name it (a moved repo's older merges
            // name the old one).
            let own_path = merges
                .iter()
                .filter(|m| idx.landed.contains_key(&m.sha))
                .find_map(|m| gitlab_mr(&m.message).map(|(path, _)| path));
            for m in &merges {
                if from_long_lived_branch(&m.message) {
                    continue;
                }
                let id = match idx.landed.get(&m.sha) {
                    Some(id) => id.clone(),
                    None => match (gitlab_mr(&m.message), github_pr(&m.message)) {
                        (Some((path, n)), _) if own_path.as_deref().is_none_or(|p| p == path) => format!("!{n}"),
                        (Some((path, n)), _) => format!(":{path}!{n}"),
                        (None, Some(n)) => format!("#{n}"),
                        (None, None) => continue,
                    },
                };
                for sha in git
                    .brought_in(m)
                    .with_context(|| format!("repo `{}`: commits of merge {}", r.name, m.sha))?
                {
                    idx.by_sha.entry(sha).or_insert_with(|| id.clone());
                }
            }
        }
        Ok(())
    }

    /// The PR package id (`<repo>#<n>`, `<repo>!<n>`) of a commit, if it belongs to a merged PR.
    fn pr_of(&self, repo: &str, c: &Commit) -> Option<String> {
        let subject = c.message.lines().next().unwrap_or("").trim();
        let found = self.repos.get(repo).and_then(|idx| {
            idx.by_sha.get(&c.sha).cloned().or_else(|| {
                let key = (c.author_email.trim().to_lowercase(), subject.to_string());
                idx.by_commit.get(&key).and_then(|v| {
                    v.iter()
                        .find(|(d, _)| (*d - c.author_date).num_days().abs() <= 1)
                        .map(|(_, id)| id.clone())
                })
            })
        });
        let found = found.or_else(|| {
            self.github
                .then(|| squash_re().captures(subject).map(|m| format!("#{}", &m[1])))
                .flatten()
        });
        found.map(|id| format!("{repo}{id}"))
    }
}

/// The first matching rule decides a commit's package (spec §4): its ticket; else the ticket of its
/// PR when the PR's ticketed commits name exactly one; else its PR; else author-week.
fn group(commits: Vec<RepoCommit>, repos: &[VelocityRepo], prs: &PrIndex) -> Vec<WorkPackage> {
    let keyed: Vec<(Option<String>, Option<String>)> = commits
        .iter()
        .map(|c| (ticket_of(&c.commit.message), prs.pr_of(&repos[c.repo].name, &c.commit)))
        .collect();
    let mut pr_tickets: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (ticket, pr) in &keyed {
        if let (Some(t), Some(pr)) = (ticket, pr) {
            pr_tickets.entry(pr.as_str()).or_default().insert(t.as_str());
        }
    }
    let mut by_id: BTreeMap<String, WorkPackage> = BTreeMap::new();
    for (c, (ticket, pr)) in commits.into_iter().zip(&keyed) {
        let ticket = ticket.clone().or_else(|| {
            let ts = pr_tickets.get(pr.as_deref()?)?;
            (ts.len() == 1)
                .then(|| ts.iter().next().map(|t| t.to_string()))
                .flatten()
        });
        let (id, pr, author) = match (&ticket, pr) {
            (Some(t), _) => (t.clone(), None, None),
            (None, Some(pr)) => (pr.clone(), Some(pr.clone()), None),
            (None, None) => (
                format!("{}~{}", c.person, iso_week(c.commit.author_date)),
                None,
                Some(c.person.clone()),
            ),
        };
        by_id
            .entry(id.clone())
            .or_insert_with(|| WorkPackage {
                id,
                ticket: ticket.clone(),
                pr,
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

/// Excluded from the diff on top of generated files (spec §4.2). Migrations stay in (D39): they are
/// often hand-written and are the evidence for R2.
pub fn excluded_from_diff(classifier: &Classifier, path: &str) -> bool {
    let p = format!("/{path}");
    classifier.is_generated(path) || p.contains("/Pods/") || p.contains("/__generated__/") || path.ends_with(".min.js")
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
    /// The changed files, for the per-file layer questions (`six-axis@3`).
    pub files: Vec<LayerFile>,
    /// Files beyond the layer-question cap (unknown layer).
    pub unknown_files: u32,
}

fn prepare(
    p: &WorkPackage,
    repos: &[VelocityRepo],
    gits: &[Git],
    cls: &Classifier,
    set: &QuestionSet,
) -> Result<Prepared> {
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
    // Names stay out of the state: the id goes only for a ticket or a PR.
    let mut state = json!({
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
    // Only PR packages carry the key, so other packages' states (and cached answers) are unchanged.
    if let Some(pr) = &p.pr {
        state["pr"] = json!(pr);
    }
    let first = subjects.first().copied().unwrap_or("");
    let layer_files: Vec<LayerFile> = files
        .iter()
        .map(|((repo, path), f)| LayerFile {
            repo: repo.clone(),
            path: path.clone(),
            asked: matches!(f.class, FileClass::Prod | FileClass::Other),
            lines: f.added + f.deleted,
        })
        .collect();
    let (asked, unknown_files) = set.layer_files(&layer_files);
    let asked = asked.len() as u32;
    Ok(Prepared {
        state: serde_json::to_string_pretty(&state)?,
        facts: Facts {
            files: files.len() as u32,
            modules: modules.len() as u32,
            layer_files: asked,
        },
        files: layer_files,
        unknown_files,
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

/// Read, group, prepare and classify. `Ok(None)` = not measured (no classifier configured).
/// The configured classifiers, built once per run. One that cannot be built (a missing API key)
/// leaves its packages unscored instead of failing the run.
pub struct Classifiers<'a> {
    config: &'a pm_config::Config,
    providers: BTreeMap<String, std::result::Result<Box<dyn Provider>, String>>,
}

impl<'a> Classifiers<'a> {
    /// `names` and their fallbacks.
    pub fn build<'n>(
        config: &'a pm_config::Config,
        names: impl IntoIterator<Item = &'n str>,
        template: pm_llm::Template,
    ) -> Classifiers<'a> {
        let mut providers = BTreeMap::new();
        let mut todo: Vec<String> = names.into_iter().map(String::from).collect();
        while let Some(name) = todo.pop() {
            if providers.contains_key(&name) {
                continue;
            }
            let Some(c) = config.classifiers.get(&name) else {
                providers.insert(name.clone(), Err(format!("no [classifiers.{name}]")));
                continue;
            };
            if let Some(f) = &c.fallback {
                todo.push(f.clone());
            }
            providers.insert(name, pm_llm::build(c, template).map_err(|e| e.to_string()));
        }
        Classifiers { config, providers }
    }

    pub fn get(&self, name: &str) -> std::result::Result<&dyn Provider, String> {
        match self.providers.get(name) {
            Some(Ok(p)) => Ok(p.as_ref()),
            Some(Err(e)) => Err(e.clone()),
            None => Err(format!("classifier `{name}` not built")),
        }
    }

    /// The fallback `name` may escalate to for a package: none when the data policy forbids it.
    fn fallback(&self, name: &str, local_only: bool) -> Option<&dyn Provider> {
        let f = self.config.classifiers.get(name)?.fallback.as_deref()?;
        self.get(f).ok().filter(|p| !local_only || p.local())
    }

    /// Everything that decides an answer besides the package: the cache key's classifier part.
    pub fn key(&self, name: &str, local_only: bool) -> std::result::Result<String, String> {
        let primary = self.get(name)?.identity();
        let gate = self.config.classifiers.get(name).and_then(|c| c.confidence_gate);
        let fallback = self.fallback(name, local_only).map(|f| f.identity());
        Ok(match (gate, fallback) {
            (None, None) => primary,
            (g, f) => format!(
                "{primary}|gate={}|fallback={}",
                g.map_or("-".into(), |g| g.to_string()),
                f.unwrap_or_else(|| "-".into())
            ),
        })
    }

    /// Classify one package: the confidence gate, one retry for a bad answer or a timeout, and an
    /// answer that does not score counts as a failure.
    pub fn ask(&self, name: &str, local_only: bool, set: &QuestionSet, prepared: &Prepared) -> Result<pm_llm::Gated> {
        let primary = self.get(name).map_err(|e| anyhow!(e))?;
        let gate = self.config.classifiers.get(name).and_then(|c| c.confidence_gate);
        let fallback = self.fallback(name, local_only);
        let req = set.request(prepared.state.clone(), &prepared.files);
        let once = || pm_llm::classify_gated(primary, fallback, gate, set.reask(), &set.context, &req);
        let mut g = once().or_else(|e| {
            if matches!(e, pm_llm::Error::Answer(_) | pm_llm::Error::Timeout(_)) {
                std::thread::sleep(Duration::from_secs(2));
                once()
            } else {
                Err(e)
            }
        })?;
        // Only what was asked is kept (a `command` may answer more).
        g.response.answers.retain(|id, _| req.questions.get(id).is_some());
        set.axes(&g.response.answers, &prepared.facts)?;
        Ok(g)
    }
}

/// A cached answer. Entries from before the gate (a bare response) still read.
pub fn cached_answer(cache: &Cache, key: &str, classifier: &str) -> Result<Option<pm_llm::Gated>> {
    let Some(v) = cache.answer::<serde_json::Value>(key)? else {
        return Ok(None);
    };
    if let Ok(g) = serde_json::from_value::<pm_llm::Gated>(v.clone()) {
        return Ok(Some(g));
    }
    Ok(serde_json::from_value::<Response>(v)
        .ok()
        .map(|response| pm_llm::Gated {
            response,
            classifier: classifier.to_string(),
            reasked: false,
            escalated: false,
            low_confidence: false,
        }))
}

/// Commits since `since` → work packages, with each repo's git handle.
pub fn read_packages(
    s: &VelocitySettings,
    repos: &[VelocityRepo],
    prs: &PrIndex,
    since: NaiveDate,
) -> Result<(Vec<WorkPackage>, Vec<Git>)> {
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
    Ok((group(commits, repos, prs), gits))
}

/// Prepare one package for its classifier (`None`: no classifier for its repos).
pub fn prepare_package(
    wp: &WorkPackage,
    repos: &[VelocityRepo],
    gits: &[Git],
    cls: &Classifier,
    set: &QuestionSet,
) -> Result<(Prepared, Option<String>, bool)> {
    let prepared = prepare(wp, repos, gits, cls, set)?;
    let local_only = wp.commits.iter().any(|c| repos[c.repo].local_only);
    Ok((prepared, pick_classifier(wp, repos), local_only))
}

fn row(wp: &WorkPackage, prepared: &Prepared) -> Package {
    let mut authors: BTreeMap<String, u32> = BTreeMap::new();
    for c in &wp.commits {
        *authors.entry(c.person.clone()).or_default() += 1;
    }
    Package {
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
        escalated: false,
        low_confidence: false,
        unknown_files: prepared.unknown_files,
        author: wp.author.clone(),
        authors,
    }
}

struct Job {
    index: usize,
    classifier: String,
    local_only: bool,
    key: String,
    prepared: Prepared,
}

/// Read, group, prepare and classify. `Ok(None)` = not measured (no classifier configured).
pub fn run_stage(s: &VelocitySettings, repos: &[VelocityRepo], mut prs: PrIndex) -> Result<Option<VelocityDetail>> {
    let p = &s.progress;
    if !configured(s.ws) {
        return Ok(None);
    }
    let config = &s.ws.config;
    let set = rubric(s.ws)?;
    let since = since(s.ws);
    p.start(VELOCITY);
    p.detail(VELOCITY, "reading commits".to_string());
    let classifiers = Classifiers::build(
        config,
        repos.iter().filter_map(|r| r.classifier.as_deref()),
        set.template,
    );
    for (name, built) in &classifiers.providers {
        if let Err(e) = built {
            p.warn(format!(
                "classifier `{name}` cannot run ({e}): its packages are not scored"
            ));
        }
    }

    if !prs.missing.is_empty() {
        p.warn(format!(
            "no PR data for {}: their commits without a ticket id are grouped by author-week{}",
            prs.missing.join(", "),
            if prs.github {
                " (or by a `(#n)` squash subject)"
            } else {
                ""
            }
        ));
    }
    p.detail(VELOCITY, "reading merges".to_string());
    prs.add_merges(repos, since)?;
    let prs = &prs;
    let (packages, gits) = read_packages(s, repos, prs, since)?;
    p.plan(VELOCITY, packages.len() as u64, 0, false);
    let cache = if s.use_cache {
        Some(Cache::open(&s.ws.state_dir().join("cache.sqlite"))?)
    } else {
        None
    };
    let mut rows: Vec<Package> = Vec::with_capacity(packages.len());
    let mut jobs = Vec::new();
    let mut refused = 0usize;
    let mut cached = 0u64;
    for (index, wp) in packages.iter().enumerate() {
        let (prepared, name, local_only) = prepare_package(wp, repos, &gits, s.classifier, &set)?;
        rows.push(row(wp, &prepared));
        let Some(name) = name else {
            p.advance(VELOCITY, 1);
            continue;
        };
        let Ok(provider) = classifiers.get(&name) else {
            p.advance(VELOCITY, 1);
            continue;
        };
        if local_only && !provider.local() {
            refused += 1;
            p.advance(VELOCITY, 1);
            continue;
        }
        let identity = classifiers.key(&name, local_only).map_err(|e| anyhow!(e))?;
        let key = answer_key(&config.project.name, &wp.id, &prepared.state, &set.id, &identity);
        let hit = match &cache {
            Some(c) => cached_answer(c, &key, &provider.identity())?,
            None => None,
        };
        match hit {
            Some(g) => {
                score_row(&mut rows[index], &set, &g, &prepared.facts);
                cached += 1;
                p.advance(VELOCITY, 1);
            }
            None => jobs.push(Job {
                index,
                classifier: name,
                local_only,
                key,
                prepared,
            }),
        }
    }
    p.plan(VELOCITY, jobs.len() as u64, cached, false);

    // Classify the misses, each classifier on its own pool.
    let started = Instant::now();
    let errors: Mutex<BTreeMap<String, (usize, String)>> = Mutex::new(BTreeMap::new());
    let results: Mutex<Vec<(usize, pm_llm::Gated)>> = Mutex::new(Vec::new());
    let by_classifier: BTreeMap<&str, Vec<&Job>> = jobs.iter().fold(BTreeMap::new(), |mut m, j| {
        m.entry(j.classifier.as_str()).or_default().push(j);
        m
    });
    let cache_path = s.ws.state_dir().join("cache.sqlite");
    std::thread::scope(|scope| {
        for (name, list) in &by_classifier {
            let workers = config.classifiers[*name].jobs.unwrap_or(DEFAULT_JOBS).clamp(1, 32);
            let next = AtomicUsize::new(0);
            let failures_in_a_row = AtomicUsize::new(0);
            let (errors, results, cache_path, set, classifiers) = (&errors, &results, &cache_path, &set, &classifiers);
            let use_cache = s.use_cache;
            scope.spawn(move || {
                std::thread::scope(|inner| {
                    for _ in 0..workers.min(list.len()) {
                        inner.spawn(|| {
                            let cache = if use_cache { Cache::open(cache_path).ok() } else { None };
                            loop {
                                let i = next.fetch_add(1, Ordering::SeqCst);
                                let Some(job) = list.get(i) else { break };
                                if failures_in_a_row.load(Ordering::SeqCst) >= MAX_FAILURES_IN_A_ROW {
                                    p.advance(VELOCITY, 1);
                                    continue;
                                }
                                p.detail(VELOCITY, format!("{name}: package {}/{}", i + 1, list.len()));
                                match classifiers.ask(name, job.local_only, set, &job.prepared) {
                                    Ok(g) => {
                                        failures_in_a_row.store(0, Ordering::SeqCst);
                                        if let Some(c) = &cache {
                                            if let Err(e) = c.put_answer(&job.key, &g) {
                                                p.warn(format!("cannot cache an answer: {e:#}"));
                                            }
                                        }
                                        results.lock().unwrap_or_else(|e| e.into_inner()).push((job.index, g));
                                    }
                                    Err(e) => {
                                        failures_in_a_row.fetch_add(1, Ordering::SeqCst);
                                        let mut errs = errors.lock().unwrap_or_else(|e| e.into_inner());
                                        let entry = errs.entry(name.to_string()).or_insert((0, format!("{e:#}")));
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
    for (index, g) in results.into_inner().unwrap_or_else(|e| e.into_inner()) {
        score_row(&mut rows[index], &set, &g, &by_index[&index].prepared.facts);
    }
    p.finish(VELOCITY);

    for (name, (n, first)) in errors.into_inner().unwrap_or_else(|e| e.into_inner()) {
        p.warn(format!("classifier `{name}`: {n} package(s) failed ({first})"));
    }
    if refused > 0 {
        p.warn(format!(
            "{refused} package(s) touch a local-only repo but their classifier is not local: not scored"
        ));
    }
    let mut health = health(&rows);
    for wp in &packages {
        match (&wp.ticket, &wp.pr) {
            (Some(_), _) => health.by_ticket += 1,
            (None, Some(_)) => health.by_pr += 1,
            (None, None) => health.by_author_week += 1,
        }
    }
    health.no_pr_data = prs.missing.clone();
    if health.unscored > 0 {
        p.warn(format!(
            "{} of {} work packages not scored: Velocity is empty for the weeks and months that hold them (run collect again to retry)",
            health.unscored, health.packages
        ));
    }
    let classifiers_used: BTreeSet<String> = rows.iter().filter_map(|r| r.classifier.clone()).collect();
    if classifiers_used.len() > 1 {
        p.warn(format!(
            "Velocity mixes classifiers ({}): their scores are not strictly comparable",
            classifiers_used.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    let models: BTreeSet<String> = rows.iter().filter_map(|r| r.model.clone()).collect();
    if models.len() > classifiers_used.len().max(1) {
        p.warn(format!(
            "work packages were answered by {} models ({}): a declined request may have run on a fallback model",
            models.len(),
            models.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    rows.sort_by(|a, b| b.date.cmp(&a.date).then(a.id.cmp(&b.id)));
    Ok(Some(VelocityDetail {
        rubric: set.id.clone(),
        classifiers: classifiers_used.into_iter().collect(),
        models: models.into_iter().collect(),
        health,
        options: VelocityOptions {
            since,
            as_of: s.as_of,
            fte: fte_periods(s.ws)?,
        },
        packages: rows,
    }))
}

fn score_row(row: &mut Package, set: &QuestionSet, g: &pm_llm::Gated, facts: &Facts) {
    if let Ok(axes) = set.axes(&g.response.answers, facts) {
        row.axes = Some(axes.0);
        row.points = Some(axes.points());
        row.size = Some(axes.size().0.to_string());
        row.classifier = Some(g.classifier.clone());
        row.model = Some(g.response.model.clone());
        row.escalated = g.escalated;
        row.low_confidence = g.low_confidence;
    }
}

fn health(rows: &[Package]) -> VelocityHealth {
    VelocityHealth {
        packages: rows.len() as u32,
        unscored: rows.iter().filter(|r| r.points.is_none()).count() as u32,
        escalated: rows.iter().filter(|r| r.escalated).count() as u32,
        low_confidence: rows.iter().filter(|r| r.low_confidence).count() as u32,
        ..Default::default()
    }
}

/// Classify one fictional package, to check that a classifier works (`pmx classifier test`).
pub fn smoke_test(ws: &Workspace, name: &str) -> Result<(pm_llm::Axes, Response, Duration)> {
    let c = ws
        .config
        .classifiers
        .get(name)
        .ok_or_else(|| anyhow!("no [classifiers.{name}] in pmx.toml"))?;
    let set = rubric(ws)?;
    let provider = pm_llm::build(c, set.template)?;
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
    let files = [
        LayerFile::new("shop-api", "src/customers/customer.ts", true, 2),
        LayerFile::new("shop-api", "src/customers/customer.dto.ts", true, 1),
    ];
    let req = set.request(serde_json::to_string_pretty(&state)?, &files);
    let r = provider.classify(&set.context, &req)?;
    let facts = Facts {
        files: 2,
        modules: 1,
        layer_files: set.layer_files(&files).0.len() as u32,
    };
    let axes = set.axes(&r.answers, &facts)?;
    Ok((axes, r, started.elapsed()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rc(sha: &str, who: &str, date: &str, subject: &str) -> RepoCommit {
        RepoCommit {
            repo: 0,
            person: who.into(),
            commit: Commit {
                sha: sha.into(),
                author_email: format!("{}@work.example", who.to_lowercase()),
                author_name: who.into(),
                author_date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
                message: subject.into(),
                files: Vec::new(),
            },
        }
    }

    fn repo() -> VelocityRepo {
        VelocityRepo {
            name: "api".into(),
            dir: PathBuf::new(),
            tip: String::new(),
            role: RepoRole::Fixed(pm_classify::Role::Backend),
            classifier: None,
            local_only: false,
        }
    }

    fn index(github: bool, prs: &[(&str, &[&str])], rebased: &[(&str, &str, &str, &str)]) -> PrIndex {
        let mut idx = RepoPrIndex::default();
        for (id, shas) in prs {
            for sha in *shas {
                idx.by_sha.insert(sha.to_string(), id.to_string());
            }
        }
        for (id, email, subject, date) in rebased {
            idx.by_commit
                .entry((email.to_string(), subject.to_string()))
                .or_default()
                .push((NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(), id.to_string()));
        }
        PrIndex {
            repos: [("api".to_string(), idx)].into(),
            github,
            missing: Vec::new(),
        }
    }

    fn ids(ps: &[WorkPackage]) -> Vec<(String, Vec<String>)> {
        ps.iter()
            .map(|p| {
                let mut shas: Vec<String> = p.commits.iter().map(|c| c.commit.sha.clone()).collect();
                shas.sort();
                (p.id.clone(), shas)
            })
            .collect()
    }

    #[test]
    fn first_matching_rule_groups_each_commit() {
        let prs = index(
            false,
            &[
                ("#1", &["a1", "a2"]),
                ("#2", &["b1", "b2", "b3"]),
                ("#3", &["c1"]),
                ("#4", &["s4"]),
            ],
            &[("#5", "jane@work.example", "tidy errors", "2026-03-04")],
        );
        let commits = vec![
            // #1: one ticket, so its untagged commit joins SHOP-1.
            rc("a1", "Jane", "2026-03-02", "SHOP-1 add form"),
            rc("a2", "Jane", "2026-03-02", "review fixes"),
            // #2 names two tickets: its untagged commit stays the PR's.
            rc("b1", "Bob", "2026-03-03", "SHOP-2 api"),
            rc("b2", "Bob", "2026-03-03", "SHOP-3 ui"),
            rc("b3", "Bob", "2026-03-03", "fix lint"),
            // #3 has no ticket at all.
            rc("c1", "Bob", "2026-03-03", "bump timeout"),
            // Squashed: the landed commit is the PR's.
            rc("s4", "Jane", "2026-03-04", "new export"),
            // Rebased: a new SHA, same author, subject and (within a day) date.
            rc("r5", "Jane", "2026-03-05", "tidy errors"),
            // Nothing matches: author-week.
            rc("x1", "Jane", "2026-03-05", "notes"),
        ];
        let got = ids(&group(commits, &[repo()], &prs));
        let want: Vec<(String, Vec<String>)> = [
            ("Jane~2026-W10", vec!["x1"]),
            ("SHOP-1", vec!["a1", "a2"]),
            ("SHOP-2", vec!["b1"]),
            ("SHOP-3", vec!["b2"]),
            ("api#2", vec!["b3"]),
            ("api#3", vec!["c1"]),
            ("api#4", vec!["s4"]),
            ("api#5", vec!["r5"]),
        ]
        .into_iter()
        .map(|(id, shas)| (id.to_string(), shas.into_iter().map(String::from).collect()))
        .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn merge_messages() {
        assert_eq!(
            gitlab_mr("Merge branch 'f' into 'main'\n\nCloses #4\n\nSee merge request acme/shop!12"),
            Some(("acme/shop".into(), 12))
        );
        assert_eq!(
            github_pr("Merge pull request #87 from acme/feature-x\n\nAdd export"),
            Some(87)
        );
        assert_eq!(github_pr("Revert \"Merge pull request #87 from acme/x\""), None);
        assert!(from_long_lived_branch("Merge branch 'develop' into 'main'"));
        assert!(from_long_lived_branch("Merge branch 'release/2.1' into 'main'"));
        assert!(from_long_lived_branch("Merge pull request #9 from acme/develop"));
        assert!(from_long_lived_branch(
            "Merge remote-tracking branch 'origin/main' into feature"
        ));
        assert!(!from_long_lived_branch("Merge branch 'feature/devops' into 'develop'"));
        assert!(!from_long_lived_branch("Merge pull request #9 from acme/maintenance"));
    }

    /// A scripted repo: `sh` runs git with fixed identities and dates.
    fn scripted(dir: &std::path::Path, steps: &[(&str, &[&str])]) {
        for (date, args) in steps {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(["-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
                .args(*args)
                .env("GIT_AUTHOR_NAME", "Jane")
                .env("GIT_AUTHOR_EMAIL", "jane@work.example")
                .env("GIT_COMMITTER_NAME", "Jane")
                .env("GIT_COMMITTER_EMAIL", "jane@work.example")
                .env("GIT_AUTHOR_DATE", format!("{date}T12:00:00Z"))
                .env("GIT_COMMITTER_DATE", format!("{date}T12:00:00Z"))
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    #[test]
    fn merges_fill_in_what_the_host_did_not_list() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let c = |name: &'static str| -> [&'static str; 4] { ["commit", "-q", "--allow-empty", name] };
        let m = |msg: &'static str, branch: &'static str| -> Vec<&'static str> {
            vec!["merge", "-q", "--no-ff", branch, "-m", msg]
        };
        let steps: Vec<(&str, Vec<&str>)> = vec![
            ("2026-03-01", vec!["init", "-q"]),
            ("2026-03-01", c("-minit").to_vec()),
            ("2026-03-01", vec!["checkout", "-q", "-b", "develop"]),
            // An MR whose host commit list came back empty: its merge SHA is known.
            ("2026-03-02", vec!["checkout", "-q", "-b", "f1"]),
            ("2026-03-02", c("-mlisted nowhere").to_vec()),
            ("2026-03-02", c("-mlisted by the host").to_vec()),
            ("2026-03-03", vec!["checkout", "-q", "develop"]),
            (
                "2026-03-03",
                m(
                    "Merge branch 'f1' into 'develop'\n\nSee merge request acme/shop!5",
                    "f1",
                ),
            ),
            // An older MR of the project the repo moved from: named only in its message.
            ("2026-03-04", vec!["checkout", "-q", "-b", "f2"]),
            ("2026-03-04", c("-mold project work").to_vec()),
            ("2026-03-05", vec!["checkout", "-q", "develop"]),
            (
                "2026-03-05",
                m(
                    "Merge branch 'f2' into 'develop'\n\nSee merge request legacy/shop!40",
                    "f2",
                ),
            ),
            // A direct push to develop, then a release merge: never the release's package.
            ("2026-03-06", c("-mdirect push").to_vec()),
            ("2026-03-07", vec!["checkout", "-q", "main"]),
            (
                "2026-03-07",
                m(
                    "Merge branch 'develop' into 'main'\n\nSee merge request acme/shop!6",
                    "develop",
                ),
            ),
        ];
        for (date, args) in &steps {
            scripted(dir, &[(date, args.as_slice())]);
        }
        let git = Git::open(dir);
        let sha_of = |subject: &str| {
            git.run(&["log", "--all", "--format=%H", &format!("--grep=^{subject}$")])
                .unwrap()
                .trim()
                .to_string()
        };
        let merge5 = git
            .run(&["log", "--all", "--merges", "--format=%H", "--grep=acme/shop!5"])
            .unwrap();
        let mut prs = PrIndex::default();
        let idx = prs.repos.entry("api".into()).or_default();
        idx.landed.insert(merge5.trim().to_string(), "!5".into());
        // The host listed this one under another MR: a host list wins over git.
        idx.by_sha.insert(sha_of("listed by the host"), "!99".into());
        let mut r = repo();
        r.dir = dir.to_path_buf();
        r.tip = "main".into();
        prs.add_merges(&[r], NaiveDate::from_ymd_opt(2026, 1, 1).unwrap())
            .unwrap();
        let idx = &prs.repos["api"];
        assert_eq!(
            idx.by_sha.get(&sha_of("listed nowhere")).map(String::as_str),
            Some("!5")
        );
        assert_eq!(
            idx.by_sha.get(&sha_of("old project work")).map(String::as_str),
            Some(":legacy/shop!40")
        );
        assert_eq!(
            idx.by_sha.get(&sha_of("listed by the host")).map(String::as_str),
            Some("!99")
        );
        // Only the skipped release merge brought it into main.
        assert_eq!(idx.by_sha.get(&sha_of("direct push")), None);
        assert_eq!(idx.by_sha.get(&sha_of("init")), None, "before any merge");
    }

    #[test]
    fn rebased_commits_match_within_a_day_only() {
        let prs = index(false, &[], &[("#7", "jane@work.example", "tidy", "2026-03-04")]);
        let near = group(vec![rc("n", "Jane", "2026-03-05", "tidy")], &[repo()], &prs);
        assert_eq!(near[0].id, "api#7");
        let far = group(vec![rc("f", "Jane", "2026-03-07", "tidy")], &[repo()], &prs);
        assert_eq!(far[0].id, "Jane~2026-W10");
    }

    #[test]
    fn github_squash_subjects_name_their_pr_without_data() {
        let c = || vec![rc("q", "Jane", "2026-03-05", "Add CSV export (#87)")];
        let gh = PrIndex {
            github: true,
            ..Default::default()
        };
        let p = group(c(), &[repo()], &gh);
        assert_eq!((p[0].id.as_str(), p[0].pr.as_deref()), ("api#87", Some("api#87")));
        // GitLab subjects carry no such marker: author-week.
        assert_eq!(group(c(), &[repo()], &PrIndex::default())[0].id, "Jane~2026-W10");
    }

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
        assert!(!excluded_from_diff(&c, "db/migrations/0001_init.sql"));
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
