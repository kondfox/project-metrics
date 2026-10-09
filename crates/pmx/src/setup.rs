//! `pmx init` and `pmx repo add` (plan §3.2): find clones, suggest a branch, a role and the
//! code host for each.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use chrono::{Datelike, NaiveDate};
use pm_classify::{Classifier, Role};
use pm_config::{CodeHost, CodeHostId, CodeHostKind, Config, People, Project, RepoConfig, RepoRole};
use pm_git::Git;

/// A suggested `[[repo]]` entry and why.
#[derive(Clone, Debug)]
pub struct Suggestion {
    pub repo: RepoConfig,
    pub why: String,
    pub host: Option<RemoteHost>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteHost {
    pub kind: CodeHostKind,
    /// API base for self-hosted GitLab.
    pub base: Option<String>,
    /// GitHub `owner/repo`, or the GitLab project path (the API accepts it as the id).
    pub id: String,
}

/// Parse `git@host:group/repo.git`, `ssh://git@host:22/group/repo` or `https://host/group/repo`.
pub fn remote_host(url: &str) -> Option<RemoteHost> {
    let (host, path) = if let Some(rest) = url.split_once("://").map(|(_, r)| r) {
        let rest = rest.rsplit_once('@').map(|(_, r)| r).unwrap_or(rest);
        let (host, path) = rest.split_once('/')?;
        (host.split(':').next()?.to_string(), path.to_string())
    } else {
        let (user_host, path) = url.split_once(':')?;
        let host = user_host.rsplit_once('@').map(|(_, h)| h).unwrap_or(user_host);
        (host.to_string(), path.to_string())
    };
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path).to_string();
    if path.is_empty() {
        return None;
    }
    let host = host.to_lowercase();
    if host == "github.com" {
        Some(RemoteHost {
            kind: CodeHostKind::Github,
            base: None,
            id: path,
        })
    } else if host.contains("gitlab") {
        Some(RemoteHost {
            kind: CodeHostKind::Gitlab,
            base: Some(format!("https://{host}")),
            id: path,
        })
    } else {
        None
    }
}

const FRONTEND_MARKERS: &[&str] = &[
    "vite.config.",
    "next.config.",
    "nuxt.config.",
    "svelte.config.",
    "angular.json",
    "vue.config.",
    "webpack.config.",
];
const BACKEND_MARKERS: &[&str] = &[
    "pom.xml",
    "go.mod",
    "requirements.txt",
    "pyproject.toml",
    "gemfile",
    "composer.json",
    "cargo.toml",
    "nest-cli.json",
    "manage.py",
    "build.gradle",
    "build.gradle.kts",
];
const MOBILE_MARKERS: &[&str] = &["androidmanifest.xml", "pubspec.yaml", "podfile", "metro.config.js"];
const QA_MARKERS: &[&str] = &["cypress.config.", "playwright.config."];

