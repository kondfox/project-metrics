//! `pmx check` (plan §3.2): every path exists, every revision resolves, tokens are present, the
//! data-policy guard holds, and no active identity is unmapped.

use pm_config::{Config, DataPolicy, FteSource, Provider, Workspace, expand_path};
use pm_git::{Git, LogRange, parse_version};

use crate::people::{Status, gather, propose};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Ok,
    Warn,
    Error,
}

#[derive(Clone, Debug)]
pub struct Finding {
    pub level: Level,
    pub message: String,
}

fn f(level: Level, message: impl Into<String>) -> Finding {
    Finding {
        level,
        message: message.into(),
    }
}

/// Follow a classifier and its fallbacks; error if any of them would send code off the machine
/// (plan §6.3).
pub fn local_only_violation(config: &Config, start: &str) -> Option<String> {
    let mut name = start.to_string();
    let mut seen = Vec::new();
    loop {
        if seen.contains(&name) {
            return Some(format!("classifier fallback loop at `{name}`"));
        }
        seen.push(name.clone());
        let Some(c) = config.classifiers.get(&name) else {
            return Some(format!("unknown classifier `{name}`"));
        };
        if !pm_llm::is_local(c) {
            return Some(format!("classifier `{name}` ({}) is not local", c.provider.as_str()));
        }
        name = c.fallback.clone()?;
    }
}

/// Can each classifier run? (Providers not built yet, a missing `claude`, an unpinned model.)
fn classifier_findings(c: &Config, out: &mut Vec<Finding>) {
    if let Some(id) = c.velocity.as_ref().and_then(|v| v.rubric.as_deref()) {
        if let Err(e) = pm_llm::QuestionSet::bundled(id) {
            out.push(f(Level::Error, format!("[velocity] rubric: {e}")));
        }
    }
    for (name, cl) in &c.classifiers {
        if let Err(e) = pm_llm::build(cl, pm_llm::Template::default()) {
            out.push(f(Level::Error, format!("classifier `{name}`: {e}")));
            continue;
        }
        if cl.provider == Provider::ClaudeCode {
            let program = cl
                .command
                .as_ref()
                .and_then(|c| c.first())
                .map_or("claude", String::as_str);
            match std::process::Command::new(program).arg("--version").output() {
                Ok(o) if o.status.success() => out.push(f(
                    Level::Ok,
                    format!(
                        "classifier `{name}`: {} (`pmx classifier test {name}` checks the login)",
                        String::from_utf8_lossy(&o.stdout).trim()
                    ),
                )),
                _ => out.push(f(
                    Level::Error,
                    format!("classifier `{name}`: `{program}` not found (install Claude Code and log in)"),
                )),
            }
            let model = cl.model.as_deref().unwrap_or(pm_llm::provider::DEFAULT_CLAUDE_MODEL);
            if !model.chars().any(|ch| ch.is_ascii_digit()) {
                out.push(f(
                    Level::Warn,
                    format!(
                        "classifier `{name}`: model `{model}` is an alias; pin a versioned id so cached scores keep their meaning"
                    ),
                ));
            }
        }
    }
}

