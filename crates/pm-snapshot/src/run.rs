//! Running each tool on a materialized tree, and parsing its output. Parsers are separate pure
//! functions so they are tested against sample output.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::LazyLock;

use pm_metrics::snapshot::Severity;
use regex::Regex;
use serde_json::Value;

use crate::raw::{Duplication, LockfileScan, OsvPackage, SastFinding, SccTotals, SecretHit};
use crate::tools::Tool;
use crate::{Result, SnapshotError};

/// Excluded from complexity, duplication and hotspots (spec §3.4).
pub const EXCLUDE_DIRS: &[&str] = &[
    "node_modules",
    "dist",
    "build",
    ".next",
    "out",
    "coverage",
    "vendor",
    "Pods",
    ".gradle",
    ".git",
    "generated",
    "__generated__",
    ".venv",
    "venv",
    "__pycache__",
    ".turbo",
    "bin",
    "obj",
];

fn output(tool: &Tool, cmd: &mut Command) -> Result<Output> {
    cmd.stdin(Stdio::null())
        .output()
        .map_err(|source| SnapshotError::Spawn {
            tool: tool.id.name().into(),
            source,
        })
}

fn parse_err(tool: &Tool, message: impl Into<String>) -> SnapshotError {
    SnapshotError::Parse {
        tool: tool.id.name().into(),
        message: message.into(),
    }
}

// ---------- scc ----------

/// `scc --format json` totals over every language.
pub fn parse_scc_totals(json: &str) -> std::result::Result<SccTotals, String> {
    let langs: Vec<Value> =
        serde_json::from_str(if json.trim().is_empty() { "[]" } else { json }).map_err(|e| e.to_string())?;
    Ok(langs.iter().fold(SccTotals::default(), |t, l| SccTotals {
        code: t.code + l["Code"].as_u64().unwrap_or(0),
        complexity: t.complexity + l["Complexity"].as_u64().unwrap_or(0),
    }))
}

pub fn scc_totals(tool: &Tool, tree: &Path, excludes: bool) -> Result<SccTotals> {
    let mut c = tool.command();
    c.args(["--format", "json"]);
    if excludes {
        c.args(["--exclude-dir", &EXCLUDE_DIRS.join(",")]);
    }
    let out = output(tool, c.arg(tree))?;
    if !out.status.success() {
        return Err(SnapshotError::Failed {
            tool: "scc".into(),
            message: String::from_utf8_lossy(&out.stderr).trim().into(),
        });
    }
    parse_scc_totals(&String::from_utf8_lossy(&out.stdout)).map_err(|m| parse_err(tool, m))
}

/// `scc --by-file`: tree-relative path → (code, complexity).
pub fn parse_scc_by_file(json: &str) -> std::result::Result<Vec<(String, u64, u64)>, String> {
    let langs: Vec<Value> =
        serde_json::from_str(if json.trim().is_empty() { "[]" } else { json }).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for l in &langs {
        for f in l["Files"].as_array().into_iter().flatten() {
            let loc = f["Location"].as_str().or(f["Filename"].as_str()).unwrap_or_default();
            let loc = loc.strip_prefix("./").unwrap_or(loc);
            if loc.is_empty() || loc.split('/').any(|seg| EXCLUDE_DIRS.contains(&seg)) {
                continue;
            }
            out.push((
                loc.to_string(),
                f["Code"].as_u64().unwrap_or(0),
                f["Complexity"].as_u64().unwrap_or(0),
            ));
        }
    }
    Ok(out)
}

pub fn scc_by_file(tool: &Tool, tree: &Path) -> Result<Vec<(String, u64, u64)>> {
    let mut c = tool.command();
    c.args([
        "--by-file",
        "--format",
        "json",
        "--exclude-dir",
        &EXCLUDE_DIRS.join(","),
        ".",
    ])
    .current_dir(tree);
    let out = output(tool, &mut c)?;
    parse_scc_by_file(&String::from_utf8_lossy(&out.stdout)).map_err(|m| parse_err(tool, m))
}

// ---------- jscpd ----------

