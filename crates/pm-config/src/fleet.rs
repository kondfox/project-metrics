//! The local fleet manifest, `fleet.toml` (plan §3.4, D2): a name and the project workspaces it
//! combines.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{ConfigError, expand_path};

pub const FLEET_FILE: &str = "fleet.toml";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetManifest {
    pub name: String,
    /// Project workspace folders (each holds `pmx.toml` and `out/project.json`), relative to the
    /// manifest. Strings for now; a URL entry is a later extension (remote fleet storage, D2).
    pub projects: Vec<String>,
}

/// One project of a loaded manifest.
#[derive(Clone, Debug, PartialEq)]
pub struct FleetEntry {
    /// Unique in the fleet: the folder name, with `-2`, `-3`, … for repeats.
    pub key: String,
    pub dir: PathBuf,
}

impl FleetManifest {
    pub fn parse(text: &str, path: &Path) -> Result<FleetManifest, ConfigError> {
        let m: FleetManifest = toml::from_str(text).map_err(|e| ConfigError::Parse {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
        if m.projects.is_empty() {
            return Err(ConfigError::Invalid(format!("{}: `projects` is empty", path.display())));
        }
        Ok(m)
    }

    pub fn load(path: &Path) -> Result<FleetManifest, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        FleetManifest::parse(&text, path)
    }

    /// The projects' folders, resolved against `base` (the manifest's folder), with their keys.
    pub fn entries(&self, base: &Path) -> Vec<FleetEntry> {
        let mut taken = BTreeSet::new();
        self.projects
            .iter()
            .map(|p| {
                let dir = expand_path(p, base);
                let stem = dir
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .filter(|s| !s.is_empty() && s != "." && s != "..")
                    .unwrap_or_else(|| "project".into());
                let mut key = stem.clone();
                let mut i = 2;
                while !taken.insert(key.clone()) {
                    key = format!("{stem}-{i}");
                    i += 1;
                }
                FleetEntry { key, dir }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_keys_projects() {
        let m = FleetManifest::parse(
            "name = \"Acme Engineering\"\nprojects = [\"shop\", \"crm\", \"../other/shop\"]\n",
            Path::new("fleet.toml"),
        )
        .unwrap();
        let e = m.entries(Path::new("/m"));
        assert_eq!(
            e.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(),
            ["shop", "crm", "shop-2"]
        );
        assert_eq!(e[0].dir, PathBuf::from("/m/shop"));
        assert!(FleetManifest::parse("name = \"x\"\nprojects = []\n", Path::new("f")).is_err());
        assert!(FleetManifest::parse("name = \"x\"\nprojects = [\"a\"]\nextra = 1\n", Path::new("f")).is_err());
    }

    #[test]
    fn example_parses() {
        let m = FleetManifest::parse(include_str!("../../../examples/fleet.toml"), Path::new("fleet.toml")).unwrap();
        assert_eq!(m.projects.len(), 3);
    }
}
