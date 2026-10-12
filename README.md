# project-metrics

> **Status: early development.** Milestones M1–M4 are done: pmx sets up a multi-repo workspace, collects
> the git-derived metrics (activity, rework, tests/docs-with-code, multi-stack, AI-assisted), the
> snapshot metrics (duplication, complexity, dependency vulnerabilities, secrets, SAST) and the PR flow
> from GitHub or GitLab (peer review, cycle time, review wait, mentoring) incrementally, reproduces the
> private prototype, and shows a project dashboard. The local fleet view (M6) is done; Velocity (M5,
> LLM-scored) is built and awaiting its exit check. The design lives in the [wiki](wiki/README.md).

## Try it

Needs Rust (stable) with the `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`),
Node ≥ 20.19 (the build compiles the dashboard) and git ≥ 2.30. Without the target or Node pmx still
builds, but with a reduced or placeholder dashboard.

Try it on a fictional project first: `pmx demo /tmp/acme && pmx -C /tmp/acme serve --open`, or on a
fictional fleet of three: `pmx demo /tmp/acme-fleet --fleet && pmx -C /tmp/acme-fleet fleet serve --open`.

```sh
cargo build --release && export PATH="$PWD/target/release:$PATH"

mkdir -p ~/metrics/acme-shop && cd ~/metrics/acme-shop
pmx init ~/src/acme          # finds the clones, suggests branch + role per repo, writes pmx.toml
pmx repo set shop-legacy --role per-file   # fix a suggestion if needed
pmx repo set shop-api --code-host-id acme/shop-api   # GitHub owner/repo or GitLab project id
export GITHUB_TOKEN=…        # or the [code_host] token_env: read access to PRs, for peer review
pmx people                   # who is who across repos (and, after a collect, code-host logins)
pmx check                    # paths, branches, tokens, unmapped identities
pmx doctor                   # which snapshot tools are installed
pmx tools install            # pinned scc, osv-scanner, gitleaks (checksum-verified)
                             # SAST uses pmx's own bundled rules (crates/pm-snapshot/rules)
pmx plan                     # what collect would do, and how long it should take
pmx collect                  # progress + ETA; Ctrl-C and re-run to resume
pmx serve --open             # the dashboard at http://127.0.0.1:7878 (includes the private lead view)
pmx export --format html     # out/dashboard.html: one self-contained file to share
pmx export --format long --weekly
```

Several projects make a **fleet**: a `fleet.toml` lists their workspace folders
([example](examples/fleet.toml)), and

```sh
cd ~/metrics
pmx fleet serve --open       # every project's headlines side by side; click one for its dashboard
                             # (local only, so the dashboards include each project's people)
pmx fleet export             # out/fleet.json + out/fleet.html (one shareable page, no per-person data)
pmx fleet export --with-private  # out/private/fleet.html: with people, for the lead only
```

`collect` writes `out/project.json` (team level; people are pseudonymous ids) and
`out/private/leads.json` (per-person drill-downs, for the lead only). Re-runs only read new commits and
ask the code host only for PRs updated since the last run.
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

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT),
at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this
work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