/// Suggest a role from the files in the tree (plan §3.2). A suggestion only: the user confirms.
pub fn detect_role(files: &[String], cls: &Classifier) -> (RepoRole, String) {
    let files: Vec<&String> = files.iter().filter(|f| !cls.is_generated(f)).collect();
    let name = |f: &str| f.rsplit('/').next().unwrap_or(f).to_lowercase();
    let any = |markers: &[&str]| {
        files.iter().any(|f| {
            let n = name(f);
            markers
                .iter()
                .any(|m| if m.ends_with('.') { n.starts_with(m) } else { n == *m })
        })
    };
    let lower: Vec<String> = files.iter().map(|f| format!("/{}", f.to_lowercase())).collect();
    let has_dir = |d: &str| lower.iter().any(|f| f.contains(d));

    let android_gradle = has_dir("/android/") || files.iter().any(|f| name(f) == "androidmanifest.xml");
    if any(MOBILE_MARKERS) || has_dir(".xcodeproj/") || (has_dir("/android/") && has_dir("/ios/")) {
        return (RepoRole::Fixed(Role::Mobile), "Android/iOS project files".into());
    }
    let sources: Vec<&String> = files.iter().copied().filter(|f| cls.is_source(f)).collect();
    if sources.is_empty() {
        let docs = files.iter().filter(|f| cls.is_doc(f)).count();
        return if docs > 0 || files.is_empty() {
            (RepoRole::Fixed(Role::Docs), "documents only, no source files".into())
        } else {
            (
                RepoRole::Fixed(Role::Infra),
                "configuration only, no source files".into(),
            )
        };
    }
    let tests = sources.iter().filter(|f| cls.matches_test(f)).count();
    let test_share = tests as f64 / sources.len() as f64;
    if test_share >= 0.6 || (any(QA_MARKERS) && test_share >= 0.3) {
        return (
            RepoRole::Fixed(Role::Qa),
            format!("{:.0}% of source files are tests", test_share * 100.0),
        );
    }
    let non_test: Vec<&&String> = sources.iter().filter(|f| !cls.matches_test(f)).collect();
    let tf = non_test
        .iter()
        .filter(|f| f.ends_with(".tf") || f.ends_with(".tfvars"))
        .count();
    if !non_test.is_empty() && tf as f64 / non_test.len() as f64 >= 0.6 {
        return (RepoRole::Fixed(Role::Infra), "mostly Terraform".into());
    }
    let fe = any(FRONTEND_MARKERS);
    let be = (any(BACKEND_MARKERS) && !android_gradle)
        || files.iter().any(|f| f.ends_with(".csproj") || f.ends_with(".sln"));
    match (fe, be) {
        (true, false) => return (RepoRole::Fixed(Role::Frontend), "frontend build config".into()),
        (false, true) => return (RepoRole::Fixed(Role::Backend), "backend project files".into()),
        (true, true) => return (RepoRole::PerFile, "both frontend and backend project files".into()),
        _ => {}
    }
    let mut shares: BTreeMap<Role, usize> = BTreeMap::new();
    for f in &non_test {
        *shares.entry(cls.role_of_file(f)).or_default() += 1;
    }
    let (top, n) = shares
        .iter()
        .max_by_key(|(_, n)| **n)
        .map(|(r, n)| (*r, *n))
        .unwrap_or((Role::Backend, 0));
    let share = n as f64 / non_test.len().max(1) as f64;
    if share >= 0.8 {
        (
            RepoRole::Fixed(top),
            format!("{:.0}% of source files look {top}", share * 100.0),
        )
    } else {
        (RepoRole::PerFile, "mixed stacks".into())
    }
}

/// `~/…` for paths under the home folder, else absolute.
pub fn config_path(path: &Path) -> String {
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        if let Ok(rel) = abs.strip_prefix(&home) {
            return format!("~/{}", rel.display());
        }
    }
    abs.display().to_string()
}

fn is_clone(dir: &Path) -> bool {
    dir.join(".git").exists() || (dir.join("HEAD").is_file() && dir.join("objects").is_dir())
}

/// Git clones in `dirs`: each dir itself, or its children and grandchildren.
pub fn scan(dirs: &[PathBuf], skip: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, skip: &Path, out: &mut Vec<PathBuf>) {
        if dir.starts_with(skip) {
            return;
        }
        if is_clone(dir) {
            out.push(dir.to_path_buf());
            return;
        }
        if depth == 0 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut children: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .filter(|p| {
                let n = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                !n.starts_with('.') && n != "node_modules" && n != "out"
            })
            .collect();
        children.sort();
        for c in children {
            walk(&c, depth - 1, skip, out);
        }
    }
    let mut out = Vec::new();
    for d in dirs {
        walk(d, 2, skip, &mut out);
    }
    out.dedup();
    out
}

