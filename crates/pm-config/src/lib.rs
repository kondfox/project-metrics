//! The project definition, `pmx.toml` (metrics spec §10.1, plan §3.3).

pub mod edit;
pub mod identity;
pub mod import;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use chrono::NaiveDate;
use pm_classify::{Role, RoleRules};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use identity::{Author, Exclusion, Identity};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("{path}: {source}")]
    Parse {
        path: PathBuf,
        source: Box<toml::de::Error>,
    },
    #[error("invalid config: {0}")]
    Invalid(String),
}

pub const CONFIG_FILE: &str = "pmx.toml";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub project: Project,
    #[serde(default, rename = "repo")]
    pub repos: Vec<RepoConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_rules: Option<RoleRules>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_host: Option<CodeHost>,
    #[serde(default)]
    pub people: People,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fte: Option<Fte>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets_triage: Vec<SecretsTriage>,
    /// LLM classifiers (spec §10.3). Parsed in M5; kept verbatim until then.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub classifiers: BTreeMap<String, toml::Table>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub velocity: Option<toml::Table>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub name: String,
    #[serde(with = "date_str")]
    pub range_start: NaiveDate,
    pub breadth_roles: Vec<Role>,
    #[serde(default = "yes")]
    pub ai_attribution: bool,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoConfig {
    /// Display name; defaults to the last path or URL segment without `.git`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Pin a commit instead of following `branch` (spec §1.1, reproducible runs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    pub role: RepoRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_host_id: Option<CodeHostId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_policy: Option<DataPolicy>,
    /// Override of lockfile auto-discovery (spec §7.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security_lockfiles: Option<Vec<String>>,
}

impl RepoConfig {
    pub fn display_name(&self) -> String {
        if let Some(n) = &self.name {
            return n.clone();
        }
        let src = self.path.as_deref().or(self.url.as_deref()).unwrap_or("repo");
        let last = src.trim_end_matches('/').rsplit(['/', ':']).next().unwrap_or(src);
        last.strip_suffix(".git").unwrap_or(last).to_string()
    }

    /// The revision to measure: the pinned `rev`, else `branch`, else the remote's default branch.
    pub fn revision(&self) -> Option<&str> {
        self.rev.as_deref().or(self.branch.as_deref())
    }
}

/// A repo's stack role, or `per-file` for a repo holding several stacks (spec §1.5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RepoRole {
    Fixed(Role),
    #[default]
    PerFile,
}

impl fmt::Display for RepoRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RepoRole::Fixed(r) => r.fmt(f),
            RepoRole::PerFile => f.write_str("per-file"),
        }
    }
}

impl FromStr for RepoRole {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        if s == "per-file" {
            Ok(RepoRole::PerFile)
        } else {
            s.parse::<Role>()
                .map(RepoRole::Fixed)
                .map_err(|e| format!("{e}, or per-file"))
        }
    }
}

