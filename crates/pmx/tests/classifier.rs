//! The confidence gate and fallback inside `collect`, per-person points in `leads.json`, and
//! `classifier eval` against a fictional reference set, all with fake `command` classifiers.
#![cfg(unix)]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use common::{BOB, JANE, Repo, d, lines};
use pm_config::Workspace;
use pmx::{CollectOptions, collect};
use serde_json::{Value, json};

/// A full answer: every score question at `level` with confidence `conf` (Jev-like).
fn answers(level: u8, conf: f64) -> Value {
    let mut a = json!({});
    for id in pm_llm::rubric::LAYERS {
        a[id] = json!({"type": "noul", "noul": 0.05});
    }
    a["layer_service"] = json!({"type": "noul", "noul": 0.95});
    let mut p = json!({"0": 0.0, "1": 0.0, "2": 0.0, "3": 0.0});
    p[level.to_string()] = json!(conf);
    for id in ["r2_data_model", "r3_contract", "r4_algorithmic", "r6_verification"] {
        a[id] = json!({"type": "score", "score": level, "probabilities": p, "confidence": conf});
    }
    a["r5_app_wide"] = json!({"type": "noul", "noul": 0.05});
    json!({"model": format!("fake-{level}-{conf}"), "answers": a, "confidence_source": "model"})
}

/// A classifier script answering `response` (or, per ticket, from `by_ticket`) and logging calls.
fn script(dir: &Path, name: &str, response: &Value, by_ticket: &[(&str, Value)]) -> String {
    std::fs::write(dir.join(format!("{name}.json")), response.to_string()).unwrap();
    let mut cases = String::new();
    for (t, v) in by_ticket {
        std::fs::write(dir.join(format!("{name}-{t}.json")), v.to_string()).unwrap();
        cases.push_str(&format!("  *{t}*) cat \"$dir/{name}-{t}.json\";;\n"));
    }
    let path = dir.join(format!("{name}.sh"));
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\ndir=\"$(dirname \"$0\")\"\ninput=$(cat)\necho x >> \"$dir/{name}.calls\"\ncase \"$input\" in\n{cases}  *) cat \"$dir/{name}.json\";;\nesac\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.display().to_string()
}

fn calls(dir: &Path, name: &str) -> usize {
    std::fs::read_to_string(dir.join(format!("{name}.calls"))).map_or(0, |s| s.lines().count())
}

fn repo(root: &Path) -> Repo {
    let api = Repo::init(root.join("api"));
    api.write("src/a.ts", &lines("A", 0..20));
    api.commit("2025-02-03", JANE, "SHOP-1 add a");
    api.write("src/b.ts", &lines("B", 0..20));
    api.commit("2025-02-04", BOB, "SHOP-1 add b");
    api.write("src/c.ts", &lines("C", 0..20));
    api.commit("2025-02-10", JANE, "SHOP-2 add c");
    api.write("src/d.ts", &lines("D", 0..20));
    api.commit("2025-02-17", BOB, "SHOP-3 add d");
    api
}

fn options() -> CollectOptions {
    let mut o = CollectOptions::new(d("2025-03-31"));
    o.fetch = false;
    o.snapshots = false;
    o
}

const PROJECT: &str = r#"
[project]
name = "Fixture"
range_start = "2025-01-01"
breadth_roles = ["backend"]

[[repo]]
path = "api"
branch = "main"
role = "backend"
"#;

