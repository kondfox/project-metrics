//! The pull-request stage (plan §4 step 6): every merged PR/MR of the repos with a
//! `code_host_id`, fetched incrementally and stored per PR in the cache, then resolved to people and
//! stacks for the PR-flow metrics (spec §8).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Result, anyhow};
use chrono::{DateTime, Duration, FixedOffset, NaiveDate, NaiveTime, Utc};
use pm_classify::{Classifier, Role};
use pm_codehost::{CodeHost, GitHub, GitLab, Http, MergedPr, Query, Selection, Sink, UreqHttp};
use pm_config::{CodeHostId, CodeHostKind, Exclusion, Identity, RepoConfig, RepoRole, Workspace};
use pm_metrics::model::PullsInfo;
use pm_metrics::pr::PrInput;
use pm_progress::Progress;
use sha2::{Digest, Sha256};

use crate::cache::Cache;

pub const PULLS: &str = "pulls";
/// Bump when the stored PR shape or what is fetched changes.
const FORMAT: u32 = 1;
/// An incremental fetch starts this long before the previous one, so clock skew and PRs updated
/// during that run are not missed.
const OVERLAP_HOURS: i64 = 24;

/// Where PRs come from.
#[derive(Clone, Default)]
pub enum PrSource {
    /// The configured code host over HTTPS.
    #[default]
    Live,
    /// The configured code host through another transport (tests, the parity replay).
    Http(Arc<dyn Http>),
    /// These PRs per repo name, as if just fetched (`pmx demo`, the parity harness).
    Fixed(BTreeMap<String, Vec<MergedPr>>),
}

pub struct PullSettings<'a> {
    pub ws: &'a Workspace,
    pub as_of: NaiveDate,
    pub source: &'a PrSource,
    /// `CreatedSince` replays the prototype's GitLab request (parity harness).
    pub selection: Selection,
    pub use_cache: bool,
    /// Continue from the last fetch (default); `false` fetches everything again.
    pub incremental: bool,
    pub progress: Progress,
}

/// One repo's PRs.
pub struct RepoPrs {
    pub repo: String,
    pub role: RepoRole,
    pub prs: Vec<MergedPr>,
}

pub struct Pulls {
    pub info: PullsInfo,
    pub repos: Vec<RepoPrs>,
}

/// The repos to read and their code-host project ids.
pub fn code_host_repos(ws: &Workspace) -> Vec<(&RepoConfig, String)> {
    ws.config
        .repos
        .iter()
        .filter_map(|r| {
            let id = match r.code_host_id.as_ref()? {
                CodeHostId::Number(n) => n.to_string(),
                CodeHostId::Name(s) => s.clone(),
            };
            Some((r, id))
        })
        .collect()
}

pub fn token_env(ws: &Workspace) -> Option<String> {
    let h = ws.config.code_host.as_ref()?;
    Some(h.token_env.clone().unwrap_or_else(|| {
        match h.kind {
            CodeHostKind::Github => "GITHUB_TOKEN",
            CodeHostKind::Gitlab => "GITLAB_TOKEN",
        }
        .into()
    }))
}

/// Cache scope of one repo: everything that changes what a fetch returns.
pub fn scope(ws: &Workspace, repo: &RepoConfig, project_id: &str, selection: Selection) -> String {
    let h = ws.config.code_host.as_ref();
    let v = serde_json::json!({
        "format": FORMAT,
        "kind": h.map(|h| h.kind),
        "base": h.and_then(|h| h.base.clone()),
        "project": project_id,
        "range_start": ws.config.project.range_start,
        "created": selection == Selection::CreatedSince,
        "files": repo.role == RepoRole::PerFile,
    });
    Sha256::digest(v.to_string().as_bytes())
        .iter()
        .take(12)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Stores each fetched PR at once, so an interrupted fetch keeps what it got.
struct StoreSink<'a> {
    cache: Option<Cache>,
    scope: String,
    known: HashMap<String, Option<String>>,
    fetched: Vec<MergedPr>,
    progress: &'a Progress,
    listed: &'a std::sync::atomic::AtomicU64,
    error: Option<anyhow::Error>,
}

