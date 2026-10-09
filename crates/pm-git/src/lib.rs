//! Git access by shelling out to `git` (plan §2.2): the commit log with `--numstat` and the
//! rework walk over `log -p -w`. Read-only; never touches a working copy.

mod numstat;
mod rework;

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use chrono::NaiveDate;

pub use numstat::{Commit, FileStat, parse_log, parse_numstat_path};
pub use rework::{ReworkCounts, ReworkWalker, is_trivial_line};

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("cannot run git: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("git {args} failed in {dir}: {stderr}")]
    Failed { dir: PathBuf, args: String, stderr: String },
    #[error("unexpected git output: {0}")]
    Parse(String),
}

pub type Result<T> = std::result::Result<T, GitError>;

/// Settings that change `log` output, pinned so a user's git config can't change the numbers.
const PINNED_CONFIG: &[&str] = &[
    "core.quotePath=false",
    "diff.noprefix=false",
    "diff.mnemonicPrefix=false",
    "diff.relative=false",
    "diff.renames=true",
    "diff.algorithm=myers",
    "diff.indentHeuristic=true",
    "log.showRoot=true",
    "log.showSignature=false",
    "color.ui=false",
    "i18n.logOutputEncoding=UTF-8",
];

/// Commit header marker in our `--format` strings (ASCII record separator).
const REC: char = '\x1e';

/// How git output is read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dialect {
    /// Paths unquoted and renames resolved to the new path; diffs parsed by their headers.
    #[default]
    Spec,
    /// The private prototype's reading, for the parity harness only: quoted paths and rename
    /// notation kept verbatim (so such files are not source), and any diff line starting with
    /// `--- ` or `+++ ` taken as a file header, even inside a hunk.
    Prototype,
}

#[derive(Clone, Debug)]
pub struct Git {
    dir: PathBuf,
    dialect: Dialect,
}

impl Git {
    pub fn open(dir: impl Into<PathBuf>) -> Git {
        Git {
            dir: dir.into(),
            dialect: Dialect::Spec,
        }
    }

    pub fn with_dialect(mut self, dialect: Dialect) -> Git {
        self.dialect = dialect;
        self
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn command(&self) -> Command {
        let mut c = Command::new("git");
        c.arg("-C").arg(&self.dir);
        for kv in PINNED_CONFIG {
            c.arg("-c").arg(kv);
        }
        if self.dialect == Dialect::Prototype {
            c.arg("-c").arg("core.quotePath=true");
        }
        c.env("GIT_TERMINAL_PROMPT", "0");
        c
    }

    fn fail(&self, args: &[&str], stderr: &[u8]) -> GitError {
        GitError::Failed {
            dir: self.dir.clone(),
            args: args.join(" "),
            stderr: String::from_utf8_lossy(stderr).trim().to_string(),
        }
    }

    /// Run git and return stdout (lossy UTF-8).
    pub fn run(&self, args: &[&str]) -> Result<String> {
        let out = self.command().args(args).stdin(Stdio::null()).output()?;
        if !out.status.success() {
            return Err(self.fail(args, &out.stderr));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// `git --version`, e.g. `git version 2.47.0`.
    pub fn version() -> Result<String> {
        let out = Command::new("git").arg("--version").output()?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    pub fn is_repo(&self) -> bool {
        self.run(&["rev-parse", "--git-dir"]).is_ok()
    }

    /// Resolve a revision to a full commit SHA.
    pub fn rev_parse(&self, rev: &str) -> Result<String> {
        let spec = format!("{rev}^{{commit}}");
        Ok(self
            .run(&["rev-parse", "--verify", "--quiet", &spec])?
            .trim()
            .to_string())
    }

    /// The remote's default branch (`origin/HEAD`), else the local `HEAD`.
    pub fn default_revision(&self) -> String {
        match self.rev_parse("origin/HEAD") {
            Ok(_) => "origin/HEAD".into(),
            Err(_) => "HEAD".into(),
        }
    }

    /// Committer date of a commit as `YYYY-MM-DD` in its own timezone.
    pub fn commit_date(&self, rev: &str) -> Result<String> {
        Ok(self
            .run(&["show", "-s", "--date=short", "--format=%cd", rev])?
            .trim()
            .to_string())
    }

    /// `git fetch`: updates remote-tracking refs only.
    pub fn fetch(&self) -> Result<()> {
        self.run(&["fetch", "--quiet", "--prune"]).map(|_| ())
    }

    /// A bare clone that only pmx reads from, with `origin/*` remote-tracking refs so that
    /// `branch = "origin/main"` works the same as for a local clone.
    pub fn clone_bare(url: &str, dest: &Path) -> Result<Git> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let out = Command::new("git")
            .args(["clone", "--quiet", "--bare", url])
            .arg(dest)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()?;
        if !out.status.success() {
            return Err(GitError::Failed {
                dir: dest.to_path_buf(),
                args: format!("clone --bare {url}"),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        let git = Git::open(dest);
        git.run(&["config", "remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*"])?;
        git.fetch()?;
        git.run(&["remote", "set-head", "origin", "--auto"])?;
        Ok(git)
    }

    /// Every non-merge commit reachable from `rev`, newest first, with per-file numstat
    /// (spec §1.6).
    pub fn commits(&self, rev: &str) -> Result<Vec<Commit>> {
        let format = format!("--format={REC}%H%x1f%ae%x1f%an%x1f%ad%x1f%B%x1d");
        let text = self.run(&["log", rev, "--no-merges", "--numstat", "--date=short", &format])?;
        parse_log(&text, self.dialect)
    }

    /// The rework walk (spec §3.1): `log -p -w --reverse` over commits authored on or after
    /// `since`, streamed through a [`ReworkWalker`].
    pub fn rework(
        &self,
        rev: &str,
        since: NaiveDate,
        young_days: i64,
        is_source: &dyn Fn(&str) -> bool,
    ) -> Result<ReworkCounts> {
        let format = format!("--pretty=format:{REC}%ad");
        let args = [
            "log",
            rev,
            "--no-merges",
            "-w",
            "--reverse",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--date=short",
            &format,
            "-p",
        ];
        let mut child = self
            .command()
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut walker = ReworkWalker::new(since, young_days, is_source, self.dialect);
        let mut reader = BufReader::with_capacity(1 << 16, child.stdout.take().expect("piped"));
        let mut buf = Vec::with_capacity(256);
        loop {
            buf.clear();
            if reader.read_until(b'\n', &mut buf)? == 0 {
                break;
            }
            walker.feed(&String::from_utf8_lossy(&buf))?;
        }
        let out = child.wait_with_output()?;
        if !out.status.success() {
            return Err(self.fail(&args, &out.stderr));
        }
        Ok(walker.finish())
    }
}

#[cfg(test)]
mod tests;