pub fn jscpd_ignore() -> String {
    let mut globs: Vec<String> = EXCLUDE_DIRS.iter().map(|d| format!("**/{d}/**")).collect();
    globs.extend(
        [
            "**/*.min.js",
            "**/*.map",
            "**/*.lock",
            "**/*-lock.json",
            "**/package-lock.json",
            "**/yarn.lock",
            "**/pnpm-lock.yaml",
            "**/Podfile.lock",
            "**/*.snap",
            "**/*.svg",
        ]
        .map(String::from),
    );
    globs.join(",")
}

pub fn parse_jscpd(json: &str) -> std::result::Result<Duplication, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let total = &v["statistics"]["total"];
    Ok(Duplication {
        duplicated_lines: total["duplicatedLines"].as_u64().unwrap_or(0),
        lines: total["lines"].as_u64().unwrap_or(0),
    })
}

/// `jscpd --min-lines 5 --min-tokens 50` with the excludes and the repo's .gitignore (spec §3.4).
pub fn jscpd(tool: &Tool, tree: &Path) -> Result<Duplication> {
    let out_dir = tempfile::tempdir()?;
    let mut c = tool.command();
    c.arg(tree)
        .args(["--reporters", "json", "--output"])
        .arg(out_dir.path())
        .args([
            "--silent",
            "--gitignore",
            "--ignore",
            &jscpd_ignore(),
            "--min-lines",
            "5",
            "--min-tokens",
            "50",
        ]);
    let out = output(tool, &mut c)?;
    let report = out_dir.path().join("jscpd-report.json");
    match std::fs::read_to_string(&report) {
        Ok(text) => parse_jscpd(&text).map_err(|m| parse_err(tool, m)),
        // jscpd writes no report when it finds no files to scan.
        Err(_) if out.status.success() => Ok(Duplication::default()),
        Err(_) => Err(SnapshotError::Failed {
            tool: "jscpd".into(),
            message: String::from_utf8_lossy(&out.stderr).trim().chars().take(400).collect(),
        }),
    }
}

// ---------- gitleaks ----------

/// Strip the tree prefix from a reported path (tools may report it canonicalized).
fn relative(path: &str, trees: &[String]) -> String {
    for t in trees {
        if let Some(rest) = path.strip_prefix(t.as_str()) {
            return rest.trim_start_matches('/').to_string();
        }
    }
    path.to_string()
}

fn tree_prefixes(tree: &Path) -> Vec<String> {
    let mut v = vec![tree.display().to_string()];
    if let Ok(c) = std::fs::canonicalize(tree) {
        v.push(c.display().to_string());
    }
    v
}

pub fn parse_gitleaks(json: &str, trees: &[String]) -> std::result::Result<Vec<SecretHit>, String> {
    let hits: Vec<Value> =
        serde_json::from_str(if json.trim().is_empty() { "[]" } else { json }).map_err(|e| e.to_string())?;
    Ok(hits
        .iter()
        .map(|h| SecretHit {
            rule: h["RuleID"].as_str().unwrap_or_default().to_string(),
            file: relative(h["File"].as_str().unwrap_or_default(), trees),
            line: h["StartLine"].as_u64().unwrap_or(0),
        })
        .collect())
}

/// `gitleaks dir --redact` on the tree (spec §7.2). Secret values never reach the result.
pub fn gitleaks(tool: &Tool, tree: &Path) -> Result<Vec<SecretHit>> {
    let report = tempfile::NamedTempFile::new()?;
    let mut c = tool.command();
    c.arg("dir")
        .arg(tree)
        .args(["--redact", "--no-banner", "--exit-code", "0", "-f", "json", "-r"])
        .arg(report.path());
    let out = output(tool, &mut c)?;
    if !out.status.success() {
        return Err(SnapshotError::Failed {
            tool: "gitleaks".into(),
            message: String::from_utf8_lossy(&out.stderr).trim().chars().take(400).collect(),
        });
    }
    let text = std::fs::read_to_string(report.path())?;
    parse_gitleaks(&text, &tree_prefixes(tree)).map_err(|m| parse_err(tool, m))
}

// ---------- semgrep ----------

pub struct SemgrepScan {
    pub findings: Vec<SastFinding>,
    /// Rules that timed out: the scan is incomplete (spec §7.4).
    pub timeouts: u64,
}

