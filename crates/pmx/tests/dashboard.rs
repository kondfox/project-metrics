//! The dashboard (M2): rendering, the privacy split, `pmx serve`, `export --format html`, and the
//! WASM engine reproducing the CLI's numbers exactly. Everything is fictional.

mod common;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;

use common::{BOB, JANE, Repo, d, lines};
use pm_config::{Config, Workspace};
use pmx::{CollectOptions, collect, dashboard, write_outputs};

/// A year of commits by three people across two stacks, so ratios, entropy and scores are all
/// exercised.
fn workspace(root: &Path) -> Workspace {
    let pat = ("Pat Lee", "pat@work.example");
    let api = Repo::init(root.join("api"));
    let web = Repo::init(root.join("web"));
    for (i, month) in (1..=12).enumerate() {
        for (k, who) in [JANE, BOB, pat].into_iter().enumerate() {
            if (i + k) % 3 == 2 {
                continue;
            }
            let date = format!("2025-{month:02}-{:02}", 3 + 7 * k);
            let n = 20 + 13 * ((i * 7 + k * 5) % 9);
            api.write(&format!("src/{}{i}.ts", k), &lines(&format!("a{k}x{i}"), 0..n));
            if k != 1 {
                api.write(&format!("src/{}{i}.test.ts", k), &lines(&format!("t{k}x{i}"), 0..n / 3));
            }
            api.commit(&date, who, "api work");
            if (i + k) % 2 == 0 {
                web.write(&format!("src/C{k}{i}.tsx"), &lines(&format!("w{k}x{i}"), 0..n + 25));
                let msg = if k == 0 {
                    "ui\n\nCo-Authored-By: Claude <noreply@anthropic.com>"
                } else {
                    "ui"
                };
                web.commit(&date, who, msg);
            }
        }
    }
    let text = format!(
        r#"
        [project]
        name = "Demo <Shop>"
        range_start = "2025-01-01"
        breadth_roles = ["frontend", "backend"]
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
    Workspace {
        root: root.join("ws"),
        config: Config::parse(&text, Path::new("pmx.toml")).unwrap(),
    }
}

fn collected(root: &Path) -> Workspace {
    let ws = workspace(root);
    let mut o = CollectOptions::new(d("2025-12-31"));
    o.fetch = false;
    let c = collect(&ws, &o).unwrap();
    write_outputs(&ws.out_dir(), &c).unwrap();
    ws
}

#[test]
fn renders_one_page_with_private_view_only_on_request() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = collected(tmp.path());
    let shared = dashboard::render_workspace(&ws.out_dir(), false).unwrap();
    assert!(shared.contains("<title>Demo &lt;Shop&gt; · pmx</title>"));
    assert!(shared.contains("\"leads\":null"));
    assert!(!shared.contains("Jane Doe"), "the shared page must not name people");
    let private = dashboard::render_workspace(&ws.out_dir(), true).unwrap();
    assert!(private.contains("Jane Doe"));
    // The project name is inside a JSON string in a <script>: it must not close the element.
    assert_eq!(shared.matches("</script>").count(), 3);
    if std::env::var_os("PMX_REQUIRE_NODE").is_some() {
        assert!(dashboard::has_engine(), "CI builds must embed the WASM engine");
    }
}

#[test]
fn engine_reproduces_every_bucket() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = collected(tmp.path());
    if !dashboard::has_engine() {
        eprintln!("skipped: pmx was built without the WASM engine");
        return;
    }
    let wasm = tmp.path().join("pm_wasm.wasm");
    std::fs::write(&wasm, dashboard::engine_wasm()).unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("web/check-engine.mjs");
    let out = match Command::new("node")
        .arg(&script)
        .arg(&wasm)
        .arg(ws.out_dir().join("project.json"))
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

fn get(addr: std::net::SocketAddr, path: &str) -> String {
    let mut s = TcpStream::connect(addr).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    let mut body = String::new();
    s.read_to_string(&mut body).unwrap();
    body
}

#[test]
fn serves_the_dashboard_locally() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = collected(tmp.path());
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    let out = ws.out_dir();
    std::thread::spawn(move || dashboard::serve(listener, &out));
    let page = get(addr, "/");
    assert!(page.starts_with("HTTP/1.1 200 OK"), "{}", &page[..80.min(page.len())]);
    assert!(page.contains("Jane Doe"), "serve shows the lead view");
    assert!(get(addr, "/etc/passwd").starts_with("HTTP/1.1 404"));
}

#[test]
fn exports_html_through_the_cli() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = collected(tmp.path());
    std::fs::write(ws.root.join("pmx.toml"), ws.config.to_toml()).unwrap();
    let pmx = |args: &[&str]| {
        let o = Command::new(env!("CARGO_BIN_EXE_pmx"))
            .arg("-C")
            .arg(&ws.root)
            .args(args)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stderr).into_owned()
    };
    pmx(&["export", "--format", "html"]);
    let shared = std::fs::read_to_string(ws.out_dir().join("dashboard.html")).unwrap();
    assert!(!shared.contains("Jane Doe"));
    let note = pmx(&["export", "--format", "html", "--with-private"]);
    assert!(note.contains("don't share"));
    assert!(
        std::fs::read_to_string(ws.out_dir().join("private/dashboard.html"))
            .unwrap()
            .contains("Jane Doe")
    );
}
