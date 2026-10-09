//! Parity harness (wiki/parity.md): runs `pmx collect` on the golden set's pinned SHAs and diffs
//! the result against the prototype's long-format table.
//!
//! The golden set is private client data. It is read from `PMX_GOLDEN_DIR` and nothing from it is
//! written anywhere: the report goes to the terminal only.
//!
//! ```text
//! PMX_GOLDEN_DIR=~/path/to/golden/2026-10-09 cargo run -p pmx-parity --release -- [project…] [--mode prototype|spec|both] [--all]
//! ```
//! `PMX_PROTOTYPE_TOOLS` points at the prototype's `tools/` (for `externals.json`); default
//! `$PMX_GOLDEN_DIR/../../tools`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use chrono::{Days, NaiveDate};
use pm_config::Workspace;
use pm_metrics::period::{month_end, months};
use pm_metrics::{ProjectFile, aggregate, multistack};
use pmx::{CollectOptions, collect};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// M1 metrics. Snapshot, security and code-host series belong to later milestones.
const M1_METRICS: &[&str] = &[
    "commits",
    "added",
    "commit_med",
    "commit_p90",
    "commits_per_dev_med",
    "active_devs",
    "rework_pct",
    "test_discipline_pct",
    "doc_discipline_pct",
    "multi_stack_pct",
    "breadth_index",
    "techs_per_dev",
    "ai_assist_commit_pct",
    "ai_assist_line_pct",
];

/// `Prototype` reads git and detects tests as the prototype did, so the rest of the pipeline is
/// checked exactly; `Spec` is the real tool, whose intentional differences are reported.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Prototype,
    Spec,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Expect {
    Exact,
    /// The register expects a difference; report it.
    Differs,
}

/// How the prototype stored the value (parity.md §2, rounding).
#[derive(Clone, Copy)]
enum Rounding {
    Int,
    Trunc,
    Places(usize),
}

fn rounding(metric: &str) -> Rounding {
    match metric {
        "commits" | "added" | "active_devs" => Rounding::Int,
        "commit_med" | "commit_p90" => Rounding::Trunc,
        "rework_pct" | "techs_per_dev" => Rounding::Places(2),
        m if m.starts_with("ai_compare_") => match m.rsplit_once('_').map(|x| x.1) {
            Some("size") => Rounding::Trunc,
            Some("pct") => Rounding::Places(0),
            _ => Rounding::Int,
        },
        _ => Rounding::Places(1),
    }
}

/// Python's `round(x, n)` (round half to even on the exact binary value): Rust's fixed-precision
/// formatting rounds the same way.
fn apply(r: Rounding, x: f64) -> f64 {
    match r {
        Rounding::Int => x,
        Rounding::Trunc => x.trunc(),
        Rounding::Places(n) => format!("{x:.n$}").parse().expect("formatted float parses"),
    }
}

struct Golden {
    dir: PathBuf,
    manifest: Value,
    /// (project, metric, period) → value
    rows: BTreeMap<(String, String, String), f64>,
    externals: Option<Value>,
}

