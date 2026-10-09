//! End-to-end `collect` on a scripted multi-repo workspace (fictional people and repos). Every
//! expected value below is worked out by hand from the script.

use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::NaiveDate;
use pm_classify::Role;
use pm_config::{Config, Workspace};
use pmx::{CollectOptions, collect, write_outputs};

const JANE: (&str, &str) = ("Jane Doe", "jane@work.example");
const JANE_HOME: (&str, &str) = ("jane", "jane@home.example");
const BOB: (&str, &str) = ("Bob Builder", "bob@work.example");
const BOT: (&str, &str) = ("dependabot[bot]", "49699333+dependabot[bot]@users.noreply.github.com");
const SAM: (&str, &str) = ("Sam Contractor", "sam@agency.example");

fn git(dir: &Path, args: &[&str], date: &str, who: (&str, &str)) {
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
fn lines(tag: &str, range: std::ops::Range<usize>) -> String {
    range.map(|i| format!("export const {tag}{i} = {i};\n")).collect()
}

struct Repo(PathBuf);

impl Repo {
    fn init(dir: PathBuf) -> Repo {
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"], "2024-01-01", JANE);
        Repo(dir)
    }

    fn write(&self, path: &str, body: &str) -> &Self {
        let p = self.0.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
        self
    }

    fn commit(&self, date: &str, who: (&str, &str), msg: &str) {
        git(&self.0, &["add", "-A"], date, who);
        git(&self.0, &["commit", "-q", "-m", msg], date, who);
    }
}

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn build_fixture(root: &Path) {
    let api = Repo::init(root.join("src/api"));
    api.write("src/old.ts", &lines("old", 0..10));
    api.commit("2024-12-20", JANE, "before the range");

    // Prod + test + doc; also deletes 2 lines added 17 days earlier (rework).
    api.write("src/user.ts", &lines("user", 0..50))
        .write("src/user.test.ts", &lines("t", 0..10))
        .write("README.md", "# API\n\nFictional.\n\nMore.\n")
        .write("src/old.ts", &lines("old", 2..10));
    api.commit("2025-01-06", JANE, "ACME-1 users");

    // Same person, other email; replaces 5 lines added 2 days earlier; AI-assisted.
    api.write("src/user.ts", &(lines("user", 0..45) + &lines("fix", 0..5)));
    api.commit(
        "2025-01-08",
        JANE_HOME,
        "ACME-1 fix\n\nCo-Authored-By: Claude <noreply@anthropic.com>",
    );

    api.write("src/deps.ts", &lines("dep", 0..30));
    api.commit("2025-01-09", BOT, "bump deps");

    api.write("src/ext.ts", &lines("ext", 0..20));
    api.commit("2025-01-10", SAM, "external work");

    // Deletes 3 lines added 28 days earlier (not rework), adds 2.
    api.write(
        "src/user.ts",
        &(lines("user", 3..45) + &lines("fix", 0..5) + &lines("feb", 0..2)),
    );
    api.commit("2025-02-03", JANE, "ACME-2 tidy");

    api.write("src/later.ts", &lines("later", 0..10));
    api.commit("2025-03-02", JANE, "after as_of");

    let web = Repo::init(root.join("upstream/web"));
    web.write("web/App.tsx", &lines("app", 0..60));
    web.commit("2025-01-07", JANE, "app shell");
    web.write("web/Button.tsx", &lines("btn", 0..45));
    web.commit("2025-01-20", BOB, "button");

    let mono = Repo::init(root.join("src/mono"));
    mono.write("api/service.py", &lines("svc", 0..45))
        .write("web/x.test.ts", &lines("x", 0..10));
    mono.commit("2025-02-05", BOB, "service");
}

fn config(root: &Path) -> Config {
    let text = format!(
        r#"
        [project]
        name = "Fixture Shop"
        range_start = "2025-01-01"
        breadth_roles = ["frontend", "backend", "qa"]

        [[repo]]
        path = "src/api"
        branch = "main"
        role = "backend"

        [[repo]]
        url = "file://{web}"
        role = "frontend"

        [[repo]]
        path = "src/mono"
        branch = "main"
        role = "per-file"

        [people]
        "Jane Doe" = ["jane@work.example", "jane@home.example"]
        externals = ["Sam Contractor"]
        "#,
        web = root.join("upstream/web").display()
    );
    Config::parse(&text, Path::new("pmx.toml")).unwrap()
}

fn monthly(p: &pm_metrics::ProjectFile, metric: &str) -> Vec<Option<f64>> {
    p.series_monthly[metric].clone()
}

fn close(a: Option<f64>, b: f64) -> bool {
    a.is_some_and(|a| (a - b).abs() < 1e-9)
}

#[test]
fn collect_fixture_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    build_fixture(root);
    let ws = Workspace {
        root: root.to_path_buf(),
        config: config(root),
    };
    let mut opts = CollectOptions::new(d("2025-02-28"));
    opts.fetch = false;
    let c = collect(&ws, &opts).unwrap();
    let p = &c.project;

    assert_eq!(p.months, ["2025-01", "2025-02"]);
    assert_eq!(p.weeks.first().map(String::as_str), Some("2025-W01"));
    assert_eq!(p.weeks.last().map(String::as_str), Some("2025-W09"));
    assert!(p.meta.partial_week && !p.meta.partial_month);
    assert_eq!(p.meta.headline_month, "2025-02");
    // The 2025-03-02 commit is after as_of.
    assert_eq!(p.meta.latest_commit, Some(d("2025-02-05")));
    assert_eq!(p.project.repos.len(), 3);
    assert_eq!(p.project.repos[1].name, "web");

    // January: api 01-06 + 01-08 (bot and external excluded), web 01-07 + 01-20.
    assert_eq!(monthly(p, "commits"), [Some(4.0), Some(2.0)]);
    // 50 + 10 + 5 + 60 + 45; February 2 + 45 + 10.
    assert_eq!(monthly(p, "added"), [Some(170.0), Some(57.0)]);
    // Sizes 62, 10, 60, 45 → median 52.5; p90 = 60·0.3 + 62·0.7.
    assert!(close(monthly(p, "commit_med")[0], 52.5));
    assert!(close(monthly(p, "commit_p90")[0], 61.4));
    // Jane 3 commits, Bob 1.
    assert_eq!(monthly(p, "commits_per_dev_med")[0], Some(2.0));
    assert_eq!(monthly(p, "ai_assist_commit_pct")[0], Some(25.0));
    assert!(close(monthly(p, "ai_assist_line_pct")[0], 500.0 / 170.0));
    assert_eq!(monthly(p, "test_discipline_pct")[0], Some(25.0));
    assert_eq!(monthly(p, "doc_discipline_pct")[0], Some(25.0));
    // Rework is codebase-wide: adds 60 + 5 + 30 (bot) + 20 (external) + 60 + 45; reworked 2 + 5.
    assert!(close(monthly(p, "rework_pct")[0], 700.0 / 220.0));
    assert_eq!(p.n_monthly["rework_pct"][0], Some(220));
    // February: 28-day-old deletions are not rework; 2 + 45 + 10 added.
    assert_eq!(monthly(p, "rework_pct")[1], Some(0.0));
    assert_eq!(p.n_monthly["rework_pct"][1], Some(57));

    // Multi-stack in January: Jane backend 65 + frontend 60, Bob frontend 45.
    assert_eq!(monthly(p, "active_devs")[0], Some(2.0));
    assert_eq!(monthly(p, "multi_stack_pct")[0], Some(50.0));
    assert_eq!(monthly(p, "techs_per_dev")[0], Some(1.5));
    let h = -(65.0f64 / 125.0 * (65.0f64 / 125.0).ln() + 60.0 / 125.0 * (60.0f64 / 125.0).ln()) / 2f64.ln();
    assert!(close(monthly(p, "breadth_index")[0], 100.0 * h / 2.0));
    // February, per-file repo: Bob backend 45 + qa 10 (qa below 40); Jane backend 2.
    assert_eq!(monthly(p, "multi_stack_pct")[1], Some(0.0));
    assert!(close(monthly(p, "mix_qa")[1], 1000.0 / 57.0));
    assert!(close(monthly(p, "mix_frontend")[0], 10500.0 / 170.0));

    // Scores: rework 3.18% is below the band; tests 25% of 80; docs 25% of 40.
    let s_rework = 100.0 - 4.0 * (15.0 - 700.0 / 220.0);
    assert!(close(monthly(p, "s_rework")[0], s_rework));
    assert!(close(monthly(p, "s_tests")[0], 31.25));
    assert!(close(monthly(p, "s_docs")[0], 62.5));
    let q = (s_rework.ln() + 31.25f64.ln() + 62.5f64.ln()) / 3.0;
    assert!(close(monthly(p, "quality")[0], q.exp()));
    assert_eq!(monthly(p, "quality_constituents")[0], Some(3.0));

    // Weekly: 2025-W02 (Jan 6–12) holds the first three human commits.
    let w02 = p.weeks.iter().position(|w| w == "2025-W02").unwrap();
    assert_eq!(p.series_weekly["commits"][w02], Some(3.0));
    // A week without commits has no ratios (no data ≠ zero).
    let w05 = p.weeks.iter().position(|w| w == "2025-W05").unwrap();
    assert_eq!(p.series_weekly["commits"][w05], Some(0.0));
    assert_eq!(p.series_weekly["test_discipline_pct"][w05], None);

    // AI vs human over the 12 months up to the latest commit.
    let ai = p.ai_compare.as_ref().unwrap();
    assert_eq!((ai.ai.commits, ai.ai.added), (1, 5));
    assert_eq!((ai.human.commits, ai.human.added), (5, 222));

    // project.json has no names; leads.json maps the pseudonyms.
    let json = serde_json::to_string(p).unwrap();
    for name in ["Jane", "Bob", "Sam", "jane@", "dependabot"] {
        assert!(!json.contains(name), "project.json leaks {name}");
    }
    assert_eq!(
        c.leads.people.values().cloned().collect::<Vec<_>>(),
        ["Bob Builder", "Jane Doe"]
    );
    let jan = &c.leads.leaders["2025-01"];
    assert_eq!(jan[0].name, "Jane Doe");
    assert_eq!(jan[0].stacks, [Role::Frontend, Role::Backend]);

    // A second run reads the cache and gives the same numbers.
    write_outputs(&ws.out_dir(), &c).unwrap();
    let again = collect(&ws, &opts).unwrap();
    assert_eq!(again.project.series_monthly, p.series_monthly);
    assert!(root.join(".pmx/cache.sqlite").exists());
    assert!(root.join(".pmx/repos/web.git").exists());
    assert!(root.join("out/private/leads.json").exists());
}

#[test]
fn ai_attribution_off_hides_ai_series() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    build_fixture(root);
    let mut cfg = config(root);
    cfg.project.ai_attribution = false;
    let ws = Workspace {
        root: root.to_path_buf(),
        config: cfg,
    };
    let mut opts = CollectOptions::new(d("2025-02-28"));
    opts.fetch = false;
    opts.use_cache = false;
    let c = collect(&ws, &opts).unwrap();
    assert!(!c.project.series_monthly.contains_key("ai_assist_commit_pct"));
    assert!(c.project.ai_compare.is_none());
}
