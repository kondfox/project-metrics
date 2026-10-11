//! The pull-request stage end to end against a fake GitLab: metrics, people, the per-PR cache,
//! incremental fetches, falling back to stored PRs, and "not measured".

mod common;

use std::sync::{Arc, Mutex};

use common::{JANE, Repo, lines};
use pm_codehost::{Http, Response};
use pm_config::Workspace;
use pmx::{CollectOptions, PrSource, collect};
use serde_json::{Value, json};

const API: &str = "https://gitlab.example.com/api/v4/projects/7/merge_requests";

/// A GitLab with a list of MRs and their notes; records every URL asked for.
#[derive(Default)]
struct FakeGitLab {
    mrs: Mutex<Vec<Value>>,
    notes: Mutex<Vec<(u64, Value)>>,
    seen: Mutex<Vec<String>>,
}

impl FakeGitLab {
    fn seen(&self) -> Vec<String> {
        std::mem::take(&mut self.seen.lock().unwrap())
    }
}

impl Http for FakeGitLab {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<Response, String> {
        assert!(headers.iter().any(|(k, _)| *k == "PRIVATE-TOKEN"));
        self.seen.lock().unwrap().push(url.to_string());
        let ok = |v: Value| {
            Ok(Response {
                status: 200,
                body: v.to_string(),
                ..Default::default()
            })
        };
        let rest = url.strip_prefix(API).expect("project 7");
        if rest.starts_with("?state=merged") {
            let since = rest
                .split('&')
                .find_map(|kv| kv.strip_prefix("updated_after="))
                .expect("updated_after");
            let mrs: Vec<Value> = self
                .mrs
                .lock()
                .unwrap()
                .iter()
                .filter(|m| m["updated_at"].as_str().unwrap() >= since)
                .cloned()
                .collect();
            return ok(if rest.ends_with("&page=1") {
                json!(mrs)
            } else {
                json!([])
            });
        }
        let iid: u64 = rest[1..].split('/').next().unwrap().parse().unwrap();
        let notes = self.notes.lock().unwrap();
        ok(notes
            .iter()
            .find(|(i, _)| *i == iid)
            .map(|(_, n)| n.clone())
            .unwrap_or(json!([])))
    }

    fn post(&self, _: &str, _: &[(&str, &str)], _: &str) -> Result<Response, String> {
        Err("no POST on GitLab".into())
    }
}

fn mr(iid: u64, author: &str, created: &str, merged: &str) -> Value {
    json!({"iid": iid, "author": {"username": author, "name": author}, "created_at": created,
           "merged_at": merged, "updated_at": merged})
}

fn note(author: &str, at: &str) -> Value {
    json!({"author": {"username": author}, "system": false, "body": "looks good", "created_at": at})
}

fn workspace(root: &std::path::Path, token_env: &str) -> Workspace {
    let repo = Repo::init(root.join("api"));
    repo.write("src/a.ts", &lines("A", 0..30));
    repo.commit("2025-02-03", JANE, "feat: a");
    let toml = format!(
        r#"
[project]
name = "Fixture"
range_start = "2025-01-01"
breadth_roles = ["backend"]

[[repo]]
path = "api"
branch = "main"
role = "backend"
code_host_id = 7

[code_host]
type = "gitlab"
base = "https://gitlab.example.com"
token_env = "{token_env}"

[people]
"Ann Example" = ["ann", "jane@work.example"]
externals = ["Sam Contractor"]
"Sam Contractor" = ["samc"]
"#
    );
    std::fs::write(root.join("pmx.toml"), toml).unwrap();
    Workspace::load(root).unwrap()
}

fn options(source: PrSource) -> CollectOptions {
    let mut o = CollectOptions::new(chrono::Utc::now().date_naive());
    o.fetch = false;
    o.snapshots = false;
    o.pull_requests = source;
    o
}

fn monthly(p: &pm_metrics::ProjectFile, metric: &str, month: &str) -> Option<f64> {
    let i = p.months.iter().position(|m| m == month)?;
    p.series_monthly.get(metric)?[i]
}

