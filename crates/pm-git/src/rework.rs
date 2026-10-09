//! Line-age rework over `git log -p -w --reverse` (spec §3.1).
//!
//! Every non-trivial added line in a source file records `(file, text) → date`. A deleted line whose
//! `(file, text)` was last added at most `young_days` earlier counts as reworked on the deletion's
//! date. Files are keyed by their post-image path, so renames are not followed and the lines of a
//! deleted file are not seen (both as in the prototype).

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;

use crate::numstat::unquote;
use crate::{Dialect, GitError, REC, Result};

const TRIVIAL: &[&str] = &[
    "{", "}", "});", ")", ");", "},", "};", "[", "]", "((", "))", "return;", "else", "try {",
];

/// Trivial lines are ignored by the rework walk: stripped length < 4, or a lone brace-like token.
pub fn is_trivial_line(s: &str) -> bool {
    let t = s.trim();
    t.chars().count() < 4 || TRIVIAL.contains(&t)
}

/// Per author date: non-trivial source lines added, and lines reworked.
pub type ReworkCounts = BTreeMap<NaiveDate, (u64, u64)>;

#[derive(PartialEq)]
enum State {
    /// Between a commit header and the first `diff --git`.
    Commit,
    /// In a file header (`diff --git`, `index`, `---`, `+++`, …).
    Header,
    Hunk,
}

pub struct ReworkWalker<'a> {
    since: NaiveDate,
    young_days: i64,
    is_source: &'a dyn Fn(&str) -> bool,
    dialect: Dialect,
    state: State,
    date: Option<NaiveDate>,
    /// Current file id, if it is a source file in a commit we count.
    file: Option<u32>,
    files: HashMap<String, (u32, bool)>,
    last_added: HashMap<(u32, Box<str>), NaiveDate>,
    counts: ReworkCounts,
}

impl<'a> ReworkWalker<'a> {
    pub fn new(since: NaiveDate, young_days: i64, is_source: &'a dyn Fn(&str) -> bool, dialect: Dialect) -> Self {
        ReworkWalker {
            since,
            young_days,
            is_source,
            dialect,
            state: State::Commit,
            date: None,
            file: None,
            files: HashMap::new(),
            last_added: HashMap::new(),
            counts: BTreeMap::new(),
        }
    }

    fn file_id(&mut self, path: &str) -> Option<u32> {
        if let Some(&(id, src)) = self.files.get(path) {
            return src.then_some(id);
        }
        let id = self.files.len() as u32;
        let src = (self.is_source)(path);
        self.files.insert(path.to_string(), (id, src));
        src.then_some(id)
    }