/// Suggest a `[[repo]]` for a clone (local path) or for pmx's bare clone of a URL.
pub fn suggest(git: &Git, location: RepoLocation, cls: &Classifier) -> Result<Suggestion> {
    let branch = git.suggested_branch();
    let sha = git
        .rev_parse(&branch)
        .map_err(|e| anyhow!("{}: cannot resolve `{branch}`: {e}", git.dir().display()))?;
    let files = git.ls_tree(&sha)?;
    let (role, why) = detect_role(&files, cls);
    let url = match &location {
        RepoLocation::Url(u) => Some(u.clone()),
        RepoLocation::Path(_) => git.remote_url(),
    };
    let host = url.as_deref().and_then(remote_host);
    let mut repo = RepoConfig {
        branch: Some(branch),
        role,
        code_host_id: host.as_ref().map(|h| CodeHostId::Name(h.id.clone())),
        ..Default::default()
    };
    match location {
        RepoLocation::Path(p) => repo.path = Some(config_path(&p)),
        RepoLocation::Url(u) => repo.url = Some(u),
    }
    Ok(Suggestion { repo, why, host })
}

pub enum RepoLocation {
    Path(PathBuf),
    Url(String),
}

/// Is this argument a URL rather than a local path?
pub fn looks_like_url(s: &str) -> bool {
    s.contains("://") || (s.contains('@') && s.contains(':') && !Path::new(s).exists())
}

/// First day of the month twelve months before `today`.
pub fn default_range_start(today: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(today.year() - 1, today.month(), 1).expect("valid date")
}

pub fn default_breadth_roles(repos: &[RepoConfig]) -> Vec<Role> {
    let mut roles = Vec::new();
    for r in repos {
        match r.role {
            RepoRole::Fixed(Role::Docs | Role::Data) => {}
            RepoRole::Fixed(role) => roles.push(role),
            RepoRole::PerFile => roles.extend([Role::Frontend, Role::Backend, Role::Qa, Role::Infra]),
        }
    }
    if roles.is_empty() {
        roles = vec![Role::Frontend, Role::Backend, Role::Mobile, Role::Qa, Role::Infra];
    }
    Role::ALL.into_iter().filter(|r| roles.contains(r)).collect()
}

/// The code host shared by every repo, if they agree.
pub fn common_host(suggestions: &[Suggestion]) -> Option<CodeHost> {
    let first = suggestions.first()?.host.as_ref()?;
    if suggestions.iter().all(|s| {
        s.host
            .as_ref()
            .is_some_and(|h| h.kind == first.kind && h.base == first.base)
    }) {
        Some(CodeHost {
            kind: first.kind,
            base: first.base.clone(),
            group: None,
            token_env: Some(match first.kind {
                CodeHostKind::Github => "GITHUB_TOKEN".into(),
                CodeHostKind::Gitlab => "GITLAB_TOKEN".into(),
            }),
        })
    } else {
        None
    }
}

/// Give suggestions distinct names (two clones can share a folder name).
pub fn dedupe_names(suggestions: &mut [Suggestion]) {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for s in suggestions.iter_mut() {
        let name = s.repo.display_name();
        let n = seen.entry(name.clone()).or_default();
        *n += 1;
        if *n > 1 {
            s.repo.name = Some(format!("{name}-{n}"));
        }
    }
}

pub fn new_config(name: String, range_start: NaiveDate, suggestions: &[Suggestion]) -> Result<Config> {
    if suggestions.is_empty() {
        bail!("no git clones found; pass the folders that hold them: `pmx init ~/src/acme`");
    }
    let repos: Vec<RepoConfig> = suggestions.iter().map(|s| s.repo.clone()).collect();
    let config = Config {
        project: Project {
            name,
            range_start,
            breadth_roles: default_breadth_roles(&repos),
            ai_attribution: true,
        },
        code_host: common_host(suggestions),
        repos,
        role_rules: None,
        people: People::default(),
        fte: None,
        secrets_triage: Vec::new(),
        snapshots: Default::default(),
        classifiers: BTreeMap::new(),
        velocity: None,
    };
    config.validate()?;
    Ok(config)
}