impl Sink for StoreSink<'_> {
    fn listed(&mut self, n: u64) {
        use std::sync::atomic::Ordering;
        let total = self.listed.fetch_add(n, Ordering::SeqCst) + n;
        self.progress.plan(PULLS, total, 0, true);
    }

    fn is_current(&self, id: &str, updated_at: Option<&DateTime<FixedOffset>>) -> bool {
        match (self.known.get(id), updated_at) {
            (Some(Some(stored)), Some(u)) => DateTime::parse_from_rfc3339(stored).is_ok_and(|s| s == *u),
            _ => false,
        }
    }

    fn pr(&mut self, pr: MergedPr) {
        if let Some(c) = &self.cache {
            let updated = pr.updated_at.map(|u| u.to_rfc3339());
            if let Err(e) = c.put_pr(&self.scope, &pr.id, updated.as_deref(), &pr) {
                self.error.get_or_insert(e);
            }
        }
        self.fetched.push(pr);
        self.progress.advance(PULLS, 1);
    }

    fn skipped(&mut self) {
        self.progress.advance(PULLS, 1);
    }
}

fn host(ws: &Workspace, source: &PrSource, token: String) -> Box<dyn CodeHost> {
    let h = ws.config.code_host.as_ref().expect("checked by the caller");
    let http: Arc<dyn Http> = match source {
        PrSource::Http(h) => h.clone(),
        _ => Arc::new(UreqHttp::default()),
    };
    match h.kind {
        CodeHostKind::Github => Box::new(GitHub::new(h.base.as_deref(), token, http)),
        CodeHostKind::Gitlab => Box::new(GitLab::new(
            h.base.as_deref().unwrap_or("https://gitlab.com"),
            token,
            http,
        )),
    }
}

