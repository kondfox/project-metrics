//! The setup flow through the real binary: init → people → repo → check → collect → export, and
//! import-config. Everything is fictional.

mod common;

use std::path::Path;
use std::process::{Command, Output};

use common::{BOB, JANE, JANE_HOME, Repo, lines};

const BOB_GH: (&str, &str) = ("bobbuilder", "12345+bobbuilder@users.noreply.github.com");

fn pmx(ws: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pmx"))
        .arg("-C")
        .arg(ws)
        .args(args)
        .output()
        .unwrap()
}

fn ok(ws: &Path, args: &[&str]) -> String {
    let out = pmx(ws, args);
    assert!(
        out.status.success(),
        "pmx {args:?} failed:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn config(ws: &Path) -> String {
    std::fs::read_to_string(ws.join("pmx.toml")).unwrap()
}

fn build(root: &Path) {
    let web = Repo::init(root.join("src/shop-web"));
    web.write("vite.config.ts", "export default {};\n")
        .write("src/App.tsx", &lines("app", 0..50))
        .write("src/api.ts", &lines("api", 0..20));
    web.commit("2025-01-07", JANE, "web shell");
    web.write("src/Cart.tsx", &lines("cart", 0..45));
    web.commit("2025-02-11", JANE_HOME, "cart");

    let api = Repo::init(root.join("src/shop-api"));
    api.write("go.mod", "module example.com/shop\n")
        .write("main.go", &lines("main", 0..60))
        .write("main_test.go", &lines("t", 0..12));
    api.commit("2025-01-08", BOB, "api");
    api.write("orders.go", &lines("orders", 0..40));
    api.commit("2025-02-12", BOB_GH, "orders");
    api.write("users.go", &lines("users", 0..41));
    api.commit("2025-02-13", JANE, "users");

    let docs = Repo::init(root.join("upstream/handbook"));
    docs.write("README.md", "# Handbook\n")
        .write("docs/onboarding.md", "Welcome.\n");
    docs.commit("2025-01-09", JANE, "handbook");
}

#[test]
fn setup_flow() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    build(root);
    let ws = root.join("ws");
    let src = root.join("src");

    let out = ok(
        &ws,
        &[
            "init",
            src.to_str().unwrap(),
            "--yes",
            "--name",
            "Acme Shop",
            "--since",
            "2025-01-01",
        ],
    );
    assert!(out.contains("2 repos found"), "{out}");
    let toml = config(&ws);
    assert!(toml.contains("name = \"Acme Shop\""), "{toml}");
    assert!(
        toml.contains("role = \"frontend\"") && toml.contains("role = \"backend\""),
        "{toml}"
    );
    assert!(toml.contains("branch = \"main\""));
    assert!(
        std::fs::read_to_string(ws.join(".gitignore"))
            .unwrap()
            .contains(".pmx/")
    );
    assert!(
        !pmx(&ws, &["init", src.to_str().unwrap(), "--yes"]).status.success(),
        "init must not overwrite"
    );

    // Nobody is in [people] yet.
    let out = pmx(&ws, &["check", "--threshold", "1"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("not in [people]"));

    let out = ok(&ws, &["people", "--apply"]);
    assert!(
        out.contains("+ Jane Doe <jane@work.example>") || out.contains("Jane Doe"),
        "{out}"
    );
    let cfg = pm_config::Workspace::load(&ws).unwrap().config;
    let jane = &cfg.people.persons["Jane Doe"];
    assert!(jane.contains(&"jane@work.example".to_string()));
    let bob = &cfg.people.persons["Bob Builder"];
    assert!(
        bob.contains(&BOB_GH.1.to_string()),
        "noreply login merged into Bob: {bob:?}"
    );
    // "jane" <jane@home.example> has nothing in common with Jane Doe: merge it by hand.
    ok(&ws, &["people", "merge", "Jane Doe", "jane@home.example"]);
    let cfg = pm_config::Workspace::load(&ws).unwrap().config;
    assert_eq!(cfg.people.persons["Jane Doe"].len(), 2);
    assert!(!cfg.people.persons.contains_key("jane"));
    ok(&ws, &["people", "external", "Pat Vendor"]);
    assert!(config(&ws).contains("Pat Vendor"));

    // A URL repo: pmx keeps its own clone.
    let url = format!("file://{}", root.join("upstream/handbook").display());
    ok(&ws, &["repo", "add", &url]);
    assert!(ws.join(".pmx/repos/handbook.git").exists());
    let list = ok(&ws, &["repo", "list"]);
    assert!(list.contains("handbook") && list.contains("docs"), "{list}");
    ok(&ws, &["repo", "set", "handbook", "--role", "qa"]);
    assert!(config(&ws).contains("role = \"qa\""));
    assert!(
        !pmx(&ws, &["repo", "set", "handbook", "--role", "chef"])
            .status
            .success()
    );
    ok(&ws, &["repo", "remove", "handbook"]);
    assert!(!config(&ws).contains("handbook"));

    let out = ok(&ws, &["check", "--threshold", "1"]);
    assert!(out.contains("0 error(s)"), "{out}");

    let plan = ok(&ws, &["plan", "--no-fetch", "--as-of", "2025-03-31"]);
    assert!(plan.contains("git ingest") && plan.contains("shop-api"), "{plan}");

    let events = ok(
        &ws,
        &[
            "collect",
            "--no-fetch",
            "--no-snapshots",
            "--as-of",
            "2025-03-31",
            "--progress",
            "json",
        ],
    );
    let events: Vec<serde_json::Value> = events.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(events.first().unwrap()["event"], "plan");
    let done = events.last().unwrap();
    assert_eq!(done["event"], "done");
    assert_eq!(done["ok"], true);
    assert!(ws.join("out/project.json").exists() && ws.join("out/private/leads.json").exists());

    let csv = ok(&ws, &["export", "--format", "long", "--monthly"]);
    assert!(csv.contains("acme-shop,commits,2025-01,2,"), "{csv}");
    assert!(csv.contains("acme-shop,commits,2025-02,3,"), "{csv}");
    // February: Jane frontend 45 + backend 41 lines (multi-stack), Bob backend 40.
    assert!(csv.contains("acme-shop,multi_stack_pct,2025-02,50,"), "{csv}");

    // A second run is served from the cache.
    let plan = ok(&ws, &["plan", "--no-fetch", "--as-of", "2025-03-31"]);
    assert!(plan.contains("ingest: cached · rework: cached"), "{plan}");
}

#[test]
fn import_config() {
    let tmp = tempfile::tempdir().unwrap();
    let proto = tmp.path().join("collector");
    std::fs::create_dir_all(proto.join("../tools")).unwrap();
    std::fs::write(
        proto.join("config_acme.json"),
        r#"{
            "project": "Acme Shop", "workspace": "/src/acme", "range_start": "2025-01-01",
            "state_window": 90, "breadth_roles": ["frontend", "backend"],
            "repos": [{"name": "shop-api", "branch": "origin/main", "role": "backend"}],
            "identity": {"__bots__": ["ci@example.com"], "jane@example.com": "Jane Doe"}
        }"#,
    )
    .unwrap();
    std::fs::write(
        proto.join("../tools/externals.json"),
        r#"{"Acme Shop": [{"name": "Sam Contractor"}]}"#,
    )
    .unwrap();
    std::fs::write(proto.join("../tools/fte.json"), r#"{"Acme Shop": 3.5}"#).unwrap();
    std::fs::write(
        proto.join("../tools/secrets_triage.json"),
        r#"{"entries": [{"repo": "shop-api", "file": "certs/dev.pem", "verdict": "rotated", "by": "Jane Doe", "date": "2026-10-08"}]}"#,
    )
    .unwrap();
    let ws = tmp.path().join("ws");
    let out = pmx(
        &ws,
        &["import-config", proto.join("config_acme.json").to_str().unwrap()],
    );
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stderr).contains("state_window"));
    let cfg = pm_config::Workspace::load(&ws).unwrap().config;
    assert_eq!(cfg.repos[0].path.as_deref(), Some("/src/acme/shop-api"));
    assert_eq!(cfg.people.externals, vec!["Sam Contractor"]);
    assert_eq!(cfg.people.bots, vec!["ci@example.com"]);
    assert_eq!(cfg.fte.unwrap().value, Some(3.5));
    assert_eq!(cfg.secrets_triage.len(), 1);
    assert!(
        !pmx(
            &ws,
            &["import-config", proto.join("config_acme.json").to_str().unwrap()]
        )
        .status
        .success()
    );
}
