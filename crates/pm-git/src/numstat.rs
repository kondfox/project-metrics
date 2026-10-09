//! Parser for `git log --numstat` with pmx's record-separated `--format`.

use chrono::NaiveDate;

use crate::{Dialect, GitError, REC, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStat {
    /// Lines added; binary files (`-`) count as 0 (spec §1.6).
    pub added: u64,
    pub deleted: u64,
    /// Post-image path; renames are resolved to the new path.
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub sha: String,
    pub author_email: String,
    pub author_name: String,
    /// Author date in the author's own timezone (spec §1.1).
    pub author_date: NaiveDate,
    pub message: String,
    pub files: Vec<FileStat>,
}

pub fn parse_log(text: &str, dialect: Dialect) -> Result<Vec<Commit>> {
    let mut commits = Vec::new();
    for rec in text.split(REC).filter(|r| !r.trim().is_empty()) {
        let (header, stats) = rec
            .split_once('\x1d')
            .ok_or_else(|| GitError::Parse("commit record without terminator".into()))?;
        let mut f = header.splitn(5, '\x1f');
        let mut next = |what: &str| {
            f.next()
                .ok_or_else(|| GitError::Parse(format!("commit record without {what}")))
        };
        let sha = next("sha")?.to_string();
        let author_email = next("email")?.to_string();
        let author_name = next("name")?.to_string();
        let date = next("date")?;
        let author_date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|e| GitError::Parse(format!("author date `{date}`: {e}")))?;
        let message = next("message")?.to_string();
        let files = stats.lines().filter_map(|l| parse_numstat_line(l, dialect)).collect();
        commits.push(Commit {
            sha,
            author_email,
            author_name,
            author_date,
            message,
            files,
        });
    }
    Ok(commits)
}

fn parse_numstat_line(line: &str, dialect: Dialect) -> Option<FileStat> {
    let mut parts = line.splitn(3, '\t');
    let (a, d, p) = (parts.next()?, parts.next()?, parts.next()?);
    let num = |s: &str| if s == "-" { Some(0) } else { s.parse().ok() };
    Some(FileStat {
        added: num(a)?,
        deleted: num(d)?,
        path: match dialect {
            Dialect::Spec => parse_numstat_path(p),
            Dialect::Prototype => p.to_string(),
        },
    })
}

/// Resolve numstat's rename notation (`dir/{old => new}/f.ts`, `old.ts => new.ts`) to the new
/// path, and undo C-style quoting.
pub fn parse_numstat_path(raw: &str) -> String {
    let p = unquote(raw);
    let Some(arrow) = p.find(" => ") else {
        return p;
    };
    if let (Some(open), Some(close)) = (p[..arrow].rfind('{'), p[arrow..].find('}').map(|i| i + arrow)) {
        let new = &p[arrow + 4..close];
        let joined = format!("{}{}{}", &p[..open], new, &p[close + 1..]);
        let collapsed = joined.replace("//", "/");
        return collapsed.trim_start_matches('/').to_string();
    }
    p[arrow + 4..].to_string()
}

/// Undo git's C-style path quoting (`"a\tb\303\251.ts"`). Unquoted input is returned as is.
pub(crate) fn unquote(s: &str) -> String {
    let Some(inner) = s.strip_prefix('"').and_then(|x| x.strip_suffix('"')) else {
        return s.to_string();
    };
    let mut bytes = Vec::with_capacity(inner.len());
    let mut it = inner.bytes().peekable();
    while let Some(b) = it.next() {
        if b != b'\\' {
            bytes.push(b);
            continue;
        }
        match it.next() {
            Some(b'n') => bytes.push(b'\n'),
            Some(b't') => bytes.push(b'\t'),
            Some(b'r') => bytes.push(b'\r'),
            Some(b'a') => bytes.push(0x07),
            Some(b'b') => bytes.push(0x08),
            Some(b'f') => bytes.push(0x0c),
            Some(b'v') => bytes.push(0x0b),
            Some(d @ b'0'..=b'7') => {
                let mut v = (d - b'0') as u32;
                for _ in 0..2 {
                    match it.peek() {
                        Some(&n @ b'0'..=b'7') => {
                            v = v * 8 + (n - b'0') as u32;
                            it.next();
                        }
                        _ => break,
                    }
                }
                bytes.push(v as u8);
            }
            Some(other) => bytes.push(other),
            None => bytes.push(b'\\'),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_paths() {
        assert_eq!(parse_numstat_path("src/a.ts"), "src/a.ts");
        assert_eq!(parse_numstat_path("src/{old => new}/f.ts"), "src/new/f.ts");
        assert_eq!(parse_numstat_path("src/{a.js => b.ts}"), "src/b.ts");
        assert_eq!(parse_numstat_path("src/{ => sub}/f.ts"), "src/sub/f.ts");
        assert_eq!(parse_numstat_path("src/{sub => }/f.ts"), "src/f.ts");
        assert_eq!(parse_numstat_path("{a => b}/f.ts"), "b/f.ts");
        assert_eq!(parse_numstat_path("old.ts => new.ts"), "new.ts");
    }

    #[test]
    fn quoted_paths() {
        assert_eq!(unquote(r#""a\tb.ts""#), "a\tb.ts");
        assert_eq!(unquote(r#""\303\251t\303\251.ts""#), "été.ts");
        assert_eq!(unquote(r#""say \"hi\".md""#), "say \"hi\".md");
        assert_eq!(unquote("plain.ts"), "plain.ts");
    }

    #[test]
    fn parses_records() {
        let text = "\x1eabc\x1fjane@example.com\x1fJane Doe\x1f2025-03-04\x1fFix a thing\n\nCo-Authored-By: Claude <noreply@anthropic.com>\n\x1d\n\n3\t1\tsrc/a.ts\n-\t-\tlogo.png\n\
                    \x1edef\x1fsam@example.com\x1fSam\x1f2025-03-03\x1fEmpty\n\x1d\n";
        let c = parse_log(text, Dialect::Spec).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].sha, "abc");
        assert_eq!(c[0].author_date, NaiveDate::from_ymd_opt(2025, 3, 4).unwrap());
        assert!(c[0].message.contains("Co-Authored-By: Claude"));
        assert_eq!(
            c[0].files,
            vec![
                FileStat {
                    added: 3,
                    deleted: 1,
                    path: "src/a.ts".into()
                },
                FileStat {
                    added: 0,
                    deleted: 0,
                    path: "logo.png".into()
                },
            ]
        );
        assert!(c[1].files.is_empty());
    }
}