    /// Feed one output line (with or without its trailing newline).
    pub fn feed(&mut self, line: &str) -> Result<()> {
        let line = line.strip_suffix('\n').unwrap_or(line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(d) = line.strip_prefix(REC) {
            let date = NaiveDate::parse_from_str(d.trim(), "%Y-%m-%d")
                .map_err(|e| GitError::Parse(format!("rework walk: author date `{d}`: {e}")))?;
            self.date = (date >= self.since).then_some(date);
            self.state = State::Commit;
            self.file = None;
            return Ok(());
        }
        if self.dialect == Dialect::Prototype {
            return self.feed_prototype(line);
        }
        if line.starts_with("diff --git ") {
            self.state = State::Header;
            self.file = None;
            return Ok(());
        }
        match self.state {
            State::Commit => {}
            State::Header => {
                if let Some(p) = line.strip_prefix("+++ ") {
                    let p = unquote(p);
                    self.file = match p.as_str() {
                        "/dev/null" => None,
                        p => {
                            let path = p.strip_prefix("b/").unwrap_or(p).to_string();
                            if self.date.is_some() { self.file_id(&path) } else { None }
                        }
                    };
                } else if line.starts_with("@@") {
                    self.state = State::Hunk;
                }
            }
            State::Hunk => self.content(line),
        }
        Ok(())
    }

    /// The prototype's line loop: no header state, so content lines that begin with `--- ` or
    /// `+++ ` are mistaken for file headers.
    fn feed_prototype(&mut self, line: &str) -> Result<()> {
        if let Some(p) = line.strip_prefix("+++ ") {
            let p = p.strip_prefix("b/").unwrap_or(p).to_string();
            self.file = match p.as_str() {
                "/dev/null" => None,
                p if self.date.is_some() => self.file_id(p),
                _ => None,
            };
        } else if !line.starts_with("--- ") {
            self.content(line);
        }
        Ok(())
    }

    fn content(&mut self, line: &str) {
        let (Some(date), Some(file)) = (self.date, self.file) else {
            return;
        };
        if let Some(text) = line.strip_prefix('+') {
            if !is_trivial_line(text) {
                self.counts.entry(date).or_default().0 += 1;
                self.last_added.insert((file, text.into()), date);
            }
        } else if let Some(text) = line.strip_prefix('-') {
            if !is_trivial_line(text) {
                if let Some(prev) = self.last_added.get(&(file, text.into())) {
                    if (date - *prev).num_days() <= self.young_days {
                        self.counts.entry(date).or_default().1 += 1;
                    }
                }
            }
        }
    }

    pub fn finish(self) -> ReworkCounts {
        self.counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn walk(text: &str, since: &str) -> ReworkCounts {
        walk_as(text, since, Dialect::Spec)
    }

    fn walk_as(text: &str, since: &str, dialect: Dialect) -> ReworkCounts {
        let is_source = |p: &str| p.ends_with(".ts") || p.ends_with(".sql");
        let mut w = ReworkWalker::new(d(since), 21, &is_source, dialect);
        for l in text.lines() {
            w.feed(l).unwrap();
        }
        w.finish()
    }

    const DIFF: &str = "\x1e2025-01-01
diff --git a/src/a.ts b/src/a.ts
new file mode 100644
--- /dev/null
+++ b/src/a.ts
@@ -0,0 +3 @@
+const answer = 42;
+}
+-- not a header, an added line starting with dashes
\x1e2025-01-10
diff --git a/src/a.ts b/src/a.ts
--- a/src/a.ts
+++ b/src/a.ts
@@ -1,3 +1,2 @@
-const answer = 42;
--- not a header, an added line starting with dashes
+const answer = 43;
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1 +1 @@
-old readme line
+new readme line
\x1e2025-03-01
diff --git a/src/a.ts b/src/a.ts
--- a/src/a.ts
+++ b/src/a.ts
@@ -1 +1 @@
-const answer = 43;
+const answer = 44;
";

    #[test]
    fn counts_young_deletions_only() {
        let c = walk(DIFF, "2024-12-01");
        // Day 1: two non-trivial adds (`}` is trivial).
        assert_eq!(c[&d("2025-01-01")], (2, 0));
        // Day 10: both deleted lines were added 9 days earlier; the `---` content line is a deletion.
        assert_eq!(c[&d("2025-01-10")], (1, 2));
        // Day 59: the line was added 50 days earlier: not rework.
        assert_eq!(c[&d("2025-03-01")], (1, 0));
    }

    #[test]
    fn prototype_dialect_skips_dash_content() {
        let c = walk_as(DIFF, "2024-12-01", Dialect::Prototype);
        // The `---` content line is skipped, so only one deletion is rework on day 10.
        assert_eq!(c[&d("2025-01-10")], (1, 1));
    }

    #[test]
    fn ignores_commits_before_since() {
        let c = walk(DIFF, "2025-01-05");
        assert!(!c.contains_key(&d("2025-01-01")));
        // The adds of Jan 1 were never seen, so nothing is rework on Jan 10.
        assert_eq!(c[&d("2025-01-10")], (1, 0));
    }

    #[test]
    fn deleted_files_and_binary_are_skipped() {
        let text = "\x1e2025-01-01
diff --git a/src/a.ts b/src/a.ts
--- /dev/null
+++ b/src/a.ts
@@ -0,0 +1 @@
+const gone = true;
\x1e2025-01-02
diff --git a/src/a.ts b/src/a.ts
deleted file mode 100644
--- a/src/a.ts
+++ /dev/null
@@ -1 +0,0 @@
-const gone = true;
diff --git a/img.png b/img.png
Binary files a/img.png and b/img.png differ
";
        let c = walk(text, "2024-01-01");
        assert_eq!(c.get(&d("2025-01-02")), None);
    }

    #[test]
    fn trivial_lines() {
        assert!(is_trivial_line("  });  "));
        assert!(is_trivial_line("abc"));
        assert!(is_trivial_line("try {"));
        assert!(!is_trivial_line("abcd"));
        assert!(!is_trivial_line("return x;"));
    }
}
