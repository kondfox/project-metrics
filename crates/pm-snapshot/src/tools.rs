//! Finding the external tools (spec §10.5) and their versions.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolId {
    Scc,
    Jscpd,
    OsvScanner,
    Gitleaks,
    Semgrep,
}

impl ToolId {
    pub const ALL: [ToolId; 5] = [
        ToolId::Scc,
        ToolId::Jscpd,
        ToolId::OsvScanner,
        ToolId::Gitleaks,
        ToolId::Semgrep,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ToolId::Scc => "scc",
            ToolId::Jscpd => "jscpd",
            ToolId::OsvScanner => "osv-scanner",
            ToolId::Gitleaks => "gitleaks",
            ToolId::Semgrep => "semgrep",
        }
    }

    /// The version results are pinned to; other versions work but may count differently.
    pub fn pinned(self) -> &'static str {
        match self {
            ToolId::Scc => "4.1.0",
            ToolId::Jscpd => "4.3.0",
            ToolId::OsvScanner => "2.5.1",
            ToolId::Gitleaks => "8.30.1",
            ToolId::Semgrep => "1.179.0",
        }
    }

    /// What the dashboard loses without it.
    pub fn used_for(self) -> &'static str {
        match self {
            ToolId::Scc => "complexity / KLOC, hotspots, SAST density",
            ToolId::Jscpd => "duplication (the 4th Quality constituent)",
            ToolId::OsvScanner => "dependency vulnerabilities (Security score)",
            ToolId::Gitleaks => "live secrets (the Security gate)",
            ToolId::Semgrep => "own-code SAST trend",
        }
    }

    pub fn install_hint(self) -> &'static str {
        match self {
            ToolId::Scc | ToolId::OsvScanner | ToolId::Gitleaks => "`pmx tools install`",
            ToolId::Jscpd => "install Node (runs `npx jscpd@4.3.0`)",
            ToolId::Semgrep => "`pipx install semgrep==1.179.0` (or `brew install semgrep`)",
        }
    }
}

/// A tool ready to run: a program plus fixed leading arguments (`npx --yes jscpd@…`).
#[derive(Clone, Debug)]
pub struct Tool {
    pub id: ToolId,
    pub program: PathBuf,
    pub prefix: Vec<String>,
    pub version: String,
}

impl Tool {
    pub fn command(&self) -> Command {
        let mut c = Command::new(&self.program);
        c.args(&self.prefix);
        c
    }

    pub fn is_pinned_version(&self) -> bool {
        self.version == self.id.pinned()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Tools {
    pub scc: Option<Tool>,
    pub jscpd: Option<Tool>,
    pub osv: Option<Tool>,
    pub gitleaks: Option<Tool>,
    pub semgrep: Option<Tool>,
}

impl Tools {
    pub fn get(&self, id: ToolId) -> Option<&Tool> {
        match id {
            ToolId::Scc => self.scc.as_ref(),
            ToolId::Jscpd => self.jscpd.as_ref(),
            ToolId::OsvScanner => self.osv.as_ref(),
            ToolId::Gitleaks => self.gitleaks.as_ref(),
            ToolId::Semgrep => self.semgrep.as_ref(),
        }
    }

    /// Look in `extra` (pmx's own tools folder) first, then on PATH.
    pub fn detect(extra: &[PathBuf]) -> Tools {
        let find = |id: ToolId, version_args: &[&str]| -> Option<Tool> {
            let program = find_program(id.name(), extra)?;
            let version = version_of(&program, version_args)?;
            Some(Tool {
                id,
                program,
                prefix: Vec::new(),
                version,
            })
        };
        let jscpd = find_program("npx", extra).map(|npx| Tool {
            id: ToolId::Jscpd,
            program: npx,
            prefix: vec!["--yes".into(), format!("jscpd@{}", ToolId::Jscpd.pinned())],
            version: ToolId::Jscpd.pinned().into(),
        });
        Tools {
            scc: find(ToolId::Scc, &["--version"]),
            jscpd,
            osv: find(ToolId::OsvScanner, &["--version"]),
            gitleaks: find(ToolId::Gitleaks, &["version"]),
            semgrep: find(ToolId::Semgrep, &["--version"]),
        }
    }
}

fn find_program(name: &str, extra: &[PathBuf]) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let path_dirs = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    extra
        .iter()
        .chain(path_dirs.iter())
        .map(|d| d.join(&exe))
        .find(|p| p.is_file())
        .or_else(|| cfg!(windows).then(|| find_cmd(name, &path_dirs)).flatten())
}

fn find_cmd(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter().map(|d| d.join(format!("{name}.cmd"))).find(|p| p.is_file())
}

/// The last version-looking token of the first output line: `scc version 4.1.0` → `4.1.0`.
pub fn parse_version(output: &str) -> Option<String> {
    output
        .lines()
        .find(|l| !l.trim().is_empty())?
        .split_whitespace()
        .rev()
        .map(|t| t.trim_start_matches('v').trim_end_matches([',', ')']))
        .find(|t| {
            t.split('.').count() >= 2
                && t.split('.')
                    .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        })
        .map(String::from)
}

fn version_of(program: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    parse_version(&text).or_else(|| Some("unknown".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("scc version 4.1.0\n").as_deref(), Some("4.1.0"));
        assert_eq!(
            parse_version("osv-scanner version: 2.5.1\ncommit: x").as_deref(),
            Some("2.5.1")
        );
        assert_eq!(parse_version("1.179.0\n").as_deref(), Some("1.179.0"));
        assert_eq!(parse_version("gitleaks version v8.30.1").as_deref(), Some("8.30.1"));
        assert_eq!(parse_version("no version here"), None);
    }
}
