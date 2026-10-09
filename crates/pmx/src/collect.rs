//! `pmx collect`: fetch → git ingest → rework walk → roll-up (plan §4, stages 2–4 and 8).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use chrono::{Days as ChronoDays, NaiveDate};
use pm_classify::{Classifier, TestRules};
use pm_config::{Identity, RepoConfig, Workspace};
use pm_git::{Dialect, Git};
use pm_metrics::model::{BuildInput, ProjectInfo, RepoInfo};
use pm_metrics::{Days, LeadsFile, ProjectFile, merge_days};
use sha2::{Digest, Sha256};

use crate::cache::Cache;
use crate::ingest::{IngestContext, RepoIngest, ingest};

/// Lines deleted within this many days of being added are rework (spec §3.1).
pub const REWORK_YOUNG_DAYS: i64 = 21;

#[derive(Clone, Debug)]
pub struct CollectOptions {
    /// Replaces the run date everywhere (spec §1.1).
    pub as_of: NaiveDate,
    pub fetch: bool,
    pub use_cache: bool,
    /// Read git and detect tests the way the private prototype did. For the parity harness only
    /// (parity.md); never set by the CLI.
    pub prototype_compat: bool,
    pub log: bool,
}

impl CollectOptions {
    pub fn new(as_of: NaiveDate) -> Self {
        CollectOptions {
            as_of,
            fetch: true,
            use_cache: true,
            prototype_compat: false,
            log: false,
        }
    }
}

pub struct Collected {
    pub project: ProjectFile,
    pub leads: LeadsFile,
    pub warnings: Vec<String>,
}

struct RepoResult {
    info: RepoInfo,
    ingest: RepoIngest,
    warnings: Vec<String>,
}

/// Everything that changes a repo's ingest result besides its tip SHA.
fn settings_key(ws: &Workspace, repo: &RepoConfig, prototype_compat: bool) -> String {
    let c = &ws.config;
    let parts = serde_json::json!({
        "pmx": env!("CARGO_PKG_VERSION"),
        "role": repo.role.to_string(),
        "role_rules": c.role_rules,
        "people": c.people,
        "range_start": c.project.range_start,
        "prototype_compat": prototype_compat,
    });
    let digest = Sha256::digest(parts.to_string().as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn open_repo(ws: &Workspace, repo: &RepoConfig, fetch: bool, warnings: &mut Vec<String>) -> Result<Git> {
    let dir = ws.repo_dir(repo);
    let name = repo.display_name();
    if let Some(url) = &repo.url {
        if !dir.exists() {
            return Git::clone_bare(url, &dir).with_context(|| format!("cloning `{name}`"));
        }
    } else if !dir.exists() {
        return Err(anyhow!("repo `{name}`: {} does not exist", dir.display()));
    }
    let git = Git::open(&dir);
    if !git.is_repo() {
        return Err(anyhow!("repo `{name}`: {} is not a git repository", dir.display()));
    }
    if fetch && repo.rev.is_none() {
        if let Err(e) = git.fetch() {
            warnings.push(format!("repo `{name}`: fetch failed, measuring local refs ({e})"));
        }
    }
    Ok(git)
}

fn collect_repo(
    ws: &Workspace,
    repo: &RepoConfig,
    opts: &CollectOptions,
    classifier: &Classifier,
    identity: &Identity,
) -> Result<RepoResult> {
    let name = repo.display_name();
    let mut warnings = Vec::new();
    let dialect = if opts.prototype_compat {
        Dialect::Prototype
    } else {
        Dialect::Spec
    };
    let git = open_repo(ws, repo, opts.fetch, &mut warnings)?.with_dialect(dialect);
    let rev = repo
        .revision()
        .map(String::from)
        .unwrap_or_else(|| git.default_revision());
    let sha = git
        .rev_parse(&rev)
        .with_context(|| format!("repo `{name}`: cannot resolve `{rev}`"))?;
    let tip_date = git.commit_date(&sha)?;
    let settings = settings_key(ws, repo, opts.prototype_compat);

    let cache = if opts.use_cache {
        Some(Cache::open(&ws.state_dir().join("cache.sqlite"))?)
    } else {
        None
    };
    let cached = match &cache {
        Some(c) => c.get(&name, &sha, &settings)?,
        None => None,
    };
    let ingest = match cached {
        Some(i) => {
            if opts.log {
                eprintln!("  {name}: cached at {}", &sha[..12]);
            }
            i
        }
        None => {
            let range_start = ws.config.project.range_start;
            let commits = git.commits(&sha).with_context(|| format!("repo `{name}`: git log"))?;
            let since = range_start - ChronoDays::new(REWORK_YOUNG_DAYS as u64);
            let is_source = |p: &str| classifier.is_source(p);
            let rework = git
                .rework(&sha, since, REWORK_YOUNG_DAYS, &is_source)
                .with_context(|| format!("repo `{name}`: rework walk"))?;
            let cx = IngestContext {
                role: repo.role,
                classifier,
                identity,
                range_start,
            };
            let ingest = ingest(&commits, &rework, &cx);
            if opts.log {
                eprintln!("  {name}: {} commits at {}", commits.len(), &sha[..12]);
            }
            if let Some(c) = &cache {
                c.put(&name, &sha, &settings, &ingest)?;
            }
            ingest
        }
    };
    Ok(RepoResult {
        info: RepoInfo {
            name,
            role: repo.role.to_string(),
            sha,
            tip_date,
        },
        ingest,
        warnings,
    })
}

pub fn collect(ws: &Workspace, opts: &CollectOptions) -> Result<Collected> {
    let config = &ws.config;
    let tests = if opts.prototype_compat {
        TestRules::Prototype
    } else {
        TestRules::Spec
    };
    let classifier = config.classifier(tests);
    let identity = Identity::new(&config.people);

    let results: Vec<Result<RepoResult>> = std::thread::scope(|s| {
        let handles: Vec<_> = config
            .repos
            .iter()
            .map(|r| s.spawn(|| collect_repo(ws, r, opts, &classifier, &identity)))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("repo worker panicked"))
            .collect()
    });

    let range_start = config.project.range_start;
    let mut days = Days::new();
    let mut dates = BTreeSet::new();
    let mut repos = Vec::new();
    let mut warnings = Vec::new();
    for r in results {
        let r = r?;
        let window: Days = r
            .ingest
            .days
            .range(range_start..=opts.as_of)
            .map(|(d, day)| (*d, day.clone()))
            .collect();
        merge_days(&mut days, &window);
        dates.extend(r.ingest.commit_dates.range(..=opts.as_of).copied());
        repos.push(r.info);
        warnings.extend(r.warnings);
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
        warnings,
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
