//! The velocity stage end to end with a fake `command` classifier: work packages, the diff and state
//! sent, R1–R6 and points computed in code, Velocity per FTE-day, the answer cache, failures, and
//! the local-only guard.
#![cfg(unix)]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use common::{BOB, BOT, JANE, Repo, d, lines};
use pm_config::Workspace;
use pmx::{CollectOptions, collect};
use serde_json::{Value, json};

/// Answers every question; `layers` layer questions get yes.
fn answers(layers: usize, r2: u8, r3: u8, r4: u8, r6: u8) -> Value {
    let mut a = json!({});
    for (i, id) in pm_llm::rubric::LAYERS.iter().enumerate() {
        a[id] = json!({"type": "noul", "noul": if i < layers { 0.9 } else { 0.1 }});
    }
    let score = |l: u8| json!({"type": "score", "score": l, "probabilities": {l.to_string(): 1.0}});
    a["r2_data_model"] = score(r2);
    a["r3_contract"] = score(r3);
    a["r4_algorithmic"] = score(r4);
    a["r6_verification"] = score(r6);
    a["r5_app_wide"] = json!({"type": "noul", "noul": 0.2});
    json!({"model": "fake-1", "answers": a, "confidence_source": "none"})
}

/// A classifier that logs each request and answers by ticket; fails while `fail` exists.
fn fake_classifier(dir: &Path) -> String {
    std::fs::write(dir.join("small.json"), answers(1, 0, 1, 1, 1).to_string()).unwrap();
    std::fs::write(dir.join("big.json"), answers(3, 2, 2, 2, 3).to_string()).unwrap();
    let script = dir.join("classify.sh");
    std::fs::write(
        &script,
        r#"#!/bin/sh
dir="$(dirname "$0")"
input=$(cat)
echo x >> "$dir/calls"
printf '%s' "$input" > "$dir/input-$$"
if [ -f "$dir/fail" ]; then echo "usage limit reached" >&2; exit 1; fi
case "$input" in *SHOP-2*) cat "$dir/big.json";; *) cat "$dir/small.json";; esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script.display().to_string()
}

fn calls(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("calls")).map_or(0, |s| s.lines().count())
}

fn options() -> CollectOptions {
    let mut o = CollectOptions::new(d("2025-03-31"));
    o.fetch = false;
    o.snapshots = false;
    o
}

fn monthly(p: &pm_metrics::ProjectFile, metric: &str, month: &str) -> Option<f64> {
    let i = p.months.iter().position(|m| m == month)?;
    p.series_monthly.get(metric)?[i]
}

#[test]
fn scores_packages_and_computes_velocity() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = fake_classifier(&bin);

    let api = Repo::init(root.join("api"));
    api.write("src/customer.ts", &lines("C", 0..20));
    api.commit("2025-02-03", JANE, "SHOP-1 add customer nickname");
    api.write("tests/customer.test.ts", &lines("T", 0..10));
    api.commit("2025-02-05", JANE, "SHOP-1 test the nickname");
    api.write("src/x.ts", &lines("X", 0..5));
    api.commit("2025-02-11", BOB, "fix typo");
    api.write("src/y.ts", &lines("Y", 0..5));
    api.commit("2025-02-12", BOB, "tweak");
    api.write("package-lock.json", "{}\n");
    api.commit("2025-02-12", BOT, "bump deps");
    api.write("src/orders/rule.ts", &lines("R", 0..30));
    api.write("db/migrations/001_orders.sql", "CREATE TABLE orders_audit (id int);\n");
    api.commit("2025-02-20", JANE, "SHOP-2 order rule");
    let vault = Repo::init(root.join("vault"));
    vault.write("src/keys.ts", &lines("K", 0..10));
    vault.commit("2025-01-15", JANE, "VLT-1 key rotation");

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

[[repo]]
path = "vault"
branch = "main"
role = "backend"
classifier = "remote"
data_policy = "local-only"

[fte]
source = "static"
value = 2.0

[classifiers.fake]
provider = "command"
command = ["sh", "{script}"]
jobs = 2

[classifiers.remote]
provider = "claude-code"
command = ["false"]