#[test]
fn gate_reasks_then_escalates_and_leads_get_points() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    repo(root);
    // Unsure about everything, except sure about SHOP-3.
    let unsure = script(&bin, "unsure", &answers(1, 0.4), &[("SHOP-3", answers(2, 0.9))]);
    let sure = script(&bin, "sure", &answers(2, 0.95), &[]);
    std::fs::write(
        root.join("pmx.toml"),
        format!(
            r#"{PROJECT}
[classifiers.jevlike]
provider = "command"
command = ["sh", "{unsure}"]
confidence_gate = 0.5
fallback = "big"

[classifiers.big]
provider = "command"
command = ["sh", "{sure}"]

[velocity]
classifier = "jevlike"
"#
        ),
    )
    .unwrap();
    let ws = Workspace::load(root).unwrap();
    let c = collect(&ws, &options()).unwrap();
    let v = c.project.velocity.as_ref().unwrap();
    let pkg = |id: &str| v.packages.iter().find(|p| p.id == id).unwrap();
    // SHOP-1 and SHOP-2: asked, re-asked with the levels reversed, then the fallback answered.
    assert!(pkg("SHOP-1").escalated && !pkg("SHOP-1").low_confidence);
    assert_eq!(
        pkg("SHOP-1").classifier.as_deref().unwrap(),
        format!("command:sh {sure}:-")
    );
    assert_eq!(pkg("SHOP-1").model.as_deref(), Some("fake-2-0.95"));
    // SHOP-3: sure the first time.
    assert!(!pkg("SHOP-3").escalated);
    assert_eq!(calls(&bin, "unsure"), 2 + 2 + 1);
    assert_eq!(calls(&bin, "sure"), 2);
    assert_eq!(
        (v.health.packages, v.health.escalated, v.health.low_confidence),
        (3, 2, 0)
    );
    assert!(
        c.warnings.iter().any(|w| w.contains("mixes classifiers")),
        "{:?}",
        c.warnings
    );

    // Per-person points in the lead file only: SHOP-1 (2 commits) splits between Jane and Bob.
    // One layer → R1 0, levels 2 → R2 R3 R4 R6 = 2. SHOP-1 has two files in one module (R5 1):
    // sum 9 → L, 5 points; SHOP-2 and SHOP-3 one file each: sum 8 → M, 3.
    let feb = &c.leads.points["2025-02"];
    assert_eq!(feb["Jane Doe"], 2.5 + 3.0);
    assert_eq!(feb["Bob Builder"], 2.5 + 3.0);
    let shared = serde_json::to_string(&c.project).unwrap();
    assert!(
        !shared.contains("Jane") && !shared.contains("Bob"),
        "no names in project.json"
    );

    // Cached: no calls the second time, same outcome.
    collect(&ws, &options()).unwrap();
    assert_eq!(calls(&bin, "unsure"), 5);

    // The fallback is not allowed for a local-only repo's packages when it isn't local: the first
    // answer is kept, flagged. (`command` counts as local, so make the fallback remote.)
    let mut ws = ws;
    ws.config.repos[0].data_policy = Some(pm_config::DataPolicy::LocalOnly);
    ws.config.classifiers.get_mut("big").unwrap().provider = pm_config::Provider::ClaudeCode;
    ws.config.classifiers.get_mut("big").unwrap().command = Some(vec!["false".into()]);
    let c = collect(&ws, &options()).unwrap();
    let v = c.project.velocity.as_ref().unwrap();
    assert_eq!(v.health.low_confidence, 2);
    assert_eq!(v.health.escalated, 0);
}

#[test]
fn eval_compares_with_the_reference() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    repo(root);
    let plain = |level: u8| {
        let mut a = answers(level, 1.0);
        a["confidence_source"] = json!("none");
        a
    };
    let cand = script(&bin, "cand", &plain(1), &[("SHOP-2", plain(2)), ("SHOP-3", plain(0))]);
    std::fs::write(
        root.join("pmx.toml"),
        format!("{PROJECT}\n[classifiers.cand]\nprovider = \"command\"\ncommand = [\"sh\", \"{cand}\"]\n"),
    )
    .unwrap();
    let ws = Workspace::load(root).unwrap();
    let reference = root.join("reference.csv");
    std::fs::write(
        &reference,
        "project,id,R1,R2,R3,R4,R5,R6\n\
         Fixture,SHOP-1,0,1,1,1,0,1\n\
         Fixture,SHOP-2,0,2,2,1,0,2\n\
         Fixture,SHOP-3,0,0,0,0,0,0\n\
         Fixture,NOPE-1,1,1,1,1,1,1\n\
         Other,SHOP-1,3,3,3,3,3,3\n",
    )
    .unwrap();
    let refs = pmx::eval::read_reference(&reference).unwrap();
    let opts = pmx::eval::EvalOptions {
        classifier: "cand".into(),
        as_of: d("2025-03-31"),
        runs: 2,
        use_cache: true,
    };
    let r = pmx::eval::eval(&ws, &refs, &opts).unwrap();
    assert_eq!(r.reference, 4);
    assert_eq!(r.missing, ["NOPE-1"]);
    assert_eq!(r.scored.len(), 3);
    assert_eq!(calls(&bin, "cand"), 6, "two runs of three");
    // R2: reference 1, 2, 0; candidate 1, 2, 0 → perfect.
    assert_eq!(r.kappa(1), Some(1.0));
    // R4: reference 1, 1, 0; candidate 1, 2, 0 → off by one once.
    assert!(r.kappa(3).unwrap() < 1.0);
    // R1 and R5 are 0 everywhere: undefined.
    assert_eq!(r.kappa(0), None);
    // Points: reference S 2 + M 3 + XS 1 = 6 vs candidate S 2 + M 3 + XS 1 = 6.
    assert_eq!(r.points(), (6, 6));
    assert_eq!(r.retest_pct(), Some(100.0));
    let text = pmx::eval::render(&r);
    assert!(
        text.contains("3 scored of 4") && text.contains("FAIL: only 3 packages"),
        "{text}"
    );

    // The first run reads the cache; the retest never does.
    pmx::eval::eval(&ws, &refs, &opts).unwrap();
    assert_eq!(calls(&bin, "cand"), 9);
}