pub fn check(ws: &Workspace, unmapped_threshold: u64) -> Vec<Finding> {
    let c = &ws.config;
    let mut out = Vec::new();

    match Git::version().ok().as_deref().and_then(parse_version) {
        Some(v) if v < (2, 30) => out.push(f(Level::Error, format!("git {}.{} is too old (need ≥ 2.30)", v.0, v.1))),
        Some(v) if v < (2, 38) => out.push(f(
            Level::Warn,
            format!(
                "git {}.{}: runs work, but ≥ 2.38 makes incremental runs faster",
                v.0, v.1
            ),
        )),
        Some(_) => out.push(f(Level::Ok, "git version")),
        None => out.push(f(Level::Error, "git not found")),
    }

    for repo in &c.repos {
        let name = repo.display_name();
        let dir = ws.repo_dir(repo);
        if !dir.exists() {
            match &repo.url {
                Some(url) => match Git::ls_remote(url) {
                    Ok(()) => out.push(f(
                        Level::Ok,
                        format!("{name}: {url} reachable (cloned on first collect)"),
                    )),
                    Err(e) => out.push(f(Level::Error, format!("{name}: cannot reach {url}: {e}"))),
                },
                None => out.push(f(Level::Error, format!("{name}: {} does not exist", dir.display()))),
            }
            continue;
        }
        let git = Git::open(&dir);
        if !git.is_repo() {
            out.push(f(
                Level::Error,
                format!("{name}: {} is not a git repository", dir.display()),
            ));
            continue;
        }
        let rev = repo
            .revision()
            .map(String::from)
            .unwrap_or_else(|| git.default_revision());
        let sha = match git.rev_parse(&rev) {
            Ok(sha) => sha,
            Err(_) => {
                out.push(f(Level::Error, format!("{name}: `{rev}` does not resolve")));
                continue;
            }
        };
        let commits = git
            .count_commits(&LogRange::new(&sha).committed_since(c.project.range_start))
            .unwrap_or(0);
        out.push(f(
            Level::Ok,
            format!("{name}: {rev} @ {} ({} commits since range start)", &sha[..12], commits),
        ));
        for lf in repo.security_lockfiles.iter().flatten() {
            if !git.path_exists(&sha, lf) {
                out.push(f(Level::Warn, format!("{name}: lockfile `{lf}` not found at {rev}")));
            }
        }
        if repo.data_policy == Some(DataPolicy::LocalOnly) {
            if let Some(cl) = c.classifier_for(repo) {
                if let Some(v) = local_only_violation(c, cl) {
                    out.push(f(Level::Error, format!("{name}: data_policy = \"local-only\" but {v}")));
                }
            }
        }
    }

    if c.code_host.is_some() {
        let env = crate::pulls::token_env(ws).expect("code host is set");
        if std::env::var_os(&env).is_none() {
            out.push(f(
                Level::Warn,
                format!("code host token ${env} is not set (needed for PR metrics)"),
            ));
        }
        let with_id = crate::pulls::code_host_repos(ws).len();
        let without: Vec<String> = c
            .repos
            .iter()
            .filter(|r| r.code_host_id.is_none())
            .map(|r| r.display_name())
            .collect();
        if with_id == 0 {
            out.push(f(
                Level::Warn,
                "[code_host] is set but no repo has a `code_host_id`: peer review not measured".to_string(),
            ));
        } else if !without.is_empty() {
            out.push(f(
                Level::Warn,
                format!("no `code_host_id`, so their PRs are not read: {}", without.join(", ")),
            ));
        }
    }
    classifier_findings(c, &mut out);
    if let Some(fte) = &c.fte {
        if fte.source == FteSource::File {
            let path = expand_path(fte.file.as_deref().unwrap_or(""), &ws.root);
            match std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            {
                Some(v) if v.get(&c.project.name).is_some() => {}
                Some(_) => out.push(f(
                    Level::Warn,
                    format!("{}: no entry for `{}`", path.display(), c.project.name),
                )),
                None => out.push(f(
                    Level::Error,
                    format!("FTE file {} is missing or not JSON", path.display()),
                )),
            }
        }
        if fte.source == FteSource::Timesheet && crate::wizard::find_on_path("gcloud").is_none() {
            out.push(f(
                Level::Error,
                "[fte] source = \"timesheet\" reads the export with gcloud, which is not on PATH".to_string(),
            ));
        }
    }

    let (mut idents, warnings) = gather(ws);
    out.extend(warnings.into_iter().map(|w| f(Level::Warn, w)));
    let logins = crate::people::gather_logins(ws);
    let n_logins = logins.len();
    idents.extend(logins);
    let proposal = propose(&c.people, &idents);
    let unmapped: Vec<String> = proposal
        .rows
        .iter()
        .filter(|(i, s)| !i.login && matches!(s, Status::Proposed(_)) && i.commits >= unmapped_threshold)
        .map(|(i, _)| format!("{} ({} commits)", i.label(), i.commits))
        .collect();
    // An unmapped login still counts as its own person; it matters for externals and for people
    // with several logins, so it is a warning.
    let unmapped_logins: Vec<String> = proposal
        .rows
        .iter()
        .filter(|(i, s)| i.login && matches!(s, Status::Proposed(_)) && i.commits >= unmapped_threshold)
        .map(|(i, _)| format!("{} ({} PRs+reviews)", i.label(), i.commits))
        .collect();
    if !unmapped_logins.is_empty() {
        out.push(f(
            Level::Warn,
            format!(
                "{} code-host logins with ≥ {unmapped_threshold} PRs and reviews are not in [people] (run `pmx people`): {}",
                unmapped_logins.len(),
                unmapped_logins.join(", ")
            ),
        ));
    }
    let idents_len = idents.len() - n_logins;
    if unmapped.is_empty() {
        out.push(f(Level::Ok, format!("{idents_len} identities mapped in [people]")));
    } else {
        out.push(f(
            Level::Error,
            format!(
                "{} identities with ≥ {unmapped_threshold} commits are not in [people] (run `pmx people`): {}",
                unmapped.len(),
                unmapped.join(", ")
            ),
        ));
    }
    out
}

/// Render findings; returns whether there was an error.
pub fn report(findings: &[Finding], w: &mut dyn std::io::Write) -> std::io::Result<bool> {
    for x in findings {
        let mark = match x.level {
            Level::Ok => "✓",
            Level::Warn => "⚠",
            Level::Error => "✗",
        };
        writeln!(w, "{mark} {}", x.message)?;
    }
    let errors = findings.iter().filter(|x| x.level == Level::Error).count();
    let warns = findings.iter().filter(|x| x.level == Level::Warn).count();
    writeln!(w, "\n{errors} error(s), {warns} warning(s)")?;
    Ok(errors > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn data_policy_guard() {
        let text = r#"
            [project]
            name = "X"
            range_start = "2025-01-01"
            breadth_roles = ["backend"]
            [[repo]]
            path = "a"
            role = "backend"
            [classifiers.local]
            provider = "openai-compatible"
            base_url = "http://localhost:11434/v1"
            fallback = "remote"
            [classifiers.remote]
            provider = "anthropic"
            [classifiers.solo]
            provider = "command"
            command = ["./classify"]
            [classifiers.claude]
            provider = "claude-code"
        "#;
        let c = Config::parse(text, Path::new("x")).unwrap();
        assert_eq!(local_only_violation(&c, "solo"), None);
        assert!(local_only_violation(&c, "local").unwrap().contains("`remote`"));
        assert!(local_only_violation(&c, "nope").unwrap().contains("unknown"));
        assert!(
            local_only_violation(&c, "claude")
                .unwrap()
                .contains("(claude-code) is not local")
        );
    }
}
