# project-metrics

> **Status: early development.** Milestone M1a is done: `pmx collect` computes the git-derived metrics
> (activity, rework, tests/docs-with-code, multi-stack, AI-assisted) and reproduces the private prototype.
> No dashboard yet. The design lives in the [wiki](wiki/README.md).

A tool a developer downloads, points at their project's git repositories, and runs to get **engineering
metrics on a web dashboard**. A **fleet view** then combines all of an organization's projects on one
screen.

The metrics are derived mostly from git history, so they work on any stack (web, mobile, desktop). They
are designed to resist gaming, and they help answer one question: **is AI-assisted / agentic
development helping or hurting?**

## What it measures

| Headline | Question |
|---|---|
| **Quality** | Is the code healthy? Geometric mean of rework, tests shipped with code, docs shipped with code, and duplication, scored against fixed targets |
| **Velocity** | How much hard work is delivered per unit of capacity? An LLM classifier rates each work package on a six-axis complexity rubric |
| **Security** | How exposed are we? Dependency vulnerabilities (OSV), with live secrets and criticals as a gate |
| **Multi-stack devs** | Are developers growing across stacks? |
| **AI-assisted** | How agentic has the work become? |
| **Peer review** | Does code get reviewed? |

Every headline drills down to a project dashboard. Scores are team-level only, never per person.
One-page summary: [wiki/metrics-overview.md](wiki/metrics-overview.md). Exact definitions:
[wiki/metrics-spec.md](wiki/metrics-spec.md).

## Try it

Needs Rust (stable) and git ≥ 2.30.

```sh
cargo build --release
# a workspace folder with a pmx.toml (see examples/pmx.toml)
./target/release/pmx -C ~/metrics/acme-shop collect
./target/release/pmx -C ~/metrics/acme-shop export --format long --weekly
```

`collect` writes `out/project.json` (team level) and `out/private/leads.json` (per-person drill-downs,
for the lead only). Setup commands (`init`, `repo`, `people`, `check`) and the dashboard come next.

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
