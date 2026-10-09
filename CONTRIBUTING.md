# Contributing

Plan and milestones: [wiki/tool-implementation-plan.md](wiki/tool-implementation-plan.md).

- **Build and test:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`. Tests build their own fictional git repos; CI runs exactly these. Install
  the `wasm32-unknown-unknown` target (the dashboard's engine) and Node (to check the engine against the
  CLI; CI requires it with `PMX_REQUIRE_NODE=1`).
- **Dashboard:** `crates/pmx/web/` is plain JS with no build step. Metric math belongs in pm-metrics,
  never in JS: the page calls the WASM engine for anything computed.
- **Parity:** `crates/pmx-parity` compares pmx with the private prototype's golden set
  ([wiki/parity.md](wiki/parity.md)). It needs `PMX_GOLDEN_DIR` and only runs for maintainers who have
  the golden set. Its report goes to the terminal; never paste it into an issue or commit.

- **The wiki is the design source of truth.** [wiki/metrics-spec.md](wiki/metrics-spec.md) defines what
  is measured. Change the spec in the same pull request as any code that changes a metric's behaviour.
- **No real data in this repo.** Never commit real names, emails, repository URLs, metric outputs or
  `pmx` workspace folders (`.pmx/`, `out/`). Use fictional examples (`Acme`, `Jane Doe`, `example.com`).
- **Licensing.** Contributions are dual-licensed MIT OR Apache-2.0 (see [README](README.md#license)).
  Don't add third-party rules, data or code under incompatible licences (for example, Semgrep registry
  rules).
