//! Git access by shelling out to `git` (plan §2.2): the commit log with `--numstat` and the
//! rework walk over `log -p -w`. Read-only; never touches a working copy.

mod numstat;
mod rework;

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

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
        c.envs(non_interactive_env());
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
            .envs(non_interactive_env())
            .stdin(Stdio::null())
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

    /// The commits of `range`, newest first, with per-file numstat (spec §1.6). `on_commit` is
    /// called as each one is parsed (for progress).
    pub fn commits(&self, range: &LogRange, on_commit: &mut dyn FnMut(&Commit)) -> Result<Vec<Commit>> {
        let format = format!("--format={REC}%H%x1f%ae%x1f%an%x1f%ad%x1f%B%x1d");
        let mut args = vec![
            "log".to_string(),
            "--no-merges".into(),
            "--numstat".into(),
            "--date=short".into(),
            format,
        ];
        args.extend(range.args());
        let mut commits = Vec::new();
        self.stream(&args, b'\x1e', &mut |chunk| {
            for c in parse_log(chunk, self.dialect)? {
                on_commit(&c);
                commits.push(c);
            }
            Ok(())
        })?;
        Ok(commits)
    }

    /// Number of non-merge commits in `range` (cheap; for planning).
    pub fn count_commits(&self, range: &LogRange) -> Result<u64> {
        let mut args = vec!["rev-list".to_string(), "--count".into(), "--no-merges".into()];
        args.extend(range.args());
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run(&refs)?
            .trim()
            .parse()
            .map_err(|e| GitError::Parse(format!("rev-list --count: {e}")))
    }

    /// Author dates of the commits in `range`.
    pub fn author_dates(&self, range: &LogRange) -> Result<Vec<NaiveDate>> {
        Ok(self.identities(range)?.into_iter().map(|i| i.date).collect())
    }

    /// Author identity and date of every non-merge commit in `range`.
    pub fn identities(&self, range: &LogRange) -> Result<Vec<AuthorStamp>> {
        let mut args = vec![
            "log".to_string(),
            "--no-merges".into(),
            "--date=short".into(),
            "--format=%ae%x1f%an%x1f%ad".into(),
        ];
        args.extend(range.args());
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run(&refs)?
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| {
                let mut f = l.splitn(3, '\x1f');
                let (email, name, date) = (f.next().unwrap_or(""), f.next().unwrap_or(""), f.next().unwrap_or(""));
                let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                    .map_err(|e| GitError::Parse(format!("author date `{date}`: {e}")))?;
                Ok(AuthorStamp {
                    email: email.to_string(),
                    name: name.to_string(),
                    date,
                })
            })
            .collect()
    }

    /// Is `a` an ancestor of (or equal to) `b`?
    pub fn is_ancestor(&self, a: &str, b: &str) -> bool {
        self.run(&["merge-base", "--is-ancestor", a, b]).is_ok()
    }

    /// The rework walk (spec §3.1): `log -p -w --reverse` over commits authored on or after
    /// `since`, streamed through a [`ReworkWalker`]. `on_commit` is called per walked commit.
    pub fn rework(
        &self,
        tip: &str,
        since: NaiveDate,
        young_days: i64,
        is_source: &dyn Fn(&str) -> bool,
        on_commit: &mut dyn FnMut(),
    ) -> Result<ReworkCounts> {
        let range = LogRange::new(tip).committed_since(since);
        let mut args: Vec<String> = [
            "log",
            "--no-merges",
            "-w",
            "--reverse",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--date=short",
            "-p",
        ]
        .map(String::from)
        .to_vec();
        args.push(format!("--pretty=format:{REC}%ad"));
        args.extend(range.args());
        let mut walker = ReworkWalker::new(since, young_days, is_source, self.dialect);
        self.stream(&args, b'\n', &mut |line| {
            if line.starts_with(REC) {
                on_commit();
            }
            walker.feed(line)
        })?;
        Ok(walker.finish())
    }

    /// Run git and hand stdout to `f` in pieces ending at `delim` (lossy UTF-8).
    fn stream(&self, args: &[String], delim: u8, f: &mut dyn FnMut(&str) -> Result<()>) -> Result<()> {
        let mut child = self
            .command()
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut reader = BufReader::with_capacity(1 << 16, child.stdout.take().expect("piped"));
        let mut buf = Vec::with_capacity(4096);
        let mut result = Ok(());
        loop {
            buf.clear();
            match reader.read_until(delim, &mut buf) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) => {
                    result = Err(e.into());
                    break;
                }
            }
            if delim != b'\n' && buf.last() == Some(&delim) {
                buf.pop();
            }
            if let Err(e) = f(&String::from_utf8_lossy(&buf)) {
                result = Err(e);
                break;
            }
        }
        drop(reader);
        let out = child.wait_with_output()?;
        result?;
        if !out.status.success() {
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            return Err(self.fail(&refs, &out.stderr));
        }
        Ok(())
    }

    /// Extract the tree at `rev` into `dest` (`git archive`: reads objects only, never touches the
    /// repo's working copy or its `.git`).
    pub fn archive_to(&self, rev: &str, dest: &Path) -> Result<()> {
        let args = ["archive", "--format=tar", rev];
        let mut child = self
            .command()
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut archive = tar::Archive::new(child.stdout.take().expect("piped"));
        archive.set_preserve_permissions(false);
        archive.set_overwrite(true);
        let unpacked = archive.unpack(dest);
        let out = child.wait_with_output()?;
        if !out.status.success() {
            return Err(self.fail(&args, &out.stderr));
        }
        unpacked.map_err(GitError::Spawn)
    }

    /// The last commit on `rev` committed before `before` (UTC midnight), as `git rev-list -1
    /// --before` finds it; `None` if the history starts later.
    pub fn sha_before(&self, rev: &str, before: NaiveDate) -> Result<Option<String>> {
        let bound = format!("--before={} 00:00:00 +0000", before.format("%Y-%m-%d"));
        let out = self.run(&["rev-list", "-1", &bound, rev, "--"])?;
        Ok(Some(out.trim().to_string()).filter(|s| !s.is_empty()))
    }

    /// Object id of `path` at `rev` (to dedupe identical lockfiles), if it exists.
    pub fn blob_id(&self, rev: &str, path: &str) -> Option<String> {
        self.run(&["rev-parse", "--verify", "--quiet", &format!("{rev}:{path}")])
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Contents of `path` at `rev`.
    pub fn show_bytes(&self, rev: &str, path: &str) -> Result<Vec<u8>> {
        let spec = format!("{rev}:{path}");
        let args = ["show", spec.as_str()];
        let out = self.command().args(args).stdin(Stdio::null()).output()?;
        if !out.status.success() {
            return Err(self.fail(&args, &out.stderr));
        }
        Ok(out.stdout)
    }

    /// `remote.origin.url`, if any.
    pub fn remote_url(&self) -> Option<String> {
        self.run(&["config", "--get", "remote.origin.url"])
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// The branch to suggest measuring: the remote's default branch (`origin/main`), else the
    /// checked-out branch, else `HEAD`.
    pub fn suggested_branch(&self) -> String {
        if let Ok(s) = self.run(&["symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"]) {
            let s = s.trim();
            if !s.is_empty() {
                return s.to_string();
            }
        }
        match self.run(&["symbolic-ref", "--quiet", "--short", "HEAD"]) {
            Ok(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => "HEAD".into(),
        }
    }

    /// Every file path in the tree at `rev`.
    pub fn ls_tree(&self, rev: &str) -> Result<Vec<String>> {
        Ok(self
            .run(&["ls-tree", "-r", "--name-only", "-z", rev])?
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(String::from)
            .collect())
    }

    /// Does `path` exist in the tree at `rev`?
    pub fn path_exists(&self, rev: &str, path: &str) -> bool {
        self.run(&["cat-file", "-e", &format!("{rev}:{path}")]).is_ok()
    }

    /// Can the remote be reached (and read) without prompting?
    pub fn ls_remote(url: &str) -> Result<()> {
        let out = Command::new("git")
            .args(["ls-remote", "--heads", url])
            .envs(non_interactive_env())
            .stdin(Stdio::null())
            .output()?;
        if !out.status.success() {
            return Err(GitError::Failed {
                dir: PathBuf::from("."),
                args: format!("ls-remote {url}"),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        Ok(())
    }
}

/// Author identity and date of one commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorStamp {
    pub email: String,
    pub name: String,
    pub date: NaiveDate,
}

/// Which commits to read: reachable from `tip`, not from `exclude`, optionally prefiltered on the
/// committer date.
#[derive(Clone, Debug)]
pub struct LogRange {
    pub tip: String,
    pub exclude: Option<String>,
    pub committed_since: Option<NaiveDate>,
}

/// A commit's committer date is practically never this much earlier than its author date, so a
/// committer-date prefilter this far before an author-date bound drops nothing (checked on the
/// golden repos). The exact filter is applied in code, on the author date.
pub const COMMITTER_DATE_MARGIN_DAYS: u64 = 7;

impl LogRange {
    pub fn new(tip: &str) -> LogRange {
        LogRange {
            tip: tip.to_string(),
            exclude: None,
            committed_since: None,
        }
    }

    /// Only commits not reachable from `old`.
    pub fn since_commit(mut self, old: &str) -> LogRange {
        self.exclude = Some(old.to_string());
        self
    }

    /// Skip commits whose committer date is well before `date` (a speed-up only; needs git ≥ 2.38,
    /// ignored on older git).
    pub fn committed_since(mut self, date: NaiveDate) -> LogRange {
        self.committed_since = Some(date);
        self
    }

    fn args(&self) -> Vec<String> {
        let mut a = vec![self.tip.clone()];
        if let Some(x) = &self.exclude {
            a.push(format!("^{x}"));
        }
        if let Some(d) = self.committed_since {
            if supports_since_as_filter() {
                let bound = d - chrono::Days::new(COMMITTER_DATE_MARGIN_DAYS);
                a.push(format!("--since-as-filter={} 00:00:00 +0000", bound.format("%Y-%m-%d")));
            }
        }
        a.push("--".into());
        a
    }
}

/// `git log --since-as-filter` arrived in git 2.38.
fn supports_since_as_filter() -> bool {
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(|| {
        Git::version()
            .ok()
            .and_then(|v| parse_version(&v))
            .is_some_and(|v| v >= (2, 38))
    })
}

/// `git version 2.47.0` → `(2, 47)`.
pub fn parse_version(s: &str) -> Option<(u32, u32)> {
    let v = s.split_whitespace().nth(2)?;
    let mut it = v.split('.');
    Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
}

/// Never prompt: no terminal credential prompts, and ssh in batch mode (unless the user set their
/// own ssh command).
fn non_interactive_env() -> Vec<(&'static str, String)> {
    let mut env = vec![("GIT_TERMINAL_PROMPT", "0".to_string())];
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        env.push(("GIT_SSH_COMMAND", "ssh -o BatchMode=yes".to_string()));
    }
    env
}

#[cfg(test)]
mod tests;