pub fn parse_semgrep(json: &str, trees: &[String]) -> std::result::Result<SemgrepScan, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let errors = v["errors"].as_array().cloned().unwrap_or_default();
    let fatal: Vec<String> = errors
        .iter()
        .filter_map(|e| e["message"].as_str())
        .filter(|m| m.contains("invalid configuration") || m.contains("Failed to download configuration"))
        .map(String::from)
        .collect();
    if !fatal.is_empty() {
        // An invalid ruleset aborts the scan: it would read as "0 findings".
        return Err(format!("semgrep config error: {}", fatal.join("; ")));
    }
    let findings = v["results"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| SastFinding {
            rule: r["check_id"]
                .as_str()
                .unwrap_or_default()
                .rsplit('.')
                .next()
                .unwrap_or_default()
                .to_string(),
            severity: r["extra"]["severity"].as_str().unwrap_or_default().to_string(),
            file: relative(r["path"].as_str().unwrap_or_default(), trees),
            line: r["start"]["line"].as_u64().unwrap_or(0),
            cwe: r["extra"]["metadata"]["cwe"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|c| c.as_str())
                .map(String::from),
        })
        .collect();
    let timeouts = errors
        .iter()
        .filter(|e| e["message"].as_str().is_some_and(|m| m.contains("Timeout")))
        .count() as u64;
    Ok(SemgrepScan { findings, timeouts })
}

/// The rule files of the configured rule paths (files, or folders of `*.yaml`/`*.yml`), sorted.
pub fn rule_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                out.extend(
                    rd.flatten()
                        .map(|e| e.path())
                        .filter(|f| f.extension().is_some_and(|e| e == "yaml" || e == "yml")),
                );
            }
        } else if p.is_file() {
            out.push(p.clone());
        }
    }
    out.sort();
    out
}

/// `semgrep scan` with the pinned local rules. Retried once on rule timeouts; still incomplete →
/// `Ok(None)` (not measured rather than a lower count, spec §7.4).
pub fn semgrep(tool: &Tool, tree: &Path, rules: &[PathBuf]) -> Result<Option<Vec<SastFinding>>> {
    if rules.is_empty() {
        return Ok(None);
    }
    for _attempt in 0..2 {
        let mut c = tool.command();
        c.args(["scan", "--metrics=off", "--json", "--quiet"]);
        for r in rules {
            c.arg("--config").arg(r);
        }
        let out = output(tool, c.arg(tree))?;
        let scan = parse_semgrep(&String::from_utf8_lossy(&out.stdout), &tree_prefixes(tree))
            .map_err(|m| parse_err(tool, m))?;
        if scan.timeouts == 0 {
            return Ok(Some(scan.findings));
        }
    }
    Ok(None)
}

// ---------- suppression markers ----------

static SUPPRESS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"nosemgrep|#\s*nosec\b|NOSONAR|gitleaks:allow|eslint-disable[^\n]*security|@SuppressWarnings\("?squid"#,
    )
    .unwrap()
});

pub fn count_suppressions(text: &str) -> u64 {
    SUPPRESS.find_iter(text).count() as u64
}

/// Suppression markers in every file of the tree (spec §7.4).
pub fn suppressions(tree: &Path) -> u64 {
    walkdir::WalkDir::new(tree)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| std::fs::read(e.path()).ok())
        .map(|b| count_suppressions(&String::from_utf8_lossy(&b)))
        .sum()
}

// ---------- osv-scanner ----------

/// Lockfile names osv-scanner understands (auto-discovery, spec §10.1).
pub const LOCKFILES: &[&str] = &[
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lock",
    "Cargo.lock",
    "go.mod",
    "composer.lock",
    "Gemfile.lock",
    "gems.locked",
    "poetry.lock",
    "Pipfile.lock",
    "pdm.lock",
    "uv.lock",
    "requirements.txt",
    "gradle.lockfile",
    "buildscript-gradle.lockfile",
    "pom.xml",
    "packages.lock.json",
    "pubspec.lock",
    "mix.lock",
    "renv.lock",
    "conan.lock",
];

