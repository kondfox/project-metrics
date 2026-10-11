//! Code-host parity (parity.md §1.2): the golden set's recorded GitLab responses are served to
//! pmx's GitLab adapter by request path, and the GitHub `gh pr list` export is read by pmx's gh
//! reader. Both feed the normal collect pipeline.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use pm_codehost::{Http, Response};
use pm_config::{CodeHost, CodeHostId, CodeHostKind, Config, Identity};
use pmx::PrSource;
use serde_json::Value;

/// Serves `inputs/codehost/<project>/index.json` (request path after `/api/v4` → file, headers).
struct Replay {
    dir: PathBuf,
    index: HashMap<String, (String, BTreeMap<String, String>)>,
}

impl Http for Replay {
    fn get(&self, url: &str, _: &[(&str, &str)]) -> Result<Response, String> {
        let path = url.split_once("/api/v4").map_or(url, |(_, p)| p);
        let Some((file, headers)) = self.index.get(path) else {
            return Ok(Response {
                status: 404,
                body: format!("not recorded: {path}"),
                ..Default::default()
            });
        };
        let body = std::fs::read_to_string(self.dir.join(file)).map_err(|e| e.to_string())?;
        Ok(Response {
            status: 200,
            headers: headers.clone(),
            body,
        })
    }

    fn post(&self, url: &str, _: &[(&str, &str)], _: &str) -> Result<Response, String> {
        Err(format!("nothing recorded for POST {url}"))
    }
}

/// Golden code-host data of a project.
pub struct Golden {
    pub source: PrSource,
    /// Months with PRs whose author the spec excludes (bots, externals; the prototype kept them).
    pub excluded_months: BTreeMap<String, u32>,
}

fn excluded_months(config: &Config, authors: &[(String, Option<String>, bool, String)]) -> BTreeMap<String, u32> {
    let id = Identity::new(&config.people);
    let mut out = BTreeMap::new();
    for (login, name, bot, month) in authors {
        if id.resolve_login(login, name.as_deref(), *bot).excluded.is_some() {
            *out.entry(month.clone()).or_default() += 1;
        }
    }
    out
}

/// The PR source for a project, and the config changes it needs. `None` = no code-host data
/// in the golden set.
pub fn source(golden: &Path, key: &str, config: &mut Config) -> Result<Option<Golden>> {
    let dir = golden.join("inputs/codehost").join(key);
    let index_path = dir.join("index.json");
    if index_path.exists() {
        let raw: BTreeMap<String, Value> = serde_json::from_str(&std::fs::read_to_string(&index_path)?)?;
        let index = raw
            .into_iter()
            .map(|(path, v)| {
                let headers = v["headers"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .filter_map(|(k, v)| Some((k.to_lowercase(), v.as_str()?.to_string())))
                    .collect();
                (path, (v["file"].as_str().unwrap_or_default().to_string(), headers))
            })
            .collect::<HashMap<_, _>>();
        let mut authors = Vec::new();
        for (path, (file, _)) in &index {
            if path.contains("/merge_requests?") {
                let mrs: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(file))?)?;
                for m in mrs.as_array().into_iter().flatten() {
                    if let (Some(login), Some(merged)) = (m["author"]["username"].as_str(), m["merged_at"].as_str()) {
                        let name = m["author"]["name"].as_str().map(String::from);
                        authors.push((login.to_string(), name, false, merged[..7].to_string()));
                    }
                }
            }
        }
        return Ok(Some(Golden {
            source: PrSource::Http(Arc::new(Replay { dir, index })),
            excluded_months: excluded_months(config, &authors),
        }));
    }
    let export = dir.join("pr.json");
    if export.exists() {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&export)?).context("gh export")?;
        let prs = pm_codehost::github::from_gh_export(&v)?;
        // The prototype's GitHub layer covered the project's one repo.
        let repo = config.repos.first_mut().context("no repo")?;
        repo.code_host_id = Some(CodeHostId::Name("golden/export".into()));
        config.code_host = Some(CodeHost {
            kind: CodeHostKind::Github,
            base: None,
            group: None,
            token_env: None,
        });
        let name = repo.display_name();
        let authors: Vec<_> = prs
            .iter()
            .filter_map(|p| {
                let a = p.author.as_ref()?;
                Some((
                    a.login.clone(),
                    a.name.clone(),
                    a.bot,
                    p.merged_at.format("%Y-%m").to_string(),
                ))
            })
            .collect();
        return Ok(Some(Golden {
            source: PrSource::Fixed([(name, prs)].into()),
            excluded_months: excluded_months(config, &authors),
        }));
    }
    Ok(None)
}