/// Fetch (or read from the cache) every repo's merged PRs. `Ok(None)` = not measured, with the
/// reason as a warning.
pub fn run_stage(s: &PullSettings) -> Result<Option<Pulls>> {
    let p = &s.progress;
    let Some(code_host) = &s.ws.config.code_host else {
        return Ok(None);
    };
    let repos = code_host_repos(s.ws);
    if repos.is_empty() {
        p.warn("no repo has a `code_host_id`: peer review not measured (`pmx repo set <name> --code-host-id …`)");
        p.skip(PULLS, "no code_host_id");
        return Ok(None);
    }
    let env = token_env(s.ws).expect("code host is set");
    let env_token = std::env::var(&env).ok().filter(|t| !t.trim().is_empty());
    let token = match s.source {
        PrSource::Live => env_token,
        // Fakes and replays need no token.
        PrSource::Http(_) | PrSource::Fixed(_) => Some(env_token.unwrap_or_default()),
    };
    let host = token.clone().map(|t| host(s.ws, s.source, t));
    let range_start =
        DateTime::<Utc>::from_naive_utc_and_offset(s.ws.config.project.range_start.and_time(NaiveTime::MIN), Utc);
    // A run as of a past date skips PRs merged later, so it must not count as a sync either.
    let until = (s.as_of < Utc::now().date_naive()).then_some(s.as_of);
    let listed = std::sync::atomic::AtomicU64::new(0);
    p.start(PULLS);
    let started = Instant::now();
    let mut out = Vec::new();
    let mut fetched_total = 0u64;
    let mut missing = Vec::new();
    // Fetch errors → repos, one warning each (a missing token fails every repo the same way).
    let mut stale: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut failed: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (repo, project_id) in &repos {
        let name = repo.display_name();
        p.detail(PULLS, name.clone());
        let scope = scope(s.ws, repo, project_id, s.selection);
        let cache = if s.use_cache {
            Some(Cache::open(&s.ws.state_dir().join("cache.sqlite"))?)
        } else {
            None
        };
        let synced = match &cache {
            Some(c) if s.incremental && s.selection == Selection::UpdatedSince => c.pr_synced(&scope)?,
            _ => None,
        };
        let since = synced
            .as_deref()
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&Utc) - Duration::hours(OVERLAP_HOURS))
            .map_or(range_start, |t| t.max(range_start));
        let known = match &cache {
            Some(c) if s.incremental => c.pr_versions(&scope)?,
            _ => HashMap::new(),
        };
        let had_data = !known.is_empty();
        let mut sink = StoreSink {
            cache,
            scope: scope.clone(),
            known,
            fetched: Vec::new(),
            progress: p,
            listed: &listed,
            error: None,
        };
        let fetch_started = Utc::now();
        let outcome: Result<()> = match (s.source, &host) {
            (PrSource::Fixed(all), _) => {
                let prs = all.get(&name).cloned().unwrap_or_default();
                sink.listed(prs.len() as u64);
                prs.into_iter().for_each(|pr| sink.pr(pr));
                Ok(())
            }
            (_, None) => Err(anyhow!("{env} is not set")),
            (_, Some(h)) => {
                let q = Query {
                    project: project_id,
                    since,
                    selection: s.selection,
                    until,
                    files: repo.role == RepoRole::PerFile,
                };
                h.merged_prs(&q, &mut sink).map_err(anyhow::Error::from)
            }
        };
        if let Some(e) = sink.error.take() {
            return Err(e);
        }
        fetched_total += sink.fetched.len() as u64;
        let prs = match (outcome, &sink.cache) {
            (Ok(()), Some(c)) => {
                if until.is_none() {
                    c.set_pr_synced(
                        &scope,
                        &fetch_started.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                    )?;
                }
                c.prs::<MergedPr>(&scope)?
            }
            (Ok(()), None) => std::mem::take(&mut sink.fetched),
            (Err(e), Some(c)) if had_data => {
                stale.entry(format!("{e:#}")).or_default().push(name.clone());
                c.prs::<MergedPr>(&scope)?
            }
            (Err(e), _) => {
                failed.entry(format!("{e:#}")).or_default().push(name.clone());
                missing.push(name.clone());
                continue;
            }
        };
        out.push(RepoPrs {
            repo: name,
            role: repo.role,
            prs: prs.into_iter().filter(|pr| pr.merged_at >= range_start).collect(),
        });
    }
    if s.use_cache {
        Cache::open(&s.ws.state_dir().join("cache.sqlite"))?.record_timing(
            PULLS,
            fetched_total,
            started.elapsed().as_secs_f64(),
        )?;
    }
    p.finish(PULLS);
    for (e, names) in stale {
        p.warn(format!(
            "cannot fetch PRs ({e}): using the PRs stored by an earlier run for {}",
            names.join(", ")
        ));
    }
    for (e, names) in failed {
        p.warn(format!("cannot fetch PRs ({e}): {}", names.join(", ")));
    }
    if !missing.is_empty() {
        // Some repos' PRs are missing: partial counts would read as a drop.
        p.warn(format!(
            "peer review not measured: no PR data for {}",
            missing.join(", ")
        ));
        return Ok(None);
    }
    Ok(Some(Pulls {
        info: PullsInfo {
            host: match code_host.kind {
                CodeHostKind::Github => "github".into(),
                CodeHostKind::Gitlab => "gitlab".into(),
            },
            repos: out.iter().map(|r| r.repo.clone()).collect(),
            sizes: code_host.kind == CodeHostKind::Github,
        },
        repos: out,
    }))
}

/// The PR's stack (spec §8): the repo role, or in a per-file repo the role with the most added
/// source lines (the first such role on a tie, in file order).
pub fn stack(role: RepoRole, pr: &MergedPr, classifier: &Classifier) -> Option<Role> {
    match role {
        RepoRole::Fixed(r) => Some(r),
        RepoRole::PerFile => {
            let mut lines: Vec<(Role, u64)> = Vec::new();
            for f in pr.files.iter().flatten().filter(|f| classifier.is_source(&f.path)) {
                let r = classifier.role_of_file(&f.path);
                match lines.iter_mut().find(|(x, _)| *x == r) {
                    Some((_, n)) => *n += f.additions,
                    None => lines.push((r, f.additions)),
                }
            }
            let top = lines.iter().map(|(_, n)| *n).max()?;
            lines.iter().find(|(_, n)| *n == top).map(|(r, _)| *r)
        }
    }
}

fn ms(d: chrono::TimeDelta) -> u64 {
    d.num_milliseconds().max(0) as u64
}

