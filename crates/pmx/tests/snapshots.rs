//! The snapshot stage end to end with fake tools (CI has none of the real ones): tree export, the
//! per-tool cache, pooling, new/fixed, the Security score and "not measured". Unix only (the fakes
//! are shell scripts).
#![cfg(unix)]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use common::{JANE, Repo, d, lines};
use pm_config::{Config, Workspace};
use pm_snapshot::{Tool, ToolId, Tools};
use pmx::{CollectOptions, collect};

/// A fake tool: a script that answers from the tree it is given. `$TREE` is the last argument
/// (or the `dir` argument for gitleaks), so answers can depend on the commit's files.
fn fake(dir: &Path, name: &str, body: &str, id: ToolId) -> Tool {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    Tool {
        id,
        program: path,
        prefix: vec![],
        version: id.pinned().into(),
    }
}

fn tools(dir: &Path) -> Tools {
    // scc: code = number of lines in src/ (so it changes with the commit), complexity fixed.
    let scc = fake(
        dir,
        "scc",
        r#"for a; do T="$a"; done
if [ "$T" = "." ]; then T=$(pwd); fi
N=$(cat "$T"/src/*.ts 2>/dev/null | wc -l | tr -d ' ')
echo "[{\"Name\":\"TypeScript\",\"Code\":$N,\"Complexity\":10,\"Files\":[{\"Location\":\"./src/a.ts\",\"Code\":$N,\"Complexity\":10}]}]""#,
        ToolId::Scc,
    );
    // jscpd: writes its report into --output; 5 duplicated lines of 100.
    let jscpd = fake(
        dir,
        "jscpd",
        r#"while [ $# -gt 0 ]; do if [ "$1" = "--output" ]; then O="$2"; fi; shift; done
echo '{"statistics":{"total":{"lines":100,"duplicatedLines":5}}}' > "$O/jscpd-report.json""#,
        ToolId::Jscpd,
    );
    // gitleaks: one AWS key once config/prod.env exists.
    let gitleaks = fake(
        dir,
        "gitleaks",
        r#"T="$2"; while [ $# -gt 0 ]; do if [ "$1" = "-r" ]; then R="$2"; fi; shift; done
if [ -f "$T/config/prod.env" ]; then
  echo "[{\"RuleID\":\"aws-access-token\",\"File\":\"$T/config/prod.env\",\"StartLine\":1}]" > "$R"
else echo "[]" > "$R"; fi"#,
        ToolId::Gitleaks,
    );
    // semgrep: one finding per src file containing "eval".
    let semgrep = fake(
        dir,
        "semgrep",
        r#"for a; do T="$a"; done
R=""
for f in "$T"/src/*.ts; do
  if grep -q eval "$f"; then R="$R{\"check_id\":\"rules.detect-eval\",\"path\":\"$f\",\"start\":{\"line\":1},\"extra\":{\"severity\":\"ERROR\",\"metadata\":{}}},"; fi
done
echo "{\"results\":[${R%,}],\"errors\":[]}""#,
        ToolId::Semgrep,
    );
    Tools {
        scc: Some(scc),
        jscpd: Some(jscpd),
        osv: None,
        gitleaks: Some(gitleaks),
        semgrep: Some(semgrep),
    }
}

#[test]
fn snapshot_stage_with_fake_tools() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let api = Repo::init(root.join("api"));
    api.write("src/a.ts", &lines("a", 0..10))
        .write("package-lock.json", "{}\n");
    api.commit("2025-01-10", JANE, "january");
    api.write("src/b.ts", "const x = eval(y);\n")
        .write("config/prod.env", "AWS=…\n");
    api.commit("2025-02-10", JANE, "february");
    api.write("src/b.ts", "const x = y;\n").write("src/c.ts", "eval(z);\n");
    api.commit("2025-03-10", JANE, "march");

    // Recorded osv-scanner output: one critical advisory.
    let osv = root.join("osv/api");
    std::fs::create_dir_all(&osv).unwrap();
    std::fs::write(
        osv.join("package-lock.json.json"),
        r#"{"results":[{"packages":[{"package":{"name":"left-pad","version":"1.0.0"},"vulnerabilities":[{"id":"GHSA-x","database_specific":{"severity":"CRITICAL"}}],"groups":[]}]}]}"#,
    )
    .unwrap();
    let rules = root.join("rules.yaml");
    std::fs::write(&rules, "rules: []\n").unwrap();

    let text = format!(
        r#"
        [project]
        name = "Snapshots"
        range_start = "2025-01-01"
        breadth_roles = ["backend"]
        [[repo]]
        path = "{}"
        branch = "main"
        role = "backend"
        [snapshots]
        cadence = "monthly"
        sast_rules = ["{}"]
        "#,
        root.join("api").display(),
        rules.display()
    );
    let ws = Workspace {
        root: root.join("ws"),
        config: Config::parse(&text, Path::new("pmx.toml")).unwrap(),
    };
    let fakes = root.join("bin");
    std::fs::create_dir_all(&fakes).unwrap();
    let mut opts = CollectOptions::new(d("2025-03-31"));
    opts.fetch = false;
    opts.tools = Some(tools(&fakes));
    opts.osv_replay = Some(root.join("osv"));
    let p = collect(&ws, &opts).unwrap().project;

    assert_eq!(p.snapshots.len(), 3, "one per month-end");
    let m = |id: &str| p.series_monthly[id].clone();
    assert_eq!(m("dup_pct"), [Some(5.0); 3]);
    assert_eq!(m("s_dup"), [Some(100.0 * ((15.0 - 5.0) / 12.0)); 3]);
    assert_eq!(m("kloc"), [Some(0.01), Some(0.011), Some(0.012)]);
    assert_eq!(m("sast_high"), [Some(0.0), Some(1.0), Some(1.0)]);
    // March: b.ts fixed, c.ts new.
    assert_eq!(m("sast_new"), [None, Some(1.0), Some(1.0)]);
    assert_eq!(m("sast_fixed"), [None, Some(0.0), Some(1.0)]);
    assert_eq!(m("secrets_high"), [Some(0.0), Some(1.0), Some(1.0)]);
    assert_eq!(m("vuln_critical"), [Some(1.0); 3]);
    // An open critical caps the score at 40.
    assert_eq!(m("security_score"), [Some(40.0); 3]);
    assert_eq!(m("quality_constituents")[2], Some(4.0));
    assert_eq!(p.security.as_ref().unwrap().top_packages[0].name, "left-pad");
    assert_eq!(p.hotspots.len(), 1);
    assert!(p.meta.not_measured.iter().all(|x| x != "duplication"));
    assert_eq!(p.meta.tool_versions.get("scc").map(String::as_str), Some("4.1.0"));

    // Second run: everything from the cache, same numbers.
    let again = collect(&ws, &opts).unwrap().project;
    assert_eq!(again.snapshots, p.snapshots);

    // Without tools: not measured, never zero.
    let mut none = opts.clone();
    none.tools = Some(Tools::default());
    none.osv_replay = None;
    none.use_cache = false;
    let q = collect(&ws, &none).unwrap().project;
    assert_eq!(q.series_monthly["dup_pct"][2], None);
    assert_eq!(q.series_monthly["security_score"][2], None);
    assert_eq!(q.series_monthly["quality_constituents"][2], Some(3.0));
    for x in [
        "duplication",
        "dependency_vulnerabilities",
        "secrets",
        "sast",
        "complexity",
    ] {
        assert!(q.meta.not_measured.iter().any(|y| y == x), "{x}");
    }
}
