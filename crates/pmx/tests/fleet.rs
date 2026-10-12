//! The local fleet (M6): `fleet.toml`, pooling across projects, the fleet page, and the WASM
//! engine reproducing `fleet.json`. Everything is fictional.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::{BOB, JANE, Repo, d, lines};
use pm_config::{Config, Workspace};
use pm_metrics::{FleetFile, ProjectFile};
use pmx::{CollectOptions, collect, dashboard, write_outputs};

/// A project of two repos; `ai` = whether it attributes AI.
fn project(root: &Path, name: &str, ai: bool, as_of: &str) -> PathBuf {
    let api = Repo::init(root.join("api"));
    let web = Repo::init(root.join("web"));
    for month in 1..=6 {
        for (k, who) in [JANE, BOB].into_iter().enumerate() {
            let date = format!("2025-{month:02}-{:02}", 5 + 7 * k);
            let n = 30 + 11 * ((month * 3 + k) % 7);
            api.write(
                &format!("src/{name}{k}{month}.ts"),
                &lines(&format!("a{k}x{month}"), 0..n),
            );
            if k == 0 {
                api.write(&format!("src/{name}{k}{month}.test.ts"), &lines("t", 0..n / 2));
            }
            api.commit(&date, who, "api work\n\nCo-Authored-By: Claude <noreply@anthropic.com>");
            web.write(
                &format!("src/C{k}{month}.tsx"),
                &lines(&format!("w{k}x{month}"), 0..n + 45),
            );
            web.commit(&date, who, "ui");
        }
    }
    let text = format!(
        r#"
        [project]
        name = "{name}"
        range_start = "2025-01-01"
        breadth_roles = ["frontend", "backend"]
        ai_attribution = {ai}
        [[repo]]
        path = "{}"
        branch = "main"
        role = "backend"
        [[repo]]
        path = "{}"
        branch = "main"
        role = "frontend"
        "#,
        root.join("api").display(),
        root.join("web").display()
    );
    let ws = Workspace {
        root: root.join("ws"),
        config: Config::parse(&text, Path::new("pmx.toml")).unwrap(),
    };
    std::fs::create_dir_all(&ws.root).unwrap();
    std::fs::write(ws.root.join("pmx.toml"), ws.config.to_toml()).unwrap();
    let mut o = CollectOptions::new(d(as_of));
    o.fetch = false;
    o.snapshots = false; // covered by tests/snapshots.rs; pooling of snapshots by pm-metrics tests
    let c = collect(&ws, &o).unwrap();
    write_outputs(&ws.out_dir(), &c).unwrap();
    ws.root
}

struct Fleet {
    _tmp: tempfile::TempDir,
    dir: PathBuf,
    shop: PathBuf,
    crm: PathBuf,
}

/// Shop (AI attributed) and CRM (trailers stripped, collected a month earlier).
fn fleet() -> Fleet {
    let tmp = tempfile::tempdir().unwrap();
    let shop = project(&tmp.path().join("shop"), "Shop", true, "2025-06-30");
    let crm = project(&tmp.path().join("crm"), "CRM", false, "2025-05-31");
    let dir = tmp.path().join("fleet");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("fleet.toml"),
        "name = \"Acme <Engineering>\"\nprojects = [\"../shop/ws\", \"../crm/ws\"]\n",
    )
    .unwrap();
    Fleet {
        _tmp: tmp,
        dir,
        shop,
        crm,
    }
}

fn pmx(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_pmx"))
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap()
}

