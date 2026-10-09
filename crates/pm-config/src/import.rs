//! Convert a prototype `config_*.json` (plus its externals list) into a [`Config`] (plan §3.2,
//! `pmx import-config`).

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde_json::Value;

use crate::{CodeHost, CodeHostId, CodeHostKind, Config, ConfigError, People, Project, RepoConfig, RepoRole};

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

/// `externals` is the prototype's `externals.json` (`{"<project>": [{name, email}], "*": [...]}`).
pub fn from_prototype(cfg: &Value, externals: Option<&Value>) -> Result<Imported, ConfigError> {
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
        fte: None,
        secrets_triage: Vec::new(),
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
        let imp = from_prototype(&cfg, Some(&ext)).unwrap();
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
        let imp = from_prototype(&cfg, None).unwrap();
        assert_eq!(imp.config.repos[0].role, RepoRole::PerFile);
        assert!(imp.notes.iter().any(|n| n.contains("per-file")));
    }
}
