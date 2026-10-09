# Project Metrics — LLM Wiki

Research and design knowledge base for a tech-stack-agnostic, gaming-resistant set of metrics that
measure how well a team develops software. Each project is measured at a starting state and then tracked
weekly and monthly, to see whether it moves forward, backward or stagnates.

Context:
- Organizations have many projects on very different stacks (fullstack web, mobile, desktop).
- Metrics must be **stack-independent** and derived primarily from **git history**, optionally enriched by
  the code host (GitHub/GitLab) and ticketing systems.
- Goal: measure both the **efficiency of the team** and the **quality of the outcome**.
- Headline metrics are **simple numbers** for an organization-wide fleet dashboard. Supporting detail
  lives one zoom level down, on the project dashboard.
- **Avoid Goodhart's-law gaming:** no single metric that can be trivially gamed (LOC, PR count, ticket
  count, coverage %, doc volume, token usage, …).
- Driving question: is **agentic / AI-assisted engineering** helping or hurting?

Pilot results quoted in some pages come from real projects and are anonymized as *Project A–D*. Some pages
mention `collector/…` and `tools/…`: these are the private Python prototype, which is not part of this repo.

## Start here
- [metrics-overview.md](metrics-overview.md) — **one-pager**: what we measure and how, with links
- [metrics-spec.md](metrics-spec.md) — **v1 implementation spec**: exact definitions, config, input contracts (source of truth for the tool)
- [tool-implementation-plan.md](tool-implementation-plan.md) — **implementation plan** for the Rust tool (`pmx`): architecture, multi-repo setup, progress/ETA, configurable LLM classifier on Jev's schema, milestones

## Design pages
- [fleet-vision.md](fleet-vision.md) — the headline fleet metrics and the drill-down principle
- [quality-index.md](quality-index.md) — the composite **Quality** score
- [velocity-index.md](velocity-index.md) — **Velocity**: six-axis complexity points ÷ capacity
- [security-index.md](security-index.md) — **Security** score from dependency vulnerabilities (OSV) and secrets
- [change-failure-proxy.md](change-failure-proxy.md) — commit convention + detection spec for a change-failure guardrail (not in v1)
- [fullstack-metrics.md](fullstack-metrics.md) — multi-stack progression design (stack + technology breadth, mentoring)
- [synthesis.md](synthesis.md) — synthesis of the research, candidate metric set and decisions

## Research pages
- [dora-metrics.md](dora-metrics.md) — DORA four/five keys
- [space-framework.md](space-framework.md) — SPACE multidimensional framework
- [devex-framework.md](devex-framework.md) — DevEx (feedback loops, cognitive load, flow)
- [goodharts-law-and-gaming.md](goodharts-law-and-gaming.md) — why single metrics fail; combining them
- [git-derived-metrics.md](git-derived-metrics.md) — concrete signals computable from git history
- [commercial-tools.md](commercial-tools.md) — what engineering-intelligence vendors measure
- [ai-agentic-productivity.md](ai-agentic-productivity.md) — measuring the impact of AI/agentic workflows
- [quality-metrics.md](quality-metrics.md) — quality, maintainability and outcome metrics
- [fullstack-and-expertise-research.md](fullstack-and-expertise-research.md) — prior art (degree of authorship, truck factor, entropy breadth)

## Rubrics
- [../rubrics/six-axis@1.md](../rubrics/six-axis@1.md) — the six-axis work-item complexity rubric used by Velocity
