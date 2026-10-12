//! The guided `pmx init` (plan D43): the prompts and the pieces of the walk-through that don't need
//! the CLI. The steps that run other commands (people, tools, check, collect, serve) live in
//! `main.rs` next to those commands.

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{Datelike, NaiveDate};
use pm_config::{ClassifierConfig, Provider, RepoRole};

use crate::setup::Suggestion;

/// Asks on stderr and reads answers line by line. Not interactive (`--yes`, or stdin not a terminal
/// without `--interactive`): every question takes its default. At the end of the input every later
/// yes/no is a no, so a short script of answers never starts a download or a collect by accident.
pub struct Prompter {
    input: Box<dyn BufRead>,
    interactive: bool,
    ended: bool,
}

impl Prompter {
    pub fn stdin(yes: bool, force: bool) -> Prompter {
        let interactive = !yes && (force || std::io::stdin().is_terminal());
        Prompter::new(Box::new(std::io::stdin().lock()), interactive)
    }

    pub fn new(input: Box<dyn BufRead>, interactive: bool) -> Prompter {
        Prompter {
            input,
            interactive,
            ended: false,
        }
    }

    pub fn interactive(&self) -> bool {
        self.interactive
    }

    /// The trimmed answer, or None when not interactive or at the end of the input.
    fn read(&mut self, prompt: &str) -> Option<String> {
        if !self.interactive || self.ended {
            return None;
        }
        eprint!("{prompt} ");
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        match self.input.read_line(&mut line) {
            Ok(0) | Err(_) => {
                self.ended = true;
                eprintln!();
                None
            }
            Ok(_) => Some(line.trim().to_string()),
        }
    }

    /// A free answer; Enter keeps `default`.
    pub fn text(&mut self, question: &str, default: &str) -> String {
        let prompt = if default.is_empty() {
            format!("{question}:")
        } else {
            format!("{question} [{default}]")
        };
        match self.read(&prompt) {
            Some(a) if !a.is_empty() => a,
            _ => default.to_string(),
        }
    }

    pub fn confirm(&mut self, question: &str, default: bool) -> bool {
        let hint = if default { "Y/n" } else { "y/N" };
        loop {
            match self.read(&format!("{question} [{hint}]")) {
                None => return default && !self.ended,
                Some(a) if a.is_empty() => return default,
                Some(a) => match a.to_lowercase().as_str() {
                    "y" | "yes" => return true,
                    "n" | "no" => return false,
                    _ => eprintln!("  answer y or n"),
                },
            }
        }
    }

    /// Parse an answer with `parse` (Enter keeps `default`), asking again until it parses.
    pub fn parsed<T>(&mut self, question: &str, default: &str, parse: impl Fn(&str) -> Result<T, String>) -> T {
        loop {
            let a = self.text(question, default);
            match parse(&a) {
                Ok(v) => return v,
                Err(e) if self.interactive && !self.ended => eprintln!("  {e}"),
                Err(e) => panic!("default `{default}` for `{question}` does not parse: {e}"),
            }
        }
    }

    /// Pick one of `options` by number (1-based on screen); Enter keeps `default`.
    pub fn choose(&mut self, question: &str, options: &[String], default: usize) -> usize {
        for (i, o) in options.iter().enumerate() {
            eprintln!("  {}) {o}", i + 1);
        }
        let n = options.len();
        self.parsed(question, &(default + 1).to_string(), |a| match a.parse::<usize>() {
            Ok(k) if (1..=n).contains(&k) => Ok(k - 1),
            _ => Err(format!("answer 1–{n}")),
        })
    }
}

