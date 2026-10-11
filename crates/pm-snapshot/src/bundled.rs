//! The bundled own-code SAST rules, `pmx-sast@1` (spec §7.4): original rules, MIT OR Apache-2.0,
//! in `rules/pmx-sast@1/` with semgrep tests beside them. Any change to a rule is a new version
//! (`@2`), because it moves the SAST trend.

use std::path::{Path, PathBuf};

pub const RULESET: &str = "pmx-sast@1";

/// (file name, contents) of every rule file.
pub const RULES: &[(&str, &str)] = &[
    ("csharp.yaml", include_str!("../rules/pmx-sast@1/csharp.yaml")),
    ("go.yaml", include_str!("../rules/pmx-sast@1/go.yaml")),
    ("java.yaml", include_str!("../rules/pmx-sast@1/java.yaml")),
    ("javascript.yaml", include_str!("../rules/pmx-sast@1/javascript.yaml")),
    ("kotlin.yaml", include_str!("../rules/pmx-sast@1/kotlin.yaml")),
    ("python.yaml", include_str!("../rules/pmx-sast@1/python.yaml")),
    ("react.yaml", include_str!("../rules/pmx-sast@1/react.yaml")),
    ("terraform.yaml", include_str!("../rules/pmx-sast@1/terraform.yaml")),
];

/// Number of rules (`- id:` entries) in the set.
pub fn rule_count() -> usize {
    RULES
        .iter()
        .map(|(_, text)| text.lines().filter(|l| l.trim_start().starts_with("- id:")).count())
        .sum()
}

/// Write the rules into `<dir>/pmx-sast@1/` (only files whose content differs) and return that
/// folder, for semgrep's `--config`.
pub fn materialize(dir: &Path) -> std::io::Result<PathBuf> {
    let out = dir.join(RULESET);
    std::fs::create_dir_all(&out)?;
    for (name, text) in RULES {
        let path = out.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(*text) {
            std::fs::write(&path, text)?;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rule_file_is_bundled() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("rules").join(RULESET);
        let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".yaml"))
            .collect();
        on_disk.sort();
        let bundled: Vec<&str> = RULES.iter().map(|(n, _)| *n).collect();
        assert_eq!(on_disk, bundled, "add new rule files to RULES");
        // Rule ids are used as the last segment of semgrep's check id: no dots, unique.
        let ids: Vec<&str> = RULES
            .iter()
            .flat_map(|(_, t)| t.lines())
            .filter_map(|l| l.trim_start().strip_prefix("- id:"))
            .map(str::trim)
            .collect();
        assert!(ids.iter().all(|id| id.starts_with("pmx-") && !id.contains('.')));
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), ids.len(), "duplicate rule id");
        assert_eq!(rule_count(), ids.len());
        // Every rule file says it is ours to license.
        assert!(RULES.iter().all(|(_, t)| t.contains("MIT OR Apache-2.0")));
    }

    #[test]
    fn materializes_once() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = materialize(tmp.path()).unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), RULES.len());
        let mtime = std::fs::metadata(dir.join("go.yaml")).unwrap().modified().unwrap();
        materialize(tmp.path()).unwrap();
        assert_eq!(
            std::fs::metadata(dir.join("go.yaml")).unwrap().modified().unwrap(),
            mtime
        );
    }
}