impl Serialize for RepoRole {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for RepoRole {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// GitLab project id or GitHub `owner/repo`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CodeHostId {
    Number(i64),
    Name(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DataPolicy {
    LocalOnly,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeHost {
    #[serde(rename = "type")]
    pub kind: CodeHostKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_env: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodeHostKind {
    Github,
    Gitlab,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fte {
    pub source: FteSource,
    /// `static`: one value for every date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// `static`: dated values instead (an open `to` means until further notice).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub periods: Vec<FtePeriod>,
    /// `file`: a JSON file in the spec §10.2 shape, keyed by project name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

/// Where FTE comes from (spec §10.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FteSource {
    Static,
    File,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FtePeriod {
    #[serde(with = "date_str")]
    pub from: NaiveDate,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "opt_date_str")]
    pub to: Option<NaiveDate>,
    pub fte: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretsTriage {
    pub repo: String,
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    pub verdict: TriageVerdict,
    pub by: String,
    pub date: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TriageVerdict {
    FalsePositive,
    Rotated,
    Accepted,
}

/// `[people]`: canonical name → emails and code-host logins, plus `bots` and `externals`
/// (spec §1.3, §10.1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct People {
    pub persons: BTreeMap<String, Vec<String>>,
    pub bots: Vec<String>,
    pub externals: Vec<String>,
}

impl Serialize for People {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m: BTreeMap<&str, &Vec<String>> = self.persons.iter().map(|(k, v)| (k.as_str(), v)).collect();
        if !self.bots.is_empty() {
            m.insert("bots", &self.bots);
        }
        if !self.externals.is_empty() {
            m.insert("externals", &self.externals);
        }
        m.serialize(s)
    }
}

impl<'de> Deserialize<'de> for People {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let mut m = BTreeMap::<String, Vec<String>>::deserialize(d)?;
        Ok(People {
            bots: m.remove("bots").unwrap_or_default(),
            externals: m.remove("externals").unwrap_or_default(),
            persons: m,
        })
    }
}

mod opt_date_str {
    use chrono::NaiveDate;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(d: &Option<NaiveDate>, s: S) -> Result<S::Ok, S::Error> {
        match d {
            Some(d) => s.collect_str(&d.format("%Y-%m-%d")),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<NaiveDate>, D::Error> {
        let s = String::deserialize(d)?;
        NaiveDate::parse_from_str(&s, "%Y-%m-%d")
            .map(Some)
            .map_err(|e| serde::de::Error::custom(format!("expected a YYYY-MM-DD date, got `{s}`: {e}")))
    }
}

mod date_str {
    use chrono::NaiveDate;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(d: &NaiveDate, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(&d.format("%Y-%m-%d"))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<NaiveDate, D::Error> {
        let s = String::deserialize(d)?;
        NaiveDate::parse_from_str(&s, "%Y-%m-%d")
            .map_err(|e| serde::de::Error::custom(format!("expected a YYYY-MM-DD date, got `{s}`: {e}")))
    }
}

impl Config {
    pub fn parse(text: &str, path: &Path) -> Result<Config, ConfigError> {
        let cfg: Config = toml::from_str(text).map_err(|e| ConfigError::Parse {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        let bad = |m: String| Err(ConfigError::Invalid(m));
        if self.repos.is_empty() {
            return bad("no [[repo]] entries".into());
        }
        let mut names = BTreeSet::new();
        for r in &self.repos {
            let name = r.display_name();
            match (&r.path, &r.url) {
                (Some(_), Some(_)) => return bad(format!("repo `{name}`: set `path` or `url`, not both")),
                (None, None) => return bad(format!("repo `{name}`: needs `path` or `url`")),
                _ => {}
            }
            if !names.insert(name.clone()) {
                return bad(format!("duplicate repo name `{name}` (set `name` to tell them apart)"));
            }
        }
        if self.project.breadth_roles.is_empty() {
            return bad("[project] breadth_roles is empty".into());
        }
        if let Some(f) = &self.fte {
            match f.source {
                FteSource::Static if f.value.is_none() && f.periods.is_empty() => {
                    return bad("[fte] source = \"static\" needs `value` or `periods`".into());
                }
                FteSource::File if f.file.is_none() => return bad("[fte] source = \"file\" needs `file`".into()),
                _ => {}
            }
        }
        for t in &self.secrets_triage {
            if !names.contains(&t.repo) {
                return bad(format!("[[secrets_triage]] names unknown repo `{}`", t.repo));
            }
        }
        let mut owner: BTreeMap<String, &str> = BTreeMap::new();
        for (person, ids) in &self.people.persons {
            for id in ids {
                if let Some(prev) = owner.insert(id.to_lowercase(), person) {
                    if prev != person {
                        return bad(format!("[people] `{id}` is listed for both `{prev}` and `{person}`"));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn classifier(&self, tests: pm_classify::TestRules) -> pm_classify::Classifier {
        pm_classify::Classifier::new(tests, self.role_rules.clone().unwrap_or_default())
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("config serializes")
    }
}

/// A workspace folder: `pmx.toml` plus `.pmx/` and `out/` (plan §3.1).
#[derive(Clone, Debug)]
pub struct Workspace {
    pub root: PathBuf,
    pub config: Config,
}

impl Workspace {
    pub fn load(root: &Path) -> Result<Workspace, ConfigError> {
        let path = root.join(CONFIG_FILE);
        let text = std::fs::read_to_string(&path).map_err(|source| ConfigError::Read {
            path: path.clone(),
            source,
        })?;
        Ok(Workspace {
            root: root.to_path_buf(),
            config: Config::parse(&text, &path)?,
        })
    }

    pub fn state_dir(&self) -> PathBuf {
        self.root.join(".pmx")
    }

    pub fn out_dir(&self) -> PathBuf {
        self.root.join("out")
    }

    /// Where the repo's git data is read from: the local clone, or pmx's bare clone of a URL.
    pub fn repo_dir(&self, repo: &RepoConfig) -> PathBuf {
        match (&repo.path, &repo.url) {
            (Some(p), _) => expand_path(p, &self.root),
            _ => self
                .state_dir()
                .join("repos")
                .join(format!("{}.git", repo.display_name())),
        }
    }
}

/// Expand `~/` and resolve relative paths against `base`.
pub fn expand_path(p: &str, base: &Path) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    let pb = PathBuf::from(p);
    if pb.is_absolute() { pb } else { base.join(pb) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../../../examples/pmx.toml");

    #[test]
    fn example_config_parses() {
        let cfg = Config::parse(EXAMPLE, Path::new("examples/pmx.toml")).unwrap();
        assert_eq!(cfg.project.name, "Acme Shop");
        assert_eq!(cfg.repos.len(), 3);
        assert_eq!(cfg.repos[0].role, RepoRole::Fixed(Role::Backend));
        assert_eq!(cfg.repos[2].role, RepoRole::PerFile);
        assert_eq!(cfg.repos[1].display_name(), "shop-web");
        assert_eq!(cfg.repos[0].code_host_id, Some(CodeHostId::Number(785)));
        assert_eq!(cfg.people.persons["Jane Doe"], vec!["jane.doe@example.com", "jdoe"]);
        assert_eq!(cfg.people.bots, vec!["gitlab-runner@example.com"]);
        assert_eq!(cfg.people.externals, vec!["Sam Contractor"]);
        assert!(cfg.project.ai_attribution);
    }

    #[test]
    fn round_trips_through_toml() {
        let cfg = Config::parse(EXAMPLE, Path::new("x")).unwrap();
        let again = Config::parse(&cfg.to_toml(), Path::new("x")).unwrap();
        assert_eq!(again.people, cfg.people);
        assert_eq!(again.repos.len(), cfg.repos.len());
    }

    #[test]
    fn rejects_bad_configs() {
        let base = r#"
            [project]
            name = "X"
            range_start = "2025-01-01"
            breadth_roles = ["backend"]
        "#;
        let err = |extra: &str| {
            Config::parse(&format!("{base}{extra}"), Path::new("x"))
                .unwrap_err()
                .to_string()
        };
        assert!(err("").contains("no [[repo]]"));
        assert!(err("[[repo]]\nrole = \"backend\"\n").contains("needs `path` or `url`"));
        assert!(err("[[repo]]\npath = \"a\"\nrole = \"fullstack\"\n").contains("unknown role"));
        assert!(
            err("[[repo]]\npath = \"x/a\"\nrole = \"qa\"\n[[repo]]\npath = \"y/a\"\nrole = \"qa\"\n")
                .contains("duplicate repo name")
        );
        assert!(
            err("[[repo]]\npath = \"a\"\nrole = \"qa\"\n[people]\n\"A\" = [\"x@e.com\"]\n\"B\" = [\"X@e.com\"]\n")
                .contains("listed for both")
        );
        assert!(err("[[repo]]\npath = \"a\"\nrole = \"qa\"\nbogus = 1\n").contains("bogus"));
    }

    #[test]
    fn repo_names() {
        let r = |path: Option<&str>, url: Option<&str>| RepoConfig {
            path: path.map(String::from),
            url: url.map(String::from),
            ..Default::default()
        };
        assert_eq!(r(Some("~/src/acme/api/"), None).display_name(), "api");
        assert_eq!(r(None, Some("git@host:acme/web.git")).display_name(), "web");
        assert_eq!(r(None, Some("https://host/acme/web")).display_name(), "web");
    }
}