/// Ask about one suggested repo. False when it should be dropped.
pub fn edit_repo(p: &mut Prompter, s: &mut Suggestion) -> bool {
    let name = s.repo.display_name();
    if !p.confirm(&format!("  keep {name}?"), true) {
        return false;
    }
    let role = s.repo.role.to_string();
    s.repo.role = p.parsed(
        "  role (frontend, backend, mobile, qa, infra, docs, data, per-file)",
        &role,
        |a| a.parse::<RepoRole>(),
    );
    let branch = s.repo.branch.clone().unwrap_or_default();
    let b = p.text("  branch", &branch);
    if !b.is_empty() {
        s.repo.branch = Some(b);
    }
    let renamed = p.text("  name", &name);
    if renamed != name {
        s.repo.name = Some(renamed);
    }
    true
}

/// Where a repo picked in the review list is: its 1-based number or its name.
pub fn pick_repo(answer: &str, suggestions: &[Suggestion]) -> Option<usize> {
    if let Ok(k) = answer.parse::<usize>() {
        return (1..=suggestions.len()).contains(&k).then(|| k - 1);
    }
    suggestions.iter().position(|s| s.repo.display_name() == answer)
}

pub fn parse_date(a: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(a, "%Y-%m-%d").map_err(|_| "answer a date as YYYY-MM-DD".into())
}

/// The default first day scored for Velocity: the first of the month three months back. Scoring is
/// the slow (and possibly paid) step, so a first run doesn't score a whole year.
pub fn default_velocity_since(today: NaiveDate, range_start: NaiveDate) -> NaiveDate {
    let m = today.year() * 12 + today.month0() as i32 - 3;
    let d = NaiveDate::from_ymd_opt(m.div_euclid(12), m.rem_euclid(12) as u32 + 1, 1).expect("valid date");
    d.max(range_start)
}

/// A classifier the wizard can offer: only what is found on this machine.
#[derive(Clone, Debug, PartialEq)]
pub struct ClassifierOffer {
    pub name: String,
    pub label: String,
    pub config: ClassifierConfig,
    /// Whether the code leaves this machine with it.
    pub remote: bool,
}

/// The model the wizard pins for `claude-code` (a versioned id, D28).
pub const CLAUDE_CODE_MODEL: &str = "claude-sonnet-5-5";

/// Where Ollama listens: `OLLAMA_HOST` (as Ollama reads it), else its default.
pub fn ollama_url() -> String {
    match std::env::var("OLLAMA_HOST") {
        Ok(h) if h.contains("://") => h.trim_end_matches('/').to_string(),
        Ok(h) if !h.is_empty() => format!("http://{h}"),
        _ => "http://localhost:11434".into(),
    }
}

pub fn classifier_offers(claude: Option<PathBuf>, ollama: &str, ollama_models: &[String]) -> Vec<ClassifierOffer> {
    let mut out = Vec::new();
    if claude.is_some() {
        out.push(ClassifierOffer {
            name: "claude".into(),
            label: format!("Claude Code under your own login ({CLAUDE_CODE_MODEL}): diffs go to Anthropic"),
            config: ClassifierConfig {
                provider: Provider::ClaudeCode,
                model: Some(CLAUDE_CODE_MODEL.into()),
                ..Default::default()
            },
            remote: true,
        });
    }
    for m in ollama_models {
        out.push(ClassifierOffer {
            name: "local".into(),
            label: format!("Ollama on this machine ({m}): nothing leaves it"),
            config: ClassifierConfig {
                provider: Provider::OpenaiCompatible,
                base_url: Some(format!("{ollama}/v1")),
                model: Some(m.clone()),
                // Reasoning off: with it on, a local model spends minutes per answer (plan §9).
                effort: Some("none".into()),
                ..Default::default()
            },
            remote: false,
        });
    }
    out
}