[velocity]
classifier = "fake"
"#
    );
    std::fs::write(root.join("pmx.toml"), toml).unwrap();
    let ws = Workspace::load(root).unwrap();

    // The classifier fails every call: nothing is scored, and Velocity is empty (not low).
    std::fs::write(bin.join("fail"), "").unwrap();
    let c = collect(&ws, &options()).unwrap();
    assert!(
        c.warnings
            .iter()
            .any(|w| w.contains("classifier `fake`: 3 package(s) failed") && w.contains("usage limit")),
        "{:?}",
        c.warnings
    );
    assert_eq!(calls(&bin), 3);
    assert_eq!(monthly(&c.project, "velocity", "2025-02"), None);
    assert_eq!(monthly(&c.project, "velocity_packages", "2025-02"), Some(3.0));

    // It works again: the three are scored.
    std::fs::remove_file(bin.join("fail")).unwrap();
    let c = collect(&ws, &options()).unwrap();
    assert_eq!(calls(&bin), 6);
    let p = &c.project;
    let v = p.velocity.as_ref().expect("measured");
    assert_eq!(v.rubric, "six-axis@2");
    assert!(!p.meta.not_measured.contains(&"velocity".to_string()));
    let ids: Vec<&str> = v.packages.iter().map(|p| p.id.as_str()).collect();
    // Newest first; Bob's untracked week is pseudonymized (Bob Builder = p1, Jane Doe = p2).
    assert_eq!(ids, ["SHOP-2", "p1~2025-W07", "SHOP-1", "VLT-1"]);
    let pkg = |id: &str| v.packages.iter().find(|p| p.id == id).unwrap();
    // SHOP-1: one layer → R1 0; src + tests are two modules → R5 2; sum 5 → S, 2 points.
    assert_eq!(pkg("SHOP-1").axes, Some([0, 0, 1, 1, 2, 1]));
    assert_eq!(pkg("SHOP-1").points, Some(2));
    assert!(pkg("SHOP-1").tests);
    assert_eq!(pkg("SHOP-1").commits, 2);
    assert_eq!(pkg("SHOP-1").date, d("2025-02-05"));
    // Two files in one module → R5 1; sum 4 → 2 points. The bot's commit is not in any package.
    assert_eq!(pkg("p1~2025-W07").axes, Some([0, 0, 1, 1, 1, 1]));
    assert_eq!(pkg("p1~2025-W07").commits, 2);
    assert_eq!(pkg("p1~2025-W07").description, "fix typo");
    // SHOP-2: three layers → 2; the migration is not counted as a file → R5 0; 11 → L, 5 points,
    // all verification debt (R6 3, no test).
    assert_eq!(pkg("SHOP-2").axes, Some([2, 2, 2, 2, 0, 3]));
    assert_eq!(
        (pkg("SHOP-2").size.as_deref(), pkg("SHOP-2").points),
        (Some("L"), Some(5))
    );
    assert_eq!(
        pkg("SHOP-2").classifier.as_deref().map(|c| c.starts_with("command:")),
        Some(true)
    );
    assert_eq!(pkg("SHOP-2").model.as_deref(), Some("fake-1"));
    // The local-only repo's classifier is remote: never sent, not scored.
    assert_eq!(pkg("VLT-1").points, None);
    assert!(c.warnings.iter().any(|w| w.contains("local-only")), "{:?}", c.warnings);

    // Feb: 9 points over 20 business days × 2 FTE.
    assert_eq!(monthly(p, "velocity_points", "2025-02"), Some(9.0));
    assert_eq!(monthly(p, "velocity", "2025-02"), Some(9.0 / 40.0));
    assert_eq!(monthly(p, "velocity_approx", "2025-02"), Some(0.0));
    assert_eq!(monthly(p, "verification_debt_pts", "2025-02"), Some(5.0));
    assert_eq!(monthly(p, "points_per_commit", "2025-02"), Some(9.0 / 5.0));
    // January holds the refused package: no Velocity there. March has no packages: 0.
    assert_eq!(monthly(p, "velocity", "2025-01"), None);
    assert_eq!(monthly(p, "velocity", "2025-03"), Some(0.0));

    // What was sent: no names, no migration, the diff of the rule.
    let sent: Vec<Value> = std::fs::read_dir(&bin)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("input-"))
        .map(|p| serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap())
        .collect();
    let shop2 = sent
        .iter()
        .find(|s| s["state"].as_str().unwrap().contains("SHOP-2"))
        .unwrap();
    let state: Value = serde_json::from_str(shop2["state"].as_str().unwrap()).unwrap();
    assert_eq!(state["ticket"], "SHOP-2");
    assert_eq!(state["generated_files_excluded"], 1);
    assert_eq!(state["files"].as_array().unwrap().len(), 1);
    let diff = state["diff"].as_str().unwrap();
    assert!(diff.contains("+export pub const R0 = 0;"));
    assert!(!diff.contains("CREATE TABLE"));
    assert!(shop2["questions"]["r2_data_model"]["criteria"].is_array());
    assert!(shop2["context"].as_str().unwrap().contains("work package"));
    for s in &sent {
        let text = s["state"].as_str().unwrap();
        assert!(
            !text.contains("Jane") && !text.contains("Bob"),
            "no names in the state: {text}"
        );
    }

    // Nothing changed: every answer comes from the cache, even with --full.
    let mut full = options();
    full.incremental = false;
    collect(&ws, &full).unwrap();
    assert_eq!(calls(&bin), 6);

    // A new commit on SHOP-1 changes that package only; it moves to March.
    api.write("src/customer.ts", &lines("C", 0..25));
    api.commit("2025-03-03", JANE, "SHOP-1 validate the nickname");
    let c = collect(&ws, &options()).unwrap();
    assert_eq!(calls(&bin), 7);
    assert_eq!(monthly(&c.project, "velocity_points", "2025-02"), Some(7.0));
    assert_eq!(monthly(&c.project, "velocity_points", "2025-03"), Some(2.0));

    // --no-velocity: not measured in that run.
    let mut off = options();
    off.velocity = false;
    let c = collect(&ws, &off).unwrap();
    assert!(c.project.velocity.is_none());
    assert!(c.project.meta.not_measured.contains(&"velocity".to_string()));
    assert!(!c.project.series_monthly.contains_key("velocity"));
}

#[test]
fn not_measured_without_a_classifier() {
    let tmp = tempfile::tempdir().unwrap();
    let api = Repo::init(tmp.path().join("api"));
    api.write("src/a.ts", &lines("A", 0..5));
    api.commit("2025-02-03", JANE, "SHOP-1 a");
    std::fs::write(
        tmp.path().join("pmx.toml"),
        "[project]\nname = \"F\"\nrange_start = \"2025-01-01\"\nbreadth_roles = [\"backend\"]\n\
         [[repo]]\npath = \"api\"\nbranch = \"main\"\nrole = \"backend\"\n",
    )
    .unwrap();
    let ws = Workspace::load(tmp.path()).unwrap();
    let c = collect(&ws, &options()).unwrap();
    assert!(c.project.velocity.is_none());
    assert!(c.project.meta.not_measured.contains(&"velocity".to_string()));
    assert!(!c.project.series_monthly.contains_key("velocity"));
}
