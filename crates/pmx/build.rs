//! Builds what the dashboard embeds:
//! - `pm-wasm` for `wasm32-unknown-unknown` (the metrics engine). Without the wasm32 target (or with
//!   `PMX_SKIP_WASM=1`) an empty module is embedded and the dashboard falls back to whole weeks and
//!   months.
//! - The React app in `web/` (`npm ci` if needed, then `npm run build`) into one HTML file. Without
//!   Node (or with `PMX_SKIP_WEB=1`) a placeholder page explains how to build it.
//!
//! `PMX_REQUIRE_WASM=1` / `PMX_REQUIRE_WEB=1` turn a fallback into a build error (CI sets both).

use std::path::{Path, PathBuf};
use std::process::Command;

const FALLBACK_PAGE: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>pmx</title></head>
<body style="font: 15px/1.5 system-ui, sans-serif; max-width: 40rem; margin: 3rem auto; padding: 0 1rem">
<h1>Dashboard not built</h1>
<p>This pmx was built without Node, so its dashboard is missing. Install Node 20.19 or newer, then
rebuild pmx (<code>cargo build --release</code>): the build runs <code>npm ci</code> and
<code>npm run build</code> in <code>crates/pmx/web</code>.</p>
<p>The data is in <code>out/project.json</code> meanwhile; <code>pmx export --format long</code> prints it as a table.</p>
<script id="pmx-data" type="application/json"></script>
</body></html>
"#;

fn required(var: &str) -> bool {
    std::env::var_os(var).is_some_and(|v| v != "0" && !v.is_empty())
}

fn build_web(manifest: &Path, out: &Path) -> Result<(), String> {
    let web = manifest.join("web");
    for p in [
        "src",
        "index.html",
        "package.json",
        "package-lock.json",
        "vite.config.ts",
        "tsconfig.json",
    ] {
        println!("cargo:rerun-if-changed={}", web.join(p).display());
    }
    println!("cargo:rerun-if-env-changed=PMX_SKIP_WEB");
    if std::env::var_os("PMX_SKIP_WEB").is_some() {
        return Err("PMX_SKIP_WEB is set".into());
    }
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let run = |args: &[&str]| -> Result<(), String> {
        match Command::new(npm).args(args).current_dir(&web).status() {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(format!("`npm {}` failed ({s})", args.join(" "))),
            Err(e) => Err(format!("cannot run npm: {e}")),
        }
    };
    if !web.join("node_modules").exists() {
        run(&["ci", "--no-audit", "--no-fund"])?;
    }
    let dist = out.join("web");
    let dist_arg = dist.display().to_string();
    run(&["run", "build", "--", "--outDir", &dist_arg, "--emptyOutDir"])
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let dest = out.join("pm_wasm.wasm");
    for dir in [
        "../pm-wasm/src",
        "../pm-metrics/src",
        "../pm-classify/src",
        "../pm-wasm/Cargo.toml",
    ] {
        println!("cargo:rerun-if-changed={}", manifest.join(dir).display());
    }
    println!("cargo:rerun-if-env-changed=PMX_SKIP_WASM");
    println!("cargo:rerun-if-env-changed=PMX_REQUIRE_WASM");
    println!("cargo:rerun-if-env-changed=PMX_REQUIRE_WEB");

    if let Err(why) = build_web(&manifest, &out) {
        if required("PMX_REQUIRE_WEB") {
            panic!("dashboard build failed: {why}");
        }
        println!(
            "cargo:warning=dashboard not built ({why}); pmx will serve a placeholder page. Fix: install Node >= 20.19"
        );
        std::fs::create_dir_all(out.join("web")).unwrap();
        std::fs::write(out.join("web/index.html"), FALLBACK_PAGE).unwrap();
    }

    let skip = |why: &str| {
        if required("PMX_REQUIRE_WASM") {
            panic!("WASM engine build failed: {why}");
        }
        println!(
            "cargo:warning=dashboard built without its WASM engine ({why}); custom ranges are disabled. Fix: `rustup target add wasm32-unknown-unknown`"
        );
        std::fs::write(&dest, b"").unwrap();
    };
    if std::env::var_os("PMX_SKIP_WASM").is_some() {
        return skip("PMX_SKIP_WASM is set");
    }
    let target_dir = out.join("wasm-target");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.args([
        "build",
        "--release",
        "--target",
        "wasm32-unknown-unknown",
        "--manifest-path",
    ])
    .arg(manifest.join("../pm-wasm/Cargo.toml"))
    .arg("--target-dir")
    .arg(&target_dir)
    .args([
        "--config",
        "profile.release.opt-level=\"s\"",
        "--config",
        "profile.release.lto=true",
        "--config",
        "profile.release.codegen-units=1",
        "--config",
        "profile.release.panic=\"abort\"",
    ]);
    // Flags meant for the host build must not leak into the wasm build.
    for var in [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_BUILD_TARGET",
        "CARGO_TARGET_DIR",
        "RUSTC_WRAPPER",
    ] {
        cmd.env_remove(var);
    }
    match cmd.status() {
        Ok(s) if s.success() => {
            let built = target_dir.join("wasm32-unknown-unknown/release/pm_wasm.wasm");
            std::fs::copy(&built, &dest).unwrap();
        }
        Ok(_) => skip("the wasm32 build failed"),
        Err(e) => skip(&format!("cannot run cargo: {e}")),
    }
}
