//! Shared helpers for building fictional git repositories in tests.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::NaiveDate;

pub const JANE: (&str, &str) = ("Jane Doe", "jane@work.example");
pub const JANE_HOME: (&str, &str) = ("jane", "jane@home.example");
pub const BOB: (&str, &str) = ("Bob Builder", "bob@work.example");
pub const BOT: (&str, &str) = ("dependabot[bot]", "49699333+dependabot[bot]@users.noreply.github.com");
pub const SAM: (&str, &str) = ("Sam Contractor", "sam@agency.example");

pub fn git(dir: &Path, args: &[&str], date: &str, who: (&str, &str)) {
    let ts = format!("{date}T10:00:00+01:00");
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
        .args(args)
        .env("GIT_AUTHOR_NAME", who.0)
        .env("GIT_AUTHOR_EMAIL", who.1)
        .env("GIT_AUTHOR_DATE", &ts)
        .env("GIT_COMMITTER_NAME", who.0)
        .env("GIT_COMMITTER_EMAIL", who.1)
        .env("GIT_COMMITTER_DATE", &ts)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `n` distinct, non-trivial lines.
pub fn lines(tag: &str, range: std::ops::Range<usize>) -> String {
    range.map(|i| format!("export pub const {tag}{i} = {i};\n")).collect()
}

pub struct Repo(pub PathBuf);

impl Repo {
    pub fn init(dir: PathBuf) -> Repo {
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"], "2024-01-01", JANE);
        Repo(dir)
    }

    pub fn write(&self, path: &str, body: &str) -> &Self {
        let p = self.0.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
        self
    }

    pub fn commit(&self, date: &str, who: (&str, &str), msg: &str) {
        git(&self.0, &["add", "-A"], date, who);
        git(&self.0, &["commit", "-q", "-m", msg], date, who);
    }
}

pub fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

impl Repo {
    pub fn rm(&self, path: &str) -> &Self {
        std::fs::remove_file(self.0.join(path)).unwrap();
        self
    }

    pub fn head(&self) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// `git reset --hard <rev>` (to rewrite history).
    pub fn reset_hard(&self, rev: &str) {
        git(&self.0, &["reset", "-q", "--hard", rev], "2024-01-01", JANE);
    }
}
