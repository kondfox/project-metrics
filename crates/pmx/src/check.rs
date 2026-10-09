//! `pmx check` (plan §3.2): every path exists, every revision resolves, tokens are present, the
//! data-policy guard holds, and no active identity is unmapped.

use pm_config::{Config, DataPolicy, FteSource, Workspace, expand_path};
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

/// Is `url`'s host loopback (`localhost`, `127.*`, `::1`)?
pub fn is_loopback(url: &str) -> bool {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let authority = rest.split('/').next().unwrap_or("");
    let authority = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    host.eq_ignore_ascii_case("localhost") || host.starts_with("127.") || host == "::1"
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
        let provider = c.get("provider").and_then(|v| v.as_str()).unwrap_or("");
        let base = c.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
        let local = match provider {
            "command" => true,
            "openai-compatible" => is_loopback(base),
            _ => false,
        };
        if !local {
            return Some(format!("classifier `{name}` ({provider}) is not local"));
        }
        name = c.get("fallback").and_then(|v| v.as_str())?.to_string();
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

    let default_classifier = c
        .velocity
        .as_ref()
        .and_then(|v| v.get("classifier"))
        .and_then(|v| v.as_str())
        .map(String::from);
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
            if let Some(cl) = repo.classifier.clone().or(default_classifier.clone()) {
                if let Some(v) = local_only_violation(c, &cl) {
                    out.push(f(Level::Error, format!("{name}: data_policy = \"local-only\" but {v}")));
                }
            }
        }
    }

    if let Some(h) = &c.code_host {
        if let Some(env) = &h.token_env {
            if std::env::var_os(env).is_none() {
                out.push(f(
                    Level::Warn,
                    format!("code host token ${env} is not set (needed for PR metrics)"),
                ));
            }
        }
    }
    for (name, cl) in &c.classifiers {
        if let Some(env) = cl.get("api_key_env").and_then(|v| v.as_str()) {
            if std::env::var_os(env).is_none() {
                out.push(f(Level::Warn, format!("classifier `{name}`: ${env} is not set")));
            }
        }
    }
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
    }

    let (idents, warnings) = gather(ws);
    out.extend(warnings.into_iter().map(|w| f(Level::Warn, w)));
    let proposal = propose(&c.people, &idents);
    let unmapped: Vec<String> = proposal
        .rows
        .iter()
        .filter(|(i, s)| matches!(s, Status::Proposed(_)) && i.commits >= unmapped_threshold)
        .map(|(i, _)| format!("{} ({} commits)", i.label(), i.commits))
        .collect();
    if unmapped.is_empty() {
        out.push(f(Level::Ok, format!("{} identities mapped in [people]", idents.len())));
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
    fn loopback() {
        assert!(is_loopback("http://localhost:11434/v1"));
        assert!(is_loopback("http://127.0.0.1:8080"));
        assert!(is_loopback("http://[::1]:8080/v1"));
        assert!(!is_loopback("https://api.example.com/v1"));
        assert!(!is_loopback("http://localhost.example.com"));
    }

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
        "#;
        let c = Config::parse(text, Path::new("x")).unwrap();
        assert_eq!(local_only_violation(&c, "solo"), None);
        assert!(local_only_violation(&c, "local").unwrap().contains("`remote`"));
        assert!(local_only_violation(&c, "nope").unwrap().contains("unknown"));
    }
}
