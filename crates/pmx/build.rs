//! Builds `pm-wasm` for `wasm32-unknown-unknown` so the dashboard can be embedded with its metrics
//! engine. Without the wasm32 target (or with `PMX_SKIP_WASM=1`) an empty module is embedded and
//! the dashboard falls back to the precomputed weeks and months; build output says how to fix it.

use std::path::PathBuf;
use std::process::Command;

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

    let skip = |why: &str| {
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
