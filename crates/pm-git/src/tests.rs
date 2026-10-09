//! Tests against a real scripted repository.

use std::path::Path;
use std::process::Command;

use chrono::NaiveDate;

use super::*;

fn git(dir: &Path, args: &[&str], date: &str, name: &str, email: &str) {
    let ts = format!("{date}T12:00:00+02:00");
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
        .args(args)
        .env("GIT_AUTHOR_NAME", name)
        .env("GIT_AUTHOR_EMAIL", email)
        .env("GIT_AUTHOR_DATE", &ts)
        .env("GIT_COMMITTER_NAME", name)
        .env("GIT_COMMITTER_EMAIL", email)
        .env("GIT_COMMITTER_DATE", &ts)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write(dir: &Path, path: &str, body: &str) {
    let p = dir.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

fn commit(dir: &Path, date: &str, msg: &str) {
    git(dir, &["add", "-A"], date, "Jane Doe", "jane@example.com");
    git(dir, &["commit", "-q", "-m", msg], date, "Jane Doe", "jane@example.com");
}

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

#[test]
fn log_and_rework_on_a_real_repo() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q"], "2025-01-01", "x", "x@example.com");

    write(dir, "src/app.ts", "const a = 1;\nconst b = 2;\n");
    write(dir, "docs/été.md", "# hello\n");
    commit(dir, "2025-01-01", "init");

    write(dir, "src/app.ts", "const a = 1;\nconst b = 3;\n");
    commit(
        dir,
        "2025-01-05",
        "tweak\n\nCo-Authored-By: Claude <noreply@anthropic.com>",
    );

    std::fs::rename(dir.join("src/app.ts"), dir.join("src/main.ts")).unwrap();
    commit(dir, "2025-01-06", "rename");

    let g = Git::open(dir);
    let tip = g.rev_parse("HEAD").unwrap();
    assert_eq!(tip.len(), 40);

    let mut seen = 0;
    let commits = g.commits(&LogRange::new(&tip), &mut |_| seen += 1).unwrap();
    assert_eq!(seen, 3);
    assert_eq!(commits.len(), 3);
    let first = &commits[2];
    assert_eq!(first.author_date, d("2025-01-01"));
    assert_eq!(first.author_name, "Jane Doe");
    let mut paths: Vec<_> = first.files.iter().map(|f| f.path.as_str()).collect();
    paths.sort();
    assert_eq!(paths, ["docs/été.md", "src/app.ts"]);
    assert!(commits[1].message.contains("Co-Authored-By: Claude"));
    assert_eq!(commits[1].files[0].added, 1);
    assert_eq!(commits[0].files[0].path, "src/main.ts");
    assert_eq!(commits[0].files[0].added, 0);

    let is_source = |p: &str| p.ends_with(".ts");
    let mut walked = 0;
    let rw = g
        .rework(&tip, d("2024-12-01"), 21, &is_source, &mut || walked += 1)
        .unwrap();
    assert_eq!(walked, 3);
    assert_eq!(rw[&d("2025-01-01")], (2, 0));
    assert_eq!(rw[&d("2025-01-05")], (1, 1));
    assert!(!rw.contains_key(&d("2025-01-06")));
}

#[test]
fn ranges_ancestry_and_setup_helpers() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q"], "2025-01-01", "x", "x@example.com");
    write(dir, "a.ts", "const a = 1;\n");
    commit(dir, "2025-01-01", "one");
    let g = Git::open(dir);
    let first = g.rev_parse("HEAD").unwrap();
    write(dir, "b.ts", "const b = 2;\n");
    commit(dir, "2025-02-01", "two");
    write(dir, "c.ts", "const c = 3;\n");
    commit(dir, "2025-03-01", "three");
    let tip = g.rev_parse("HEAD").unwrap();

    assert_eq!(g.count_commits(&LogRange::new(&tip)).unwrap(), 3);
    let newer = LogRange::new(&tip).since_commit(&first);
    assert_eq!(g.count_commits(&newer).unwrap(), 2);
    let new_commits = g.commits(&newer, &mut |_| {}).unwrap();
    assert_eq!(new_commits.len(), 2);
    assert_eq!(g.author_dates(&newer).unwrap(), vec![d("2025-03-01"), d("2025-02-01")]);
    let ids = g.identities(&LogRange::new(&tip)).unwrap();
    assert_eq!(ids[0].email, "jane@example.com");
    assert_eq!(ids[0].name, "Jane Doe");
    // The committer-date prefilter keeps everything within its margin and drops older commits.
    let recent = LogRange::new(&tip).committed_since(d("2025-02-05"));
    assert_eq!(g.count_commits(&recent).unwrap(), 2);

    assert!(g.is_ancestor(&first, &tip));
    assert!(!g.is_ancestor(&tip, &first));
    assert_eq!(g.suggested_branch(), "main");
    assert_eq!(g.remote_url(), None);
    let mut files = g.ls_tree(&tip).unwrap();
    files.sort();
    assert_eq!(files, ["a.ts", "b.ts", "c.ts"]);
    assert!(g.path_exists(&tip, "b.ts"));
    assert!(!g.path_exists(&first, "b.ts"));
    assert_eq!(parse_version("git version 2.47.0"), Some((2, 47)));
    assert_eq!(parse_version("git version 2.39.5 (Apple Git-154)"), Some((2, 39)));
}