#[test]
fn fetches_incrementally_and_falls_back_to_stored_prs() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = workspace(tmp.path(), "PMX_TEST_UNSET_TOKEN");
    let gl = Arc::new(FakeGitLab::default());
    *gl.mrs.lock().unwrap() = vec![
        // Reviewed after 2 h by bob; ann's own comment doesn't count.
        mr(1, "ann", "2025-02-03T09:00:00Z", "2025-02-04T09:00:00Z"),
        // Never reviewed.
        mr(2, "bob", "2025-02-10T09:00:00Z", "2025-02-10T21:00:00Z"),
        // An external's MR is not counted (spec §1.3).
        mr(3, "samc", "2025-02-11T09:00:00Z", "2025-02-11T10:00:00Z"),
    ];
    *gl.notes.lock().unwrap() = vec![(
        1,
        json!([note("ann", "2025-02-03T10:00:00Z"), note("bob", "2025-02-03T11:00:00Z")]),
    )];
    let src = PrSource::Http(gl.clone());

    let c = collect(&ws, &options(src.clone())).unwrap();
    let p = &c.project;
    assert_eq!(monthly(p, "pr_count", "2025-02"), Some(2.0));
    assert_eq!(monthly(p, "review_coverage_pct", "2025-02"), Some(50.0));
    assert_eq!(monthly(p, "review_wait_median_h", "2025-02"), Some(2.0));
    // Median of 24 h and 12 h.
    assert_eq!(monthly(p, "pr_cycle_median_h", "2025-02"), Some(18.0));
    assert_eq!(monthly(p, "pr_count", "2025-03"), Some(0.0));
    assert_eq!(monthly(p, "pr_cycle_median_h", "2025-03"), None);
    assert!(!p.series_monthly.contains_key("pr_size_median"), "GitLab has no sizes");
    let info = p.meta.pull_requests.as_ref().unwrap();
    assert_eq!(
        (info.host.as_str(), info.repos.clone()),
        ("gitlab", vec!["api".to_string()])
    );
    assert!(p.meta.not_measured.contains(&"pr_size".to_string()));
    assert!(!p.meta.not_measured.contains(&"peer_review".to_string()));
    let first = gl.seen();
    assert!(first[0].contains("updated_after=2025-01-01T00:00:00Z"), "{first:?}");
    assert_eq!(first.iter().filter(|u| u.contains("/notes")).count(), 3);

    // Second run: only what changed since the last fetch (minus a day's overlap) is listed, and
    // MRs stored at the same version are not fetched again.
    gl.mrs.lock().unwrap().push(mr(
        4,
        "bob",
        "2025-03-01T09:00:00Z",
        &chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    ));
    let c = collect(&ws, &options(src)).unwrap();
    let again = gl.seen();
    assert!(!again[0].contains("updated_after=2025-01-01"), "{again:?}");
    let notes: Vec<&String> = again.iter().filter(|u| u.contains("/notes")).collect();
    assert_eq!(notes.len(), 1, "only the new MR: {again:?}");
    assert!(notes[0].contains("/4/notes"));
    let total: f64 = c.project.series_monthly["pr_count"].iter().flatten().sum();
    assert_eq!(total, 3.0);

    // The host is unreachable (here: no token): the stored PRs are used, with a warning.
    let c = collect(&ws, &options(PrSource::Live)).unwrap();
    assert!(
        c.warnings
            .iter()
            .any(|w| w.contains("using the PRs stored by an earlier run for api")),
        "{:?}",
        c.warnings
    );
    let total: f64 = c.project.series_monthly["pr_count"].iter().flatten().sum();
    assert_eq!(total, 3.0);

    // `pmx people` sees the logins.
    let logins = pmx::people::gather_logins(&ws);
    let names: Vec<&str> = logins.iter().map(|i| i.email.as_str()).collect();
    assert_eq!(names, ["ann", "bob", "samc"]);
}

#[test]
fn not_measured_without_code_host_data() {
    // No token and nothing stored: not measured, never zero.
    let tmp = tempfile::tempdir().unwrap();
    let ws = workspace(tmp.path(), "PMX_TEST_UNSET_TOKEN");
    let c = collect(&ws, &options(PrSource::Live)).unwrap();
    assert!(c.project.meta.pull_requests.is_none());
    assert!(c.project.meta.not_measured.contains(&"peer_review".to_string()));
    assert!(!c.project.series_monthly.contains_key("pr_count"));
    assert!(
        c.warnings.iter().any(|w| w.contains("PMX_TEST_UNSET_TOKEN is not set")),
        "{:?}",
        c.warnings
    );

    // No [code_host] at all: the stage is skipped quietly.
    let mut ws = ws;
    ws.config.code_host = None;
    let c = collect(&ws, &options(PrSource::Live)).unwrap();
    assert!(c.project.meta.not_measured.contains(&"peer_review".to_string()));
    assert!(c.warnings.iter().all(|w| !w.contains("PR")), "{:?}", c.warnings);
}
