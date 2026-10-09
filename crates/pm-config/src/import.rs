//! Convert a prototype `config_*.json` (plus its externals list) into a [`Config`] (plan §3.2,
//! `pmx import-config`).

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde_json::Value;

use crate::{
    CodeHost, CodeHostId, CodeHostKind, Config, ConfigError, Fte, FtePeriod, FteSource, People, Project, RepoConfig,
    RepoRole, SecretsTriage, TriageVerdict,
};

pub struct Imported {
    pub config: Config,
    /// The prototype's trailing multi-stack window in days (`state_window`). The spec replaced it
    /// with the selected range; the parity harness still needs it.
    pub state_window_days: Option<i64>,
    /// Things the importer dropped or changed, for the user to review.
    pub notes: Vec<String>,
}

fn invalid(m: impl Into<String>) -> ConfigError {
    ConfigError::Invalid(m.into())
}

fn str_field<'a>(v: &'a Value, key: &str) -> Result<&'a str, ConfigError> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("prototype config: missing string `{key}`")))
}

fn is_sha(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The prototype's side files, all optional.
#[derive(Default)]
pub struct SideFiles<'a> {
    /// `externals.json`: `{"<project>": [{name, email}], "*": [...]}`.
    pub externals: Option<&'a Value>,
    /// `fte.json`: `{"<project>": <fte>}`.
    pub fte: Option<&'a Value>,
    /// `secrets_triage.json`: `{"entries": [{repo, file, rule?, verdict, by?, date?}]}`.
    pub secrets_triage: Option<&'a Value>,
}

pub fn from_prototype(cfg: &Value, side: &SideFiles) -> Result<Imported, ConfigError> {
    let externals = side.externals;
    let mut notes = Vec::new();
    let name = str_field(cfg, "project")?.to_string();
    let range_start = NaiveDate::parse_from_str(str_field(cfg, "range_start")?, "%Y-%m-%d")
        .map_err(|e| invalid(format!("prototype config: range_start: {e}")))?;
    let breadth_roles = cfg
        .get("breadth_roles")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("prototype config: missing `breadth_roles`"))?
        .iter()
        .map(|r| r.as_str().unwrap_or_default().parse().map_err(invalid))
        .collect::<Result<Vec<_>, _>>()?;
    let workspace = cfg.get("workspace").and_then(Value::as_str);
    let monorepo = cfg.get("monorepo").and_then(Value::as_bool).unwrap_or(false);
    let lockfiles = cfg.get("security_lockfiles").and_then(Value::as_object);

    let mut repos = Vec::new();
    for r in cfg
        .get("repos")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("prototype config: missing `repos`"))?
    {
        let rname = str_field(r, "name")?.to_string();
        let branch = r.get("branch").and_then(Value::as_str).map(String::from);
        let (branch, rev) = match branch {
            Some(b) if is_sha(&b) => (None, Some(b)),
            b => (b, None),
        };
        let role = if monorepo {
            RepoRole::PerFile
        } else {
            str_field(r, "role")?.parse::<RepoRole>().map_err(invalid)?
        };
        let path = match workspace {
            Some(ws) => format!("{}/{}", ws.trim_end_matches('/'), rname),
            None => rname.clone(),
        };
        repos.push(RepoConfig {
            name: Some(rname.clone()),
            path: Some(path),
            branch,
            rev,
            role,
            code_host_id: r.get("gitlab_id").and_then(Value::as_i64).map(CodeHostId::Number),
            security_lockfiles: lockfiles
                .and_then(|l| l.get(&rname))
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()),
            ..Default::default()
        });
    }
    let repo_names: Vec<String> = repos.iter().map(RepoConfig::display_name).collect();
    if monorepo {
        notes.push("`monorepo: true` became `role = \"per-file\"` on every repo".into());
    }

    let mut people = People::default();
    if let Some(identity) = cfg.get("identity").and_then(Value::as_object) {
        let mut persons: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (k, v) in identity {
            if k == "__bots__" {
                people.bots = v
                    .as_array()
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default();
            } else if !k.starts_with("__") {
                if let Some(p) = v.as_str() {
                    persons.entry(p.to_string()).or_default().push(k.clone());
                }
            }
        }
        people.persons = persons;
    }
    if let Some(ext) = externals {
        for key in ["*", name.as_str()] {
            for e in ext.get(key).and_then(Value::as_array).into_iter().flatten() {
                for field in ["name", "email"] {
                    if let Some(s) = e.get(field).and_then(Value::as_str) {
                        if !s.trim().is_empty() {
                            people.externals.push(s.to_string());
                        }
                    }
                }
            }
        }
    }

    let code_host = cfg.get("gitlab").map(|g| CodeHost {
        kind: CodeHostKind::Gitlab,
        base: g.get("base").and_then(Value::as_str).map(String::from),
        group: g.get("group").and_then(Value::as_str).map(String::from),
        token_env: g.get("token_env").and_then(Value::as_str).map(String::from),
    });

    let state_window_days = cfg.get("state_window").and_then(Value::as_i64);
    if state_window_days.is_some() {
        notes.push("`state_window` dropped: multi-stack now uses the selected range (spec §5)".into());
    }

    let fte = import_fte(side.fte, &name, &mut notes);
    let config = Config {
        project: Project {
            name,
            range_start,
            breadth_roles,
            ai_attribution: cfg.get("ai_attribution").and_then(Value::as_bool).unwrap_or(true),
        },
        repos,
        role_rules: None,
        code_host,
        people,
        fte,
        secrets_triage: import_triage(side.secrets_triage, &repo_names, &mut notes),
        snapshots: Default::default(),
        classifiers: BTreeMap::new(),
        velocity: None,
    };
    config.validate()?;
    Ok(Imported {
        config,
        state_window_days,
        notes,
    })
}

