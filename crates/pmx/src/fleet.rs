//! The local fleet (plan §3.4, M6): `fleet.toml` lists project workspaces; pmx reads each one's
//! `out/project.json` (never `out/private/`), pools them into `fleet.json` and renders one
//! self-contained page with the fleet view and every project's dashboard (plan D33).

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
}

impl Fleet {
    /// Read the manifest and every project's `out/project.json`.
    pub fn load(manifest: &Path, generated_at: String) -> Result<Fleet> {
        let m = FleetManifest::load(manifest)?;
        let base = manifest.parent().unwrap_or(Path::new("."));
        let mut parsed = Vec::new();
        let mut raw = BTreeMap::new();
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
            raw.insert(e.key, text);
        }
        let Some(file) = build_fleet(&m.name, &parsed, generated_at) else {
            bail!("{}: no projects", manifest.display());
        };
        Ok(Fleet {
            manifest: manifest.to_path_buf(),
            file,
            projects: raw,
        })
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

    /// The fleet page: `fleet.json`, every `project.json` and the engine. No `leads.json`: the
    /// fleet page is for sharing.
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
        Ok(format!(
            "{{\"fleet\":{},\"projects\":{{{projects}}},\"leads\":null,\"wasm\":\"{wasm}\"}}",
            script_safe(&serde_json::to_string(&self.file)?),
        ))
    }
}