/// Lockfiles in the tree, outside dependency and build folders.
pub fn discover_lockfiles(tree: &Path) -> Vec<String> {
    let mut out: Vec<String> = walkdir::WalkDir::new(tree)
        .into_iter()
        .filter_entry(|e| !(e.file_type().is_dir() && EXCLUDE_DIRS.contains(&e.file_name().to_string_lossy().as_ref())))
        .flatten()
        .filter(|e| e.file_type().is_file() && LOCKFILES.contains(&e.file_name().to_string_lossy().as_ref()))
        .filter_map(|e| {
            e.path()
                .strip_prefix(tree)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
        })
        .collect();
    out.sort();
    out
}

fn severity_word(s: &str) -> Severity {
    match s.to_uppercase().as_str() {
        "CRITICAL" => Severity::Critical,
        "HIGH" => Severity::High,
        "MODERATE" | "MEDIUM" => Severity::Moderate,
        "LOW" => Severity::Low,
        _ => Severity::Unknown,
    }
}

/// From the group's CVSS `max_severity`: ≥ 9 critical, ≥ 7 high, ≥ 4 moderate, else low.
fn severity_score(score: Option<f64>) -> Severity {
    match score {
        None => Severity::Unknown,
        Some(s) if s >= 9.0 => Severity::Critical,
        Some(s) if s >= 7.0 => Severity::High,
        Some(s) if s >= 4.0 => Severity::Moderate,
        Some(_) => Severity::Low,
    }
}

/// osv-scanner v2 JSON → package entries with advisory severities (spec §7.1).
pub fn parse_osv(json: &str) -> std::result::Result<Vec<OsvPackage>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for res in v["results"].as_array().into_iter().flatten() {
        for p in res["packages"].as_array().into_iter().flatten() {
            let mut group_sev = std::collections::HashMap::new();
            for g in p["groups"].as_array().into_iter().flatten() {
                let score = match &g["max_severity"] {
                    Value::String(s) => s.parse::<f64>().ok(),
                    Value::Number(n) => n.as_f64(),
                    _ => None,
                };
                for id in g["ids"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                    group_sev.insert(id.to_string(), score);
                }
            }
            let vulns = p["vulnerabilities"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|vuln| {
                    let id = vuln["id"].as_str().unwrap_or_default().to_string();
                    let sev = match vuln["database_specific"]["severity"].as_str() {
                        Some(s) if !s.is_empty() => severity_word(s),
                        _ => severity_score(group_sev.get(&id).copied().flatten()),
                    };
                    (id, sev)
                })
                .collect();
            out.push(OsvPackage {
                name: p["package"]["name"].as_str().unwrap_or_default().to_string(),
                version: p["package"]["version"].as_str().unwrap_or_default().to_string(),
                vulns,
            });
        }
    }
    Ok(out)
}

/// Scan one lockfile's content (written under its own file name, which tells osv-scanner the
/// format). osv-scanner exits non-zero when it finds vulnerabilities; the JSON decides.
pub fn osv(tool: &Tool, name: &str, content: &[u8]) -> Result<String> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join(Path::new(name).file_name().unwrap_or_default());
    std::fs::write(&file, content)?;
    let mut c = tool.command();
    c.args(["scan", "source", "--lockfile"])
        .arg(&file)
        .args(["--format", "json"]);
    let out = output(tool, &mut c)?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if serde_json::from_str::<Value>(&text).is_err() {
        return Err(SnapshotError::Failed {
            tool: "osv-scanner".into(),
            message: String::from_utf8_lossy(&out.stderr).trim().chars().take(400).collect(),
        });
    }
    Ok(text)
}

