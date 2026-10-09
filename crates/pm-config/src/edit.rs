//! Edit an existing `pmx.toml` in place (for `pmx repo` and `pmx people`), keeping the user's
//! comments and layout. Every edit is validated by parsing the result.

use std::path::Path;

use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

use crate::{Config, ConfigError, People, RepoConfig};

pub struct ConfigDoc {
    doc: DocumentMut,
}

fn invalid(m: impl Into<String>) -> ConfigError {
    ConfigError::Invalid(m.into())
}

/// Serialize a value with `toml`, then re-read it as an editable table.
fn to_table<T: serde::Serialize>(v: &T) -> Result<Table, ConfigError> {
    let text = toml::to_string(v).map_err(|e| invalid(e.to_string()))?;
    let doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| invalid(e.to_string()))?;
    Ok(doc.as_table().clone())
}

fn repo_of(t: &Table) -> Option<RepoConfig> {
    toml::from_str(&t.to_string()).ok()
}

impl ConfigDoc {
    pub fn parse(text: &str) -> Result<ConfigDoc, ConfigError> {
        Ok(ConfigDoc {
            doc: text.parse().map_err(|e: toml_edit::TomlError| invalid(e.to_string()))?,
        })
    }

    pub fn to_text(&self) -> String {
        self.doc.to_string()
    }

    /// The edited config, validated.
    pub fn config(&self) -> Result<Config, ConfigError> {
        Config::parse(&self.to_text(), Path::new(crate::CONFIG_FILE))
    }

    /// Apply an edit; if it fails or leaves an invalid config, the document is unchanged.
    fn transact(&mut self, f: impl FnOnce(&mut Self) -> Result<(), ConfigError>) -> Result<(), ConfigError> {
        let backup = self.doc.clone();
        let result = f(self).and_then(|()| self.config().map(|_| ()));
        if result.is_err() {
            self.doc = backup;
        }
        result
    }

    fn repos_mut(&mut self) -> Result<&mut ArrayOfTables, ConfigError> {
        if self.doc.get("repo").is_none() {
            self.doc.insert("repo", Item::ArrayOfTables(ArrayOfTables::new()));
        }
        self.doc["repo"]
            .as_array_of_tables_mut()
            .ok_or_else(|| invalid("`repo` must be an array of tables ([[repo]])"))
    }

    fn position(&self, name: &str) -> Option<usize> {
        self.doc
            .get("repo")?
            .as_array_of_tables()?
            .iter()
            .position(|t| repo_of(t).is_some_and(|r| r.display_name() == name))
    }

    pub fn add_repo(&mut self, repo: &RepoConfig) -> Result<(), ConfigError> {
        self.transact(|d| d.add_repo_raw(repo))
    }

    fn add_repo_raw(&mut self, repo: &RepoConfig) -> Result<(), ConfigError> {
        let name = repo.display_name();
        if self.position(&name).is_some() {
            return Err(invalid(format!("a repo named `{name}` already exists")));
        }
        let mut table = to_table(repo)?;
        // Keep the new [[repo]] next to the others rather than after the last table.
        let after = self
            .doc
            .get("repo")
            .and_then(Item::as_array_of_tables)
            .and_then(|a| a.iter().filter_map(Table::position).max());
        if let Some(p) = after {
            table.set_position(Some(p));
        }
        self.repos_mut()?.push(table);
        Ok(())
    }

    pub fn remove_repo(&mut self, name: &str) -> Result<(), ConfigError> {
        self.transact(|d| {
            let i = d
                .position(name)
                .ok_or_else(|| invalid(format!("no repo named `{name}`")))?;
            d.repos_mut()?.remove(i);
            Ok(())
        })
    }

    /// Set (`Some`) or remove (`None`) one string key of a repo.
    pub fn set_repo_key(&mut self, name: &str, key: &str, value: Option<&str>) -> Result<(), ConfigError> {
        self.transact(|d| {
            let i = d
                .position(name)
                .ok_or_else(|| invalid(format!("no repo named `{name}`")))?;
            let t = d.repos_mut()?.get_mut(i).expect("position is valid");
            match value {
                Some(v) => {
                    t.insert(key, toml_edit::value(v));
                }
                None => {
                    t.remove(key);
                }
            }
            Ok(())
        })
    }

    /// Replace `[people]`.
    pub fn set_people(&mut self, people: &People) -> Result<(), ConfigError> {
        self.transact(|d| d.set_people_raw(people))
    }

    fn set_people_raw(&mut self, people: &People) -> Result<(), ConfigError> {
        let mut table = to_table(people)?;
        match self.doc.get_mut("people").and_then(Item::as_table_mut) {
            Some(existing) => {
                // Keep the table's place and its leading comment.
                table.set_position(existing.position());
                *table.decor_mut() = existing.decor().clone();
                *existing = table;
            }
            None => {
                self.doc.insert("people", Item::Table(table));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RepoRole;
    use pm_classify::Role;

    const BASE: &str = r#"# Acme config (fictional)
[project]
name = "Acme"
range_start = "2025-01-01"
breadth_roles = ["backend", "frontend"]

[[repo]]
path = "~/src/api"   # the API
role = "backend"

# people below
[people]
"Jane Doe" = ["jane@example.com"]
"#;

    #[test]
    fn add_set_remove_repo_keeps_comments() {
        let mut d = ConfigDoc::parse(BASE).unwrap();
        d.add_repo(&RepoConfig {
            path: Some("~/src/web".into()),
            role: RepoRole::Fixed(Role::Frontend),
            branch: Some("origin/main".into()),
            ..Default::default()
        })
        .unwrap();
        let text = d.to_text();
        assert!(text.contains("# the API"), "{text}");
        assert!(text.contains("# people below"));
        assert!(
            text.find("~/src/web").unwrap() < text.find("[people]").unwrap(),
            "new repo should sit with the others:\n{text}"
        );
        assert_eq!(d.config().unwrap().repos.len(), 2);
        assert!(
            d.add_repo(&RepoConfig {
                path: Some("x/web".into()),
                ..Default::default()
            })
            .is_err()
        );

        d.set_repo_key("web", "role", Some("per-file")).unwrap();
        d.set_repo_key("web", "branch", None).unwrap();
        let c = d.config().unwrap();
        assert_eq!(c.repos[1].role, RepoRole::PerFile);
        assert_eq!(c.repos[1].branch, None);
        assert!(d.set_repo_key("web", "role", Some("chef")).is_err());
        assert_eq!(
            d.config().unwrap().repos[1].role,
            RepoRole::PerFile,
            "failed edit must not stick"
        );

        d.remove_repo("api").unwrap();
        assert_eq!(d.config().unwrap().repos[0].display_name(), "web");
        assert!(d.remove_repo("nope").is_err());
    }

    #[test]
    fn replace_people() {
        let mut d = ConfigDoc::parse(BASE).unwrap();
        let mut p = People::default();
        p.persons.insert(
            "Jane Doe".into(),
            vec!["jane@example.com".into(), "jd@home.example".into()],
        );
        p.persons.insert("Bob Builder".into(), vec!["bob@example.com".into()]);
        p.bots = vec!["ci@example.com".into()];
        d.set_people(&p).unwrap();
        let text = d.to_text();
        assert!(text.contains("# people below"), "{text}");
        assert_eq!(d.config().unwrap().people, p);
    }
}
