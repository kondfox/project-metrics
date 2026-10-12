# pmx — project metrics

pmx measures how well a team develops software, mostly from **git history**, so the same numbers work on
any stack. Point it at a project's repositories and it shows a dashboard in your browser. The dashboard
covers Quality (rework, tests and docs shipped with code, duplication), Security (vulnerable
dependencies, live secrets), multi-stack developers, AI-assisted work, peer review and, with an LLM,
Velocity. Everything runs on your machine.

> **Status: early development (0.x).** The metrics reproduce a private prototype on three pilot
> projects. Expect changes between releases. Design and milestones: [wiki](wiki/README.md).

## Install

```sh
brew install kondfox/tap/pmx
# or
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/kondfox/project-metrics/releases/latest/download/pmx-installer.sh | sh
```

Windows, `cargo binstall` and archives: [getting started](docs/getting-started.md#1-install-1-minute).
You need git ≥ 2.30.

## Try it

```sh
pmx demo /tmp/acme && pmx -C /tmp/acme serve --open        # a fictional project
```

## Your project

```sh
mkdir -p ~/metrics/acme-shop && cd ~/metrics/acme-shop
pmx init ~/src/acme      # finds the clones and walks you through the setup, then collects
pmx serve --open         # the dashboard at http://127.0.0.1:7878 (with the lead's per-person view)
```

Later: `pmx collect` (only reads what is new; Ctrl-C and re-run to resume), then
`pmx export --format html` to write one self-contained `out/dashboard.html` you can share. It has no
per-person data.

| Command | What it does |
|---|---|
| `pmx init [dirs…]` | Guided setup: repos, roles, token, classifier, people, tools, check, collect |
| `pmx repo add\|list\|set\|remove` | Edit the repo list |
| `pmx people` | Group author identities into people; `merge`, `bot`, `external` |
| `pmx check` · `pmx doctor` | Is the setup complete? Which tools are installed? |
| `pmx tools install` | The pinned scc, osv-scanner and gitleaks (checksum-verified) |
| `pmx plan` · `pmx collect` | What a collect would do (with an ETA) · do it |
| `pmx serve` · `pmx export` | The dashboard, live or as a file (`--format html` or `long` CSV) |
| `pmx classifier test\|eval` | Check a Velocity classifier, or qualify one against reference scores |
| `pmx fleet serve\|export` | Several projects side by side ([fleet](docs/fleet.md)) |
| `pmx import-config` | Convert a config from the Python prototype |

**Docs:** [getting started](docs/getting-started.md) · [Velocity](docs/velocity.md) ·
[fleet](docs/fleet.md) · [troubleshooting](docs/troubleshooting.md) ·
[what the metrics mean](wiki/metrics-overview.md) · [example pmx.toml](examples/pmx.toml).

## Privacy

- `out/project.json` and the exported dashboard are **team level**. People appear only as pseudonymous
  ids (`p1`, `p2`, …).
- Per-person views live in `out/private/` and in `pmx serve`, which only your machine can reach.
- Velocity sends diffs to the classifier you configure. `data_policy = "local-only"` on a repo
  allows only classifiers on your machine ([details](docs/velocity.md)).
- `pmx.toml` names tokens only by environment variable, so it is safe to commit.

## Design

A single Rust binary with the dashboard (React, plus the metrics engine compiled to WASM) embedded. One
workspace folder per project, pooling any number of repositories. Incremental, resumable collection.
Classifiers all use one question/answer schema (modelled on [Jev](https://docs.typesafe.ai/)'s): Jev,
OpenAI-compatible endpoints (including local models), Anthropic, Claude Code, or any command.

Plan and milestones: [wiki/tool-implementation-plan.md](wiki/tool-implementation-plan.md). Building from
source and contributing: [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT),
at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this
work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
