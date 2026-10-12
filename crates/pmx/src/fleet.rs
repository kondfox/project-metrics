//! The local fleet (plan §3.4, M6): `fleet.toml` lists project workspaces; pmx reads each one's
//! `out/project.json`, pools them into `fleet.json` and renders one self-contained page with the
//! fleet view and every project's dashboard (plan D33). Only the lead's own copy
//! ([`Fleet::with_private`]: `fleet serve`, `fleet export --with-private`) also reads each project's
//! `out/private/leads.json` (D40).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use pm_config::fleet::FleetManifest;
use pm_metrics::{FleetFile, ProjectFile, build_fleet};

use crate::dashboard::{engine_base64, inject, script_safe};

/// A manifest's projects, read from disk.
pub struct Fleet {
    pub manifest: PathBuf,
    pub file: FleetFile,
    /// key → `project.json` as written by `pmx collect`.
    pub projects: BTreeMap<String, String>,
    /// key → the project's workspace folder.
    pub dirs: BTreeMap<String, PathBuf>,
    /// key → `private/leads.json`; `None` for the shareable page.
    pub leads: Option<BTreeMap<String, String>>,
}

impl Fleet {
    /// Read the manifest and every project's `out/project.json`.
    pub fn load(manifest: &Path, generated_at: String) -> Result<Fleet> {
        let m = FleetManifest::load(manifest)?;
        let base = manifest.parent().unwrap_or(Path::new("."));
        let mut parsed = Vec::new();
        let mut raw = BTreeMap::new();
        let mut dirs = BTreeMap::new();
        for e in m.entries(base) {
            let path = e.dir.join("out").join("project.json");
            let text = std::fs::read_to_string(&path).with_context(|| {
                format!(
                    "reading {} (run `pmx -C {} collect` first)",
                    path.display(),
                    e.dir.display()
                )
            })?;
            let p: ProjectFile =
                serde_json::from_str(&text).with_context(|| format!("{} is not a pmx project file", path.display()))?;
            parsed.push((e.key.clone(), p));
            raw.insert(e.key.clone(), text);
            dirs.insert(e.key, e.dir);
        }
        let Some(file) = build_fleet(&m.name, &parsed, generated_at) else {
            bail!("{}: no projects", manifest.display());
        };
        Ok(Fleet {
            manifest: manifest.to_path_buf(),
            file,
            projects: raw,
            dirs,
            leads: None,
        })
    }

    /// Add each project's per-person lead view (`out/private/leads.json`; a project without one
    /// shows no people). The page then names people: never share it.
    pub fn with_private(mut self) -> Result<Fleet> {
        let mut leads = BTreeMap::new();
        for (key, dir) in &self.dirs {
            let path = dir.join("out").join("private").join("leads.json");
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            serde_json::from_str::<serde_json::Value>(&text)
                .with_context(|| format!("{} is not JSON", path.display()))?;
            leads.insert(key.clone(), text);
        }
        self.leads = Some(leads);
        Ok(self)
    }

    /// Projects collected before the fleet's as-of date, as `(name, as_of)`.
    pub fn stale(&self) -> Vec<(String, String)> {
        self.file
            .projects
            .iter()
            .filter(|p| p.stale)
            .map(|p| (p.name.clone(), p.as_of.to_string()))
            .collect()
    }

    pub fn json(&self) -> String {
        serde_json::to_string_pretty(&self.file).expect("fleet serializes")
    }

    /// The fleet page: `fleet.json`, every `project.json` and the engine; the lead views only
    /// after [`Fleet::with_private`].
    pub fn render(&self) -> Result<String> {
        inject(
            &self.page_data(&engine_base64())?,
            &format!("{} · pmx fleet", self.file.name),
        )
    }

    /// The JSON object the page reads (`wasm` is the engine in base64; empty for dev data).
    pub fn page_data(&self, wasm: &str) -> Result<String> {
        let projects = self
            .projects
            .iter()
            .map(|(k, text)| Ok(format!("{}:{}", serde_json::to_string(k)?, script_safe(text.trim()))))
            .collect::<Result<Vec<_>>>()?
            .join(",");
        let leads = match &self.leads {
            None => "null".to_string(),
            Some(l) => format!(
                "{{{}}}",
                l.iter()
                    .map(|(k, text)| Ok(format!("{}:{}", serde_json::to_string(k)?, script_safe(text.trim()))))
                    .collect::<Result<Vec<_>>>()?
                    .join(",")
            ),
        };
        Ok(format!(
            "{{\"fleet\":{},\"projects\":{{{projects}}},\"leads\":{leads},\"wasm\":\"{wasm}\"}}",
            script_safe(&serde_json::to_string(&self.file)?),
        ))
    }
}
