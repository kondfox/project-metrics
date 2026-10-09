# project-metrics

> **Status: early development.** Milestones M1–M2 are done: pmx sets up a multi-repo workspace, collects
> the git-derived metrics (activity, rework, tests/docs-with-code, multi-stack, AI-assisted) incrementally,
> reproduces the private prototype, and shows a project dashboard. Snapshot metrics (duplication,
> security), PR flow and Velocity come next. The design lives in the [wiki](wiki/README.md).

## Try it

Needs Rust (stable) with the `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`;
without it the dashboard can't compute custom ranges) and git ≥ 2.30.

```sh
cargo build --release && export PATH="$PWD/target/release:$PATH"

mkdir -p ~/metrics/acme-shop && cd ~/metrics/acme-shop
pmx init ~/src/acme          # finds the clones, suggests branch + role per repo, writes pmx.toml
pmx repo set shop-legacy --role per-file   # fix a suggestion if needed
pmx people                   # who is who across repos; --apply writes [people]
pmx check                    # paths, branches, tokens, unmapped identities
pmx plan                     # what collect would do, and how long it should take
pmx collect                  # progress + ETA; Ctrl-C and re-run to resume
pmx serve --open             # the dashboard at http://127.0.0.1:7878 (includes the private lead view)
pmx export --format html     # out/dashboard.html: one self-contained file to share
pmx export --format long --weekly
```

`collect` writes `out/project.json` (team level; people are pseudonymous ids) and
`out/private/leads.json` (per-person drill-downs, for the lead only). Re-runs only read new commits.
A config from the Python prototype converts with `pmx import-config config_acme.json`.

## Design

- A single **Rust** binary (`pmx`, working name) with the dashboard embedded.
- One workspace folder per project, listing any number of repositories, which are pooled into one project.
- Incremental, resumable collection with progress and an ETA.
- A **configurable LLM classifier** that always uses one question/answer schema (modelled on
  [Jev](https://docs.typesafe.ai/)'s). It works with Jev, OpenAI-compatible endpoints (including local
  models for sensitive code), Anthropic, or any command, with a `local-only` data-policy guard.

Full plan and milestones: [wiki/tool-implementation-plan.md](wiki/tool-implementation-plan.md).

## License

The dashboard bundles [Apache ECharts](https://echarts.apache.org/) 5.6.0 (Apache-2.0); its licence and
notice are in [crates/pmx/web/vendor](crates/pmx/web/vendor).

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT),
at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this
work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