fn read<T: serde::de::DeserializeOwned>(p: &Path) -> T {
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

#[test]
fn exports_the_pooled_fleet_and_one_shareable_page() {
    let f = fleet();
    let o = pmx(&f.dir, &["fleet", "export"]);
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(o.status.success(), "{stderr}");
    assert!(stderr.contains("CRM was collected as of 2025-05-31"), "{stderr}");

    let fleet: FleetFile = read(&f.dir.join("out/fleet.json"));
    let shop: ProjectFile = read(&f.shop.join("out/project.json"));
    let crm: ProjectFile = read(&f.crm.join("out/project.json"));
    assert_eq!(fleet.name, "Acme <Engineering>");
    assert_eq!(fleet.as_of, d("2025-06-30"));
    let keys: Vec<(&str, bool)> = fleet.projects.iter().map(|p| (p.key.as_str(), p.stale)).collect();
    // Both folders are called `ws`: keys stay unique.
    assert_eq!(keys, [("ws", false), ("ws-2", true)]);

    // Additive counts pool exactly; ratios pool numerators and denominators.
    let at = |p: &[String], l: &str| p.iter().position(|x| x == l).unwrap();
    for month in ["2025-02", "2025-05"] {
        let (i, s, c) = (
            at(&fleet.months, month),
            at(&shop.months, month),
            at(&crm.months, month),
        );
        let v =
            |series: &std::collections::BTreeMap<String, Vec<Option<f64>>>, id: &str, i: usize| series[id][i].unwrap();
        let n =
            |series: &std::collections::BTreeMap<String, Vec<Option<u64>>>, id: &str, i: usize| series[id][i].unwrap();
        assert_eq!(
            v(&fleet.series_monthly, "commits", i),
            v(&shop.series_monthly, "commits", s) + v(&crm.series_monthly, "commits", c)
        );
        assert_eq!(
            v(&fleet.series_monthly, "active_devs", i),
            4.0,
            "Jane in shop ≠ Jane in CRM"
        );
        let prod = n(&shop.n_monthly, "test_discipline_pct", s) + n(&crm.n_monthly, "test_discipline_pct", c);
        assert_eq!(n(&fleet.n_monthly, "test_discipline_pct", i), prod);
        // AI-assisted only from the project that attributes it.
        assert_eq!(
            v(&fleet.series_monthly, "ai_assist_commit_pct", i),
            v(&shop.series_monthly, "ai_assist_commit_pct", s)
        );
        assert_eq!(fleet.coverage_monthly[i].fed["ai"], ["ws"]);
    }
    assert!(
        !fleet.series_monthly.contains_key("velocity"),
        "no project scores Velocity"
    );

    // One page, every project in it, no people.
    let page = std::fs::read_to_string(f.dir.join("out/fleet.html")).unwrap();
    assert!(page.contains("<title>Acme &lt;Engineering&gt; · pmx fleet</title>"));
    assert!(
        !page.contains("Jane Doe") && !page.contains("Bob Builder"),
        "fleet pages are shareable"
    );
    let open = r#"<script id="pmx-data" type="application/json">"#;
    let start = page.find(open).unwrap() + open.len();
    let data: serde_json::Value =
        serde_json::from_str(&page[start..start + page[start..].find("</script>").unwrap()]).unwrap();
    assert_eq!(data["fleet"]["projects"].as_array().unwrap().len(), 2);
    assert_eq!(data["projects"]["ws-2"]["project"]["name"], "CRM");
    assert!(data["leads"].is_null());
}

#[test]
fn the_leads_copy_names_people_and_leaves_the_shared_page_alone() {
    let f = fleet();
    assert!(pmx(&f.dir, &["fleet", "export", "--with-private"]).status.success());
    assert!(
        !f.dir.join("out/fleet.html").exists(),
        "the private export writes only its own copy"
    );
    let page = std::fs::read_to_string(f.dir.join("out/private/fleet.html")).unwrap();
    let open = r#"<script id="pmx-data" type="application/json">"#;
    let start = page.find(open).unwrap() + open.len();
    let data: serde_json::Value =
        serde_json::from_str(&page[start..start + page[start..].find("</script>").unwrap()]).unwrap();
    assert_eq!(data["leads"]["ws"]["schema"], "pmx.leads/1");
    assert_eq!(data["leads"]["ws-2"]["schema"], "pmx.leads/1");
    assert!(page.contains("Jane Doe"), "the lead's copy names people");
}

#[test]
fn reports_a_missing_project() {
    let f = fleet();
    std::fs::remove_file(f.crm.join("out/project.json")).unwrap();
    let o = pmx(&f.dir, &["fleet", "export"]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("collect` first"));
}

#[test]
fn engine_reproduces_every_fleet_bucket() {
    if !dashboard::has_engine() {
        eprintln!("skipped: pmx was built without the WASM engine");
        return;
    }
    let f = fleet();
    assert!(pmx(&f.dir, &["fleet", "export"]).status.success());
    let wasm = f.dir.join("pm_wasm.wasm");
    std::fs::write(&wasm, dashboard::engine_wasm()).unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("web/scripts/check-fleet-engine.mjs");
    let out = match Command::new("node")
        .arg(&script)
        .arg(&wasm)
        .arg(f.dir.join("out/fleet.json"))
        .arg(format!("ws={}", f.shop.join("out/project.json").display()))
        .arg(format!("ws-2={}", f.crm.join("out/project.json").display()))
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            assert!(std::env::var_os("PMX_REQUIRE_NODE").is_none(), "node is required: {e}");
            eprintln!("skipped: no node ({e})");
            return;
        }
    };
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}{}", String::from_utf8_lossy(&out.stderr));
    assert!(text.contains(", 0 differ"), "{text}");
}

#[test]
fn serves_the_fleet_page() {
    let f = fleet();
    let manifest = f.dir.join("fleet.toml");
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        dashboard::serve_page(listener, &|| {
            pmx::fleet::Fleet::load(&manifest, "2025-07-01T00:00:00Z".into())?.render()
        })
    });
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(addr).unwrap();
    write!(s, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    let mut body = String::new();
    s.read_to_string(&mut body).unwrap();
    assert!(body.starts_with("HTTP/1.1 200 OK"));
    assert!(body.contains("\"fleet\":"));
}