fn load_golden() -> Result<Golden> {
    let dir = PathBuf::from(std::env::var("PMX_GOLDEN_DIR").context("set PMX_GOLDEN_DIR to the golden folder")?);
    let manifest: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json"))?)?;
    let mut rows = BTreeMap::new();
    for (i, line) in std::fs::read_to_string(dir.join("metrics.csv"))?
        .lines()
        .enumerate()
        .skip(1)
    {
        let f: Vec<&str> = line.split(',').collect();
        if f.len() < 4 {
            bail!("metrics.csv line {}: expected project,metric,period,value,n", i + 1);
        }
        if let Ok(v) = f[3].parse::<f64>() {
            rows.insert((f[0].to_string(), f[1].to_string(), f[2].to_string()), v);
        }
    }
    let tools = std::env::var("PMX_PROTOTYPE_TOOLS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dir.join("../../tools"));
    let ext_path = tools.join("externals.json");
    let externals = match std::fs::read(&ext_path) {
        Ok(bytes) => {
            let got: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
            let want = manifest["prototype"]["externals.json_sha256"]
                .as_str()
                .unwrap_or_default();
            if got != want {
                eprintln!("warning: {} does not match the manifest's SHA-256", ext_path.display());
            }
            Some(serde_json::from_slice(&bytes)?)
        }
        Err(_) => {
            eprintln!("warning: no {}; externals are not excluded", ext_path.display());
            None
        }
    };
    Ok(Golden {
        dir,
        manifest,
        rows,
        externals,
    })
}

struct Run {
    project: ProjectFile,
    /// metric → period → value, with the prototype's windows where they differ from the spec.
    values: BTreeMap<String, BTreeMap<String, Option<f64>>>,
}

fn run_project(g: &Golden, key: &str, mode: Mode) -> Result<Run> {
    let cfg_json: Value = serde_json::from_str(
        &std::fs::read_to_string(g.dir.join("configs").join(format!("config_{key}.json")))
            .with_context(|| format!("golden config for `{key}`"))?,
    )?;
    let side = pm_config::import::SideFiles {
        externals: g.externals.as_ref(),
        ..Default::default()
    };
    let imported = pm_config::import::from_prototype(&cfg_json, &side)?;
    let pinned = &g.manifest["projects"][key]["repos"];
    for repo in &imported.config.repos {
        let name = repo.display_name();
        let want = pinned
            .as_array()
            .and_then(|a| a.iter().find(|r| r["name"] == name.as_str()))
            .and_then(|r| r["sha"].as_str());
        if repo.rev.as_deref() != want {
            bail!(
                "{key}/{name}: config rev {:?} is not the manifest's pinned SHA {want:?}",
                repo.rev
            );
        }
    }
    let as_of = NaiveDate::parse_from_str(g.manifest["as_of"].as_str().unwrap_or_default(), "%Y-%m-%d")?;
    let tmp = tempfile::tempdir()?;
    let ws = Workspace {
        root: tmp.path().to_path_buf(),
        config: imported.config,
    };
    let mut opts = CollectOptions::new(as_of);
    opts.fetch = false;
    opts.use_cache = false;
    opts.prototype_compat = mode == Mode::Prototype;
    let c = collect(&ws, &opts)?;
    let p = c.project;

    let mut values: BTreeMap<String, BTreeMap<String, Option<f64>>> = BTreeMap::new();
    for (metric, series) in &p.series_monthly {
        let m = values.entry(metric.clone()).or_default();
        for (label, v) in p.months.iter().zip(series) {
            m.insert(label.clone(), *v);
        }
    }
    // The prototype computed multi-stack over a trailing window ending at the month's end
    // (parity.md §3); query pmx's days the same way.
    let window = imported.state_window_days.unwrap_or(90) as u64;
    for b in months(p.project.range_start, as_of) {
        let end = month_end(b.start);
        let start = end + Days::new(1) - Days::new(window);
        let ms = multistack(&aggregate(&p.days, start, end), &p.project.breadth_roles);
        for (id, v) in [
            ("multi_stack_pct", ms.multi_pct),
            ("breadth_index", ms.breadth_index),
            ("techs_per_dev", ms.techs_per_dev),
            ("active_devs", Some(ms.authors as f64)),
        ] {
            values.entry(id.into()).or_default().insert(b.label.clone(), v);
        }
    }
    if let Some(ai) = &p.ai_compare {
        for (grp, s) in [("ai", &ai.ai), ("human", &ai.human)] {
            for (k, v) in [
                ("commits", Some(s.commits as f64)),
                ("added", Some(s.added as f64)),
                ("med_size", s.med_size),
                ("test_pct", s.test_pct),
                ("doc_pct", s.doc_pct),
            ] {
                values
                    .entry(format!("ai_compare_{grp}_{k}"))
                    .or_default()
                    .insert("last-12m".into(), v);
            }
        }
    }
    Ok(Run { project: p, values })
}

struct Line {
    metric: String,
    expect: Expect,
    total: usize,
    matched: usize,
    mismatches: Vec<(String, f64, f64)>,
}

fn expectation(metric: &str, mode: Mode, ai_window_ok: bool) -> Expect {
    if mode == Mode::Prototype {
        // The prototype's test rules and git reading: every M1 series must match exactly, except
        // the AI comparison when the spec moved its window.
        return if metric.starts_with("ai_compare_") && !ai_window_ok {
            Expect::Differs
        } else {
            Expect::Exact
        };
    }
    // Spec mode adds the test-detection fix and the git-reading fixes (parity.md §3), which move
    // any line- or file-based series. Only pure commit counts must still match.
    match metric {
        "commits" | "commits_per_dev_med" | "ai_assist_commit_pct" => Expect::Exact,
        m if m.starts_with("ai_compare_") && m.ends_with("_commits") && ai_window_ok => Expect::Exact,
        _ => Expect::Differs,
    }
}

fn compare(g: &Golden, key: &str, run: &Run, mode: Mode) -> Vec<Line> {
    let as_of = run.project.meta.as_of.to_string();
    let ai_window_ok = g.manifest["projects"][key]["repos"]
        .as_array()
        .into_iter()
        .flatten()
        .all(|r| r["tip_date"].as_str().is_some_and(|d| d[..10] <= *as_of));
    let mut lines: BTreeMap<String, Line> = BTreeMap::new();
    for ((proj, metric, period), golden) in &g.rows {
        let in_scope =
            M1_METRICS.contains(&metric.as_str()) || metric.starts_with("mix_") || metric.starts_with("ai_compare_");
        if proj != key || !in_scope {
            continue;
        }
        let ours = run.values.get(metric).and_then(|m| m.get(period)).copied().flatten();
        // No data in pmx is null; the prototype wrote 0 (parity.md §2).
        let ours = apply(rounding(metric), ours.unwrap_or(0.0));
        let line = lines.entry(metric.clone()).or_insert_with(|| Line {
            metric: metric.clone(),
            expect: expectation(metric, mode, ai_window_ok),
            total: 0,
            matched: 0,
            mismatches: Vec::new(),
        });
        line.total += 1;
        if ours == *golden {
            line.matched += 1;
        } else {
            line.mismatches.push((period.clone(), *golden, ours));
        }
    }
    lines.into_values().collect()
}

fn main() -> ExitCode {
    match real_main() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn real_main() -> Result<bool> {
    let mut keys = Vec::new();
    let mut modes = vec![Mode::Prototype, Mode::Spec];
    let mut show_all = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--mode" => {
                modes = match args.next().as_deref() {
                    Some("spec") => vec![Mode::Spec],
                    Some("prototype") => vec![Mode::Prototype],
                    Some("both") => vec![Mode::Prototype, Mode::Spec],
                    other => bail!("--mode spec|prototype|both, got {other:?}"),
                }
            }
            "--all" => show_all = true,
            k => keys.push(k.to_string()),
        }
    }
    let g = load_golden()?;
    if keys.is_empty() {
        keys = g.manifest["projects"]
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();
    }
    println!("golden: {}  as_of: {}", g.dir.display(), g.manifest["as_of"]);
    let mut ok = true;
    for mode in &modes {
        println!(
            "\n=== {} ===",
            match mode {
                Mode::Prototype => "prototype dialect (must match exactly)",
                Mode::Spec => "spec (intentional differences are reported as deltas)",
            }
        );
        for key in &keys {
            let started = std::time::Instant::now();
            let run = run_project(&g, key, *mode)?;
            let lines = compare(&g, key, &run, *mode);
            println!("\n[{key}] ({:.1}s)", started.elapsed().as_secs_f64());
            println!(
                "  {:<26} {:<8} {:>9}  first differences (period: golden → pmx)",
                "metric", "expect", "match"
            );
            for l in &lines {
                let failed = l.expect == Expect::Exact && l.matched != l.total;
                ok &= !failed;
                let flag = if failed {
                    "FAIL"
                } else if l.matched == l.total {
                    "ok"
                } else {
                    "delta"
                };
                let shown = if show_all { l.mismatches.len() } else { 4 };
                let diffs: Vec<String> = l
                    .mismatches
                    .iter()
                    .take(shown)
                    .map(|(p, a, b)| format!("{p}: {a} → {b}"))
                    .collect();
                let more = l.mismatches.len().saturating_sub(shown);
                println!(
                    "  {:<26} {:<8} {:>4}/{:<4} {:<5} {}{}",
                    l.metric,
                    format!("{:?}", l.expect).to_lowercase(),
                    l.matched,
                    l.total,
                    flag,
                    diffs.join("; "),
                    if more > 0 { format!(" (+{more})") } else { String::new() }
                );
            }
        }
    }
    println!("\n{}", if ok { "PARITY OK" } else { "PARITY FAILED" });
    Ok(ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_rounding() {
        assert_eq!(apply(Rounding::Places(2), 2.675), 2.67);
        assert_eq!(apply(Rounding::Places(1), 0.25), 0.2);
        assert_eq!(apply(Rounding::Places(0), 2.5), 2.0);
        assert_eq!(apply(Rounding::Places(0), 3.5), 4.0);
        assert_eq!(apply(Rounding::Trunc, 17.9), 17.0);
    }

    #[test]
    fn roundings() {
        assert!(matches!(rounding("ai_compare_ai_med_size"), Rounding::Trunc));
        assert!(matches!(rounding("ai_compare_human_test_pct"), Rounding::Places(0)));
        assert!(matches!(rounding("ai_compare_ai_commits"), Rounding::Int));
        assert!(matches!(rounding("rework_pct"), Rounding::Places(2)));
        assert!(matches!(rounding("mix_qa"), Rounding::Places(1)));
    }
}