/// Resolve PRs to people and stacks. `exclude_authors`: drop PRs by bots and externals (spec §1.3;
/// the prototype kept them).
pub fn inputs(repos: &[RepoPrs], identity: &Identity, classifier: &Classifier, exclude_authors: bool) -> Vec<PrInput> {
    let mut out = Vec::new();
    for r in repos {
        for pr in &r.prs {
            let author = pr
                .author
                .as_ref()
                .map(|a| identity.resolve_login(&a.login, a.name.as_deref(), a.bot));
            if exclude_authors && author.as_ref().is_some_and(|a| a.excluded.is_some()) {
                continue;
            }
            let person = author.map(|a| a.person);
            let mut reviewers: Vec<String> = Vec::new();
            let mut first: Option<DateTime<FixedOffset>> = None;
            for rv in &pr.reviews {
                let by = identity.resolve_login(&rv.by.login, rv.by.name.as_deref(), rv.by.bot);
                if by.excluded == Some(Exclusion::Bot) || Some(&by.person) == person.as_ref() {
                    continue;
                }
                first = Some(first.map_or(rv.at, |f| f.min(rv.at)));
                if !reviewers.contains(&by.person) {
                    reviewers.push(by.person);
                }
            }
            out.push(PrInput {
                merged: pr.merged_at.date_naive(),
                cycle_ms: ms(pr.merged_at - pr.created_at),
                first_review_ms: first.map(|f| ms(f - pr.created_at)),
                author: person,
                reviewers,
                stack: stack(r.role, pr, classifier),
                size: pr.additions.zip(pr.deletions).map(|(a, d)| a + d),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_codehost::{Account, FileChange, Review};

    fn t(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    fn pr(author: &str, reviews: &[(&str, &str)]) -> MergedPr {
        MergedPr {
            id: "#1".into(),
            author: Some(Account {
                login: author.into(),
                name: None,
                bot: false,
            }),
            created_at: t("2025-03-01T10:00:00Z"),
            merged_at: t("2025-03-02T22:30:00-03:00"),
            updated_at: None,
            additions: Some(30),
            deletions: Some(5),
            files: None,
            reviews: reviews
                .iter()
                .map(|(by, at)| Review {
                    by: Account {
                        login: by.to_string(),
                        name: None,
                        bot: false,
                    },
                    at: t(at),
                })
                .collect(),
        }
    }

    #[test]
    fn resolves_reviewers_and_times() {
        let mut people = pm_config::People::default();
        people
            .persons
            .insert("Ann".into(), vec!["ann-gh".into(), "ann@example.com".into()]);
        people.externals = vec!["Ext".into()];
        let id = Identity::new(&people);
        let c = Classifier::default();
        let repos = vec![RepoPrs {
            repo: "api".into(),
            role: RepoRole::Fixed(Role::Backend),
            prs: vec![
                pr(
                    "ann-gh",
                    &[
                        ("ann@example.com", "2025-03-01T10:30:00Z"),
                        ("renovate[bot]", "2025-03-01T10:31:00Z"),
                        ("bob", "2025-03-01T12:00:00Z"),
                        ("cy", "2025-03-01T13:00:00+02:00"),
                        ("bob", "2025-03-01T13:00:00Z"),
                    ],
                ),
                pr("Ext", &[]),
            ],
        }];
        let ins = inputs(&repos, &id, &c, true);
        assert_eq!(ins.len(), 1, "the external's PR is dropped");
        let p = &ins[0];
        assert_eq!(p.author.as_deref(), Some("Ann"));
        assert_eq!(p.reviewers, ["bob", "cy"]);
        // cy at 11:00Z came first; the author's own comment (another login) and bots don't count.
        assert_eq!(p.first_review_ms, Some(3_600_000));
        assert_eq!(p.merged.to_string(), "2025-03-02", "the merge date in its own offset");
        assert_eq!(p.size, Some(35));
        assert_eq!(inputs(&repos, &id, &c, false).len(), 2);
    }

    #[test]
    fn per_file_stack_is_the_dominant_role() {
        let c = Classifier::default();
        let mut p = pr("ann", &[]);
        p.files = Some(vec![
            FileChange {
                path: "web/App.tsx".into(),
                additions: 40,
            },
            FileChange {
                path: "api/main.go".into(),
                additions: 25,
            },
            FileChange {
                path: "api/db.go".into(),
                additions: 25,
            },
            FileChange {
                path: "README.md".into(),
                additions: 500,
            },
        ]);
        assert_eq!(stack(RepoRole::PerFile, &p, &c), Some(Role::Backend));
        assert_eq!(stack(RepoRole::Fixed(Role::Mobile), &p, &c), Some(Role::Mobile));
        p.files = None;
        assert_eq!(stack(RepoRole::PerFile, &p, &c), None);
    }
}