fn import_fte(fte: Option<&Value>, project: &str, notes: &mut Vec<String>) -> Option<Fte> {
    let v = fte?.get(project)?;
    let mut f = Fte {
        source: FteSource::Static,
        value: None,
        periods: Vec::new(),
        file: None,
    };
    if let Some(x) = v.as_f64() {
        f.value = Some(x);
    } else if let Some(list) = v.as_array() {
        for p in list {
            let date = |k: &str| {
                p.get(k)
                    .and_then(Value::as_str)
                    .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            };
            match (date("from"), p.get("fte").and_then(Value::as_f64)) {
                (Some(from), Some(fte)) => f.periods.push(FtePeriod {
                    from,
                    to: date("to"),
                    fte,
                }),
                _ => notes.push(format!("fte: skipped a period without `from` or `fte`: {p}")),
            }
        }
    }
    (f.value.is_some() || !f.periods.is_empty()).then_some(f)
}

fn import_triage(triage: Option<&Value>, repos: &[String], notes: &mut Vec<String>) -> Vec<SecretsTriage> {
    let mut out = Vec::new();
    let entries = triage.and_then(|t| t.get("entries")).and_then(Value::as_array);
    for e in entries.into_iter().flatten() {
        let field = |k: &str| e.get(k).and_then(Value::as_str).map(String::from);
        let (Some(repo), Some(file)) = (field("repo"), field("file")) else {
            continue;
        };
        if !repos.contains(&repo) {
            continue;
        }
        let verdict = match field("verdict").as_deref() {
            Some("false-positive") => TriageVerdict::FalsePositive,
            Some("rotated") => TriageVerdict::Rotated,
            Some("accepted") => TriageVerdict::Accepted,
            other => {
                notes.push(format!(
                    "secrets triage: skipped {repo}/{file}, unknown verdict {other:?}"
                ));
                continue;
            }
        };
        if field("by").is_none() || field("date").is_none() {
            notes.push(format!("secrets triage: {repo}/{file} has no `by`/`date`; set them"));
        }
        out.push(SecretsTriage {
            repo,
            file,
            rule: field("rule"),
            verdict,
            by: field("by").unwrap_or_else(|| "unknown".into()),
            date: field("date").unwrap_or_else(|| "unknown".into()),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_classify::Role;
    use serde_json::json;

    #[test]
    fn converts_a_prototype_config() {
        let cfg = json!({
            "project": "Acme Shop",
            "workspace": "/src/acme",
            "range_start": "2024-09-01",
            "state_window": 90,
            "gitlab": {"base": "https://gitlab.example.com", "group": "acme", "token_env": "GL_TOKEN"},
            "repos": [
                {"name": "shop-api", "branch": "origin/main", "role": "backend", "gitlab_id": 7},
                {"name": "shop-web", "branch": "0123456789abcdef0123456789abcdef01234567", "role": "frontend"}
            ],
            "breadth_roles": ["frontend", "backend"],
            "ai_attribution": false,
            "security_lockfiles": {"shop-web": ["package-lock.json"]},
            "identity": {
                "__bots__": ["ci@example.com"],
                "jane@example.com": "Jane Doe",
                "jdoe@home.example": "Jane Doe"
            }
        });
        let ext = json!({"*": [{"name": "Pat Vendor"}], "Acme Shop": [{"name": "Sam", "email": "sam@agency.example"}]});
        let fte = json!({"Acme Shop": 4.5, "Other": 2});
        let triage = json!({"entries": [
            {"repo": "shop-api", "file": "certs/dev.pem", "verdict": "rotated", "by": "Jane Doe", "date": "2026-10-08"},
            {"repo": "elsewhere", "file": "x.pem", "verdict": "accepted"},
            {"repo": "shop-web", "file": ".env.sample", "rule": "generic-api-key", "verdict": "false-positive"}
        ]});
        let side = SideFiles {
            externals: Some(&ext),
            fte: Some(&fte),
            secrets_triage: Some(&triage),
        };
        let imp = from_prototype(&cfg, &side).unwrap();
        assert_eq!(imp.config.fte.as_ref().unwrap().value, Some(4.5));
        let t = &imp.config.secrets_triage;
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].rule.as_deref(), Some("generic-api-key"));
        assert_eq!(t[1].by, "unknown");
        assert!(imp.notes.iter().any(|n| n.contains("no `by`/`date`")));
        let c = imp.config;
        assert_eq!(imp.state_window_days, Some(90));
        assert!(!c.project.ai_attribution);
        assert_eq!(c.repos[0].path.as_deref(), Some("/src/acme/shop-api"));
        assert_eq!(c.repos[0].branch.as_deref(), Some("origin/main"));
        assert_eq!(c.repos[0].code_host_id, Some(CodeHostId::Number(7)));
        assert_eq!(
            c.repos[1].rev.as_deref(),
            Some("0123456789abcdef0123456789abcdef01234567")
        );
        assert_eq!(c.repos[1].branch, None);
        assert_eq!(c.repos[1].role, RepoRole::Fixed(Role::Frontend));
        assert_eq!(
            c.repos[1].security_lockfiles.as_deref(),
            Some(&["package-lock.json".to_string()][..])
        );
        assert_eq!(
            c.people.persons["Jane Doe"],
            vec!["jane@example.com", "jdoe@home.example"]
        );
        assert_eq!(c.people.bots, vec!["ci@example.com"]);
        assert_eq!(c.people.externals, vec!["Pat Vendor", "Sam", "sam@agency.example"]);
        assert_eq!(c.code_host.unwrap().kind, CodeHostKind::Gitlab);
    }

    #[test]
    fn monorepo_becomes_per_file() {
        let cfg = json!({
            "project": "Mono", "range_start": "2025-01-01", "monorepo": true,
            "repos": [{"name": "mono", "branch": "origin/main", "role": "backend"}],
            "breadth_roles": ["frontend", "backend", "qa"]
        });
        let imp = from_prototype(&cfg, &SideFiles::default()).unwrap();
        assert_eq!(imp.config.repos[0].role, RepoRole::PerFile);
        assert!(imp.notes.iter().any(|n| n.contains("per-file")));
    }
}