/// An executable on PATH.
pub fn find_on_path(program: &str) -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) { &["exe", "cmd", "bat"] } else { &[""] };
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|dir| {
        exts.iter()
            .map(|e| dir.join(program).with_extension(e))
            .find(|p| is_executable(p))
    })
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// The models a local Ollama serves; empty when none is running (asked with a short timeout).
pub fn ollama_models(ollama: &str) -> Vec<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_millis(800)))
        .build()
        .into();
    let Ok(mut resp) = agent.get(format!("{ollama}/api/tags")).call() else {
        return Vec::new();
    };
    let Some(v) = resp
        .body_mut()
        .read_to_string()
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    else {
        return Vec::new();
    };
    v["models"]
        .as_array()
        .map(|ms| ms.iter().filter_map(|m| m["name"].as_str().map(String::from)).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_config::RepoConfig;

    fn prompter(answers: &str) -> Prompter {
        Prompter::new(Box::new(std::io::Cursor::new(answers.to_string())), true)
    }

    fn suggestion(name: &str) -> Suggestion {
        Suggestion {
            repo: RepoConfig {
                path: Some(format!("~/src/{name}")),
                branch: Some("origin/main".into()),
                role: "backend".parse().unwrap(),
                ..Default::default()
            },
            why: String::new(),
            host: None,
        }
    }

    #[test]
    fn defaults_and_answers() {
        let mut p = prompter("\nAcme\n\nmaybe\ny\n");
        assert_eq!(p.text("name", "shop"), "shop");
        assert_eq!(p.text("name", "shop"), "Acme");
        assert!(!p.confirm("go?", false));
        assert!(p.confirm("go?", false), "asks again after an unclear answer");
        // End of input: texts keep their default, yes/no questions become no.
        assert_eq!(p.text("name", "shop"), "shop");
        assert!(!p.confirm("download?", true));
    }

    #[test]
    fn not_interactive_takes_defaults() {
        let mut p = Prompter::new(Box::new(std::io::Cursor::new("n\n")), false);
        assert!(p.confirm("go?", true));
        assert_eq!(p.choose("which?", &["a".into(), "b".into()], 1), 1);
    }

    #[test]
    fn choose_and_reask() {
        let mut p = prompter("7\n2\n");
        assert_eq!(p.choose("which?", &["a".into(), "b".into()], 0), 1);
        let mut p = prompter("2025-13-01\n2025-01-01\n");
        assert_eq!(
            p.parsed("since", "2024-10-01", parse_date),
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()
        );
    }

    #[test]
    fn edits_a_repo() {
        let mut s = suggestion("shop-api");
        let mut p = prompter("\nfullstack\nper-file\norigin/develop\napi\n");
        assert!(edit_repo(&mut p, &mut s));
        assert_eq!(s.repo.role.to_string(), "per-file");
        assert_eq!(s.repo.branch.as_deref(), Some("origin/develop"));
        assert_eq!(s.repo.display_name(), "api");

        let mut p = prompter("n\n");
        assert!(!edit_repo(&mut p, &mut suggestion("x")));
    }

    #[test]
    fn picks_by_number_or_name() {
        let ss = [suggestion("shop-api"), suggestion("shop-web")];
        assert_eq!(pick_repo("2", &ss), Some(1));
        assert_eq!(pick_repo("shop-api", &ss), Some(0));
        assert_eq!(pick_repo("3", &ss), None);
        assert_eq!(pick_repo("nope", &ss), None);
    }

    #[test]
    fn velocity_since() {
        let d = |y, m, dd| NaiveDate::from_ymd_opt(y, m, dd).unwrap();
        assert_eq!(default_velocity_since(d(2026, 10, 12), d(2025, 10, 1)), d(2026, 7, 1));
        assert_eq!(default_velocity_since(d(2026, 2, 3), d(2025, 2, 1)), d(2025, 11, 1));
        assert_eq!(default_velocity_since(d(2026, 2, 3), d(2026, 1, 1)), d(2026, 1, 1));
    }

    #[test]
    fn offers_only_what_is_found() {
        assert!(classifier_offers(None, "http://localhost:11434", &[]).is_empty());
        let o = classifier_offers(
            Some("/bin/claude".into()),
            "http://localhost:11434",
            &["qwen3.5:9b".into()],
        );
        assert_eq!(o.len(), 2);
        assert!(o[0].remote && !o[1].remote);
        assert_eq!(o[1].config.base_url.as_deref(), Some("http://localhost:11434/v1"));
    }
}
