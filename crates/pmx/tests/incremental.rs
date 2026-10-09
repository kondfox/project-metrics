//! Incremental and resumed runs must give exactly what a full run gives (plan §5, M1b).

mod common;

use std::path::Path;

use common::{BOB, JANE, Repo, d, lines};
use pm_config::{Config, Workspace};
use pm_metrics::ProjectFile;
use pmx::collect::{Work, plan};
use pmx::{CollectOptions, collect};

fn workspace(root: &Path, repo: &Path) -> Workspace {
    let text = format!(
        r#"
        [project]
        name = "Incremental"
        range_start = "2025-01-01"
        breadth_roles = ["backend"]

        [[repo]]
        path = "{}"
        branch = "main"
        role = "backend"
        "#,
        repo.display()
    );
    Workspace {
        root: root.to_path_buf(),
        config: Config::parse(&text, Path::new("pmx.toml")).unwrap(),
    }
}

fn opts(cache: bool) -> CollectOptions {
    let mut o = CollectOptions::new(d("2025-04-30"));
    o.fetch = false;
    o.use_cache = cache;
    o
}

fn same(a: &ProjectFile, b: &ProjectFile) {
    assert_eq!(a.days, b.days, "days differ");
    assert_eq!(a.series_monthly, b.series_monthly);
    assert_eq!(a.series_weekly, b.series_weekly);
    assert_eq!(a.n_monthly, b.n_monthly);
    assert_eq!(a.ai_compare, b.ai_compare);
}

/// A run without any cache, in a fresh workspace.
fn fresh(repo: &Path) -> ProjectFile {
    let tmp = tempfile::tempdir().unwrap();
    collect(&workspace(tmp.path(), repo), &opts(false)).unwrap().project
}

fn works(ws: &Workspace) -> (Work, Work) {
    let p = plan(ws, &opts(true)).unwrap().remove(0);
    (p.ingest, p.rework)
}

#[test]
fn incremental_resumed_and_rewritten_runs_equal_full_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = Repo::init(tmp.path().join("api"));
    repo.write("src/a.ts", &lines("a", 0..30));
    repo.commit("2024-12-28", JANE, "before the range");
    repo.write("src/a.ts", &(lines("a", 0..30) + &lines("jan", 0..10)));
    repo.commit("2025-01-20", JANE, "january");
    repo.write("src/b.ts", &lines("b", 0..20));
    repo.commit("2025-02-03", BOB, "february");
    let phase1 = repo.head();

    let ws_dir = tmp.path().join("ws");
    let ws = workspace(&ws_dir, &repo.0);
    assert!(matches!(works(&ws), (Work::Full { .. }, Work::Full { .. })));
    let first = collect(&ws, &opts(true)).unwrap().project;
    same(&first, &fresh(&repo.0));
    assert_eq!(works(&ws), (Work::Cached, Work::Cached));

    // New commits, one of them back-dated before the cached tip: it deletes lines added 12 days
    // earlier, so it is rework on a day the cached walk already covered.
    repo.write("src/a.ts", &(lines("a", 0..30) + &lines("jan", 0..4)));
    repo.commit("2025-02-01", JANE, "back-dated cleanup");
    repo.write("src/b.ts", &lines("b", 5..20));
    repo.commit("2025-02-10", BOB, "trim b");
    repo.write("src/c.ts", &lines("c", 0..15));
    repo.commit("2025-03-05", JANE, "march");
    match works(&ws) {
        (Work::Incremental { units: 3, .. }, Work::Incremental { .. }) => {}
        other => panic!("expected incremental work, got {other:?}"),
    }
    let second = collect(&ws, &opts(true)).unwrap().project;
    let full = fresh(&repo.0);
    same(&second, &full);
    let rework = |p: &ProjectFile| p.days[&d("2025-02-01")].reworked;
    assert_eq!(rework(&second), 6, "the back-dated deletions are rework");

    // Interrupted after the ingest stage: the rework result is missing; the next run redoes only
    // that stage.
    let db = rusqlite::Connection::open(ws_dir.join(".pmx/cache.sqlite")).unwrap();
    db.execute("DELETE FROM stage_result WHERE stage = 'rework'", [])
        .unwrap();
    assert!(matches!(works(&ws), (Work::Cached, Work::Full { .. })));
    same(&collect(&ws, &opts(true)).unwrap().project, &full);

    // History rewritten (force-push): the cached SHA is no longer an ancestor, so it starts over.
    repo.reset_hard(&phase1);
    repo.write("src/d.ts", &lines("d", 0..8));
    repo.commit("2025-02-20", BOB, "rewritten");
    assert!(matches!(works(&ws), (Work::Full { .. }, Work::Full { .. })));
    same(&collect(&ws, &opts(true)).unwrap().project, &fresh(&repo.0));

    // `--full` ignores the cache even when nothing changed.
    let mut o = opts(true);
    o.incremental = false;
    let p = plan(&ws, &o).unwrap().remove(0);
    assert!(matches!(p.ingest, Work::Full { .. }));
}
