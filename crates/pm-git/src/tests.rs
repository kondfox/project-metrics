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

    let commits = g.commits(&tip).unwrap();
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
    let rw = g.rework(&tip, d("2024-12-01"), 21, &is_source).unwrap();
    assert_eq!(rw[&d("2025-01-01")], (2, 0));
    assert_eq!(rw[&d("2025-01-05")], (1, 1));
    assert!(!rw.contains_key(&d("2025-01-06")));
}