pub const CONFIG_HEADER: &str = "# pmx project definition (wiki/metrics-spec.md §10.1). Safe to commit: tokens are\n\
# referenced by env-var name only. Edit with `pmx repo …` and `pmx people`, or by hand.\n\n";

#[cfg(test)]
mod tests {
    use super::*;

    fn files(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn roles_from_trees() {
        let c = Classifier::default();
        let cases: &[(&[&str], RepoRole)] = &[
            (
                &[
                    "web/vite.config.ts",
                    "web/src/App.tsx",
                    "web/src/api.ts",
                    "web/src/hooks.ts",
                ],
                RepoRole::Fixed(Role::Frontend),
            ),
            (
                &["Api/Api.csproj", "Api/Program.cs", "Api/Users.cs"],
                RepoRole::Fixed(Role::Backend),
            ),
            (
                &[
                    "app/src/main/AndroidManifest.xml",
                    "app/src/main/java/Main.kt",
                    "build.gradle",
                ],
                RepoRole::Fixed(Role::Mobile),
            ),
            (
                &["main.tf", "variables.tf", "modules/vpc/main.tf"],
                RepoRole::Fixed(Role::Infra),
            ),
            (
                &[
                    "cypress.config.ts",
                    "cypress/e2e/login.cy.ts",
                    "cypress/e2e/cart.cy.ts",
                    "cypress/support/cmd.ts",
                ],
                RepoRole::Fixed(Role::Qa),
            ),
            (
                &["README.md", "docs/setup.md", "docs/img.png"],
                RepoRole::Fixed(Role::Docs),
            ),
            (
                &["web/next.config.js", "web/pages/index.tsx", "api/go.mod", "api/main.go"],
                RepoRole::PerFile,
            ),
            (
                &["src/a.ts", "src/b.ts", "src/c.ts", "src/d.ts", "src/e.ts"],
                RepoRole::Fixed(Role::Backend),
            ),
            (&["src/a.ts", "src/b.tsx", "src/c.sql", "src/d.ts"], RepoRole::PerFile),
        ];
        for (tree, want) in cases {
            assert_eq!(detect_role(&files(tree), &c).0, *want, "{tree:?}");
        }
    }

    #[test]
    fn remotes() {
        assert_eq!(
            remote_host("git@github.com:acme/shop.git"),
            Some(RemoteHost {
                kind: CodeHostKind::Github,
                base: None,
                id: "acme/shop".into()
            })
        );
        assert_eq!(
            remote_host("https://gitlab.example.com/acme/team/shop-api.git"),
            Some(RemoteHost {
                kind: CodeHostKind::Gitlab,
                base: Some("https://gitlab.example.com".into()),
                id: "acme/team/shop-api".into()
            })
        );
        assert_eq!(
            remote_host("ssh://git@gitlab.example.com:2222/acme/shop.git").map(|h| h.id),
            Some("acme/shop".into())
        );
        assert_eq!(remote_host("https://git.example.com/acme/shop.git"), None);
        assert_eq!(remote_host("/srv/git/shop.git"), None);
    }

    #[test]
    fn defaults() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 9).unwrap();
        assert_eq!(default_range_start(d), NaiveDate::from_ymd_opt(2025, 10, 1).unwrap());
        let r = |role| RepoConfig {
            role,
            path: Some("x".into()),
            ..Default::default()
        };
        assert_eq!(
            default_breadth_roles(&[r(RepoRole::Fixed(Role::Backend)), r(RepoRole::Fixed(Role::Docs))]),
            vec![Role::Backend]
        );
        assert_eq!(default_breadth_roles(&[]).len(), 5);
        assert!(looks_like_url("git@github.com:acme/shop.git"));
        assert!(looks_like_url("https://example.com/x.git"));
        assert!(!looks_like_url("../shop"));
    }
}