pub fn lockfile_scan(path: &str, osv_json: &str) -> std::result::Result<LockfileScan, String> {
    Ok(LockfileScan {
        path: path.to_string(),
        packages: parse_osv(osv_json)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scc() {
        let json = r#"[{"Name":"Go","Code":800,"Complexity":40,"Files":[{"Location":"./internal/a.go","Code":500,"Complexity":30},{"Location":"vendor/x.go","Code":5,"Complexity":1}]},{"Name":"YAML","Code":200,"Complexity":0,"Files":[]}]"#;
        assert_eq!(
            parse_scc_totals(json).unwrap(),
            SccTotals {
                code: 1000,
                complexity: 40
            }
        );
        assert_eq!(parse_scc_totals("").unwrap(), SccTotals::default());
        assert_eq!(
            parse_scc_by_file(json).unwrap(),
            vec![("internal/a.go".to_string(), 500, 30)]
        );
    }

    #[test]
    fn jscpd_report() {
        let json = r#"{"statistics":{"total":{"lines":10000,"duplicatedLines":321,"percentage":3.21}}}"#;
        assert_eq!(
            parse_jscpd(json).unwrap(),
            Duplication {
                duplicated_lines: 321,
                lines: 10000
            }
        );
        assert!(jscpd_ignore().contains("**/node_modules/**"));
    }

    #[test]
    fn gitleaks_report() {
        let trees = vec!["/tmp/t1".to_string(), "/private/tmp/t1".to_string()];
        let json = r#"[{"RuleID":"aws-access-token","File":"/private/tmp/t1/config/prod.env","StartLine":3,"Secret":"REDACTED"}]"#;
        assert_eq!(
            parse_gitleaks(json, &trees).unwrap(),
            vec![SecretHit {
                rule: "aws-access-token".into(),
                file: "config/prod.env".into(),
                line: 3
            }]
        );
    }

    #[test]
    fn semgrep_report() {
        let trees = vec!["/tmp/t".to_string()];
        let json = r#"{"version":"1.179.0","results":[{"check_id":"rules.js.detect-eval","path":"/tmp/t/src/a.js","start":{"line":7},"extra":{"severity":"ERROR","metadata":{"cwe":["CWE-95: Eval"]}}}],"errors":[{"message":"Timeout when running rule x"}]}"#;
        let s = parse_semgrep(json, &trees).unwrap();
        assert_eq!(s.timeouts, 1);
        assert_eq!(s.findings[0].rule, "detect-eval");
        assert_eq!(s.findings[0].file, "src/a.js");
        assert_eq!(s.findings[0].cwe.as_deref(), Some("CWE-95: Eval"));
        let bad = r#"{"results":[],"errors":[{"message":"invalid configuration file found"}]}"#;
        assert!(parse_semgrep(bad, &trees).is_err());
    }

    #[test]
    fn suppression_markers() {
        let text = "x = eval(y) // nosemgrep\nkey = 1  # nosec\n# nosecurity\n// eslint-disable-next-line security/detect-object-injection\n@SuppressWarnings(\"squid:S1234\")";
        assert_eq!(count_suppressions(text), 4);
    }

    #[test]
    fn osv_report() {
        let json = r#"{"results":[{"packages":[
            {"package":{"name":"lodash","version":"4.17.20"},
             "vulnerabilities":[{"id":"GHSA-a","database_specific":{"severity":"HIGH"}},{"id":"GHSA-b"},{"id":"GHSA-c"}],
             "groups":[{"ids":["GHSA-b"],"max_severity":"9.8"},{"ids":["GHSA-c"],"max_severity":""}]},
            {"package":{"name":"minimist","version":"1.2.0"},"vulnerabilities":[{"id":"GHSA-d","database_specific":{"severity":"MEDIUM"}}],"groups":[]}
        ]}]}"#;
        let p = parse_osv(json).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(
            p[0].vulns,
            vec![
                ("GHSA-a".to_string(), Severity::High),
                ("GHSA-b".to_string(), Severity::Critical),
                ("GHSA-c".to_string(), Severity::Unknown)
            ]
        );
        assert_eq!(p[1].vulns[0].1, Severity::Moderate);
    }

    #[test]
    fn lockfile_discovery() {
        let t = tempfile::tempdir().unwrap();
        for f in [
            "package-lock.json",
            "web/yarn.lock",
            "web/node_modules/x/package-lock.json",
            "api/go.mod",
            "README.md",
        ] {
            let p = t.path().join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x").unwrap();
        }
        assert_eq!(
            discover_lockfiles(t.path()),
            vec!["api/go.mod", "package-lock.json", "web/yarn.lock"]
        );
    }
}
