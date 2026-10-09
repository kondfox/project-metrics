# What we measure — one-pager

We measure how well each project team develops software, using mostly **git history**, so the same
numbers work on any stack (web, mobile, desktop). The main question behind it: **is agentic / AI-assisted
work helping or hurting?**

Exact definitions, formulas and inputs: **[metrics-spec.md](metrics-spec.md)**. Background:
[synthesis.md](synthesis.md), [fleet-vision.md](fleet-vision.md).

## Ground rules
- **Weekly cadence**, rolled up to months on the dashboard (stored per day, so both are exact).
- **Team/project level only.** Quality is never shown per person; per-person views are private to the lead.
- **Trends over snapshots.** Read each project against its own history.
- **Speed is always paired with quality**, so neither can be gamed alone. Volume counts (commits, lines)
  are context, never targets ([goodharts-law-and-gaming.md](goodharts-law-and-gaming.md)).
- **Fixed targets, not rankings.** Scores are 0–100 vs fixed yardsticks, bands A ≥ 90 · B ≥ 75 · C ≥ 55 ·
  D ≥ 30 · E.
- **Every headline drills down** to the project dashboard; a headline alone can mislead.
- **Our work only.** Bots and external developers are excluded from author-based metrics.

## Fleet headlines (one row per project)

| Metric | What it answers | How it's measured | Details |
|---|---|---|---|
| **Quality** | Is the code healthy, and does it get worse as AI use grows? | Geometric mean of 4 sub-scores: rework (distance from the healthy 15–25% band), tests shipped with code (target 80%), docs shipped with code (target 40%), duplication (3% → 100, 15% → 0) | [spec §3](metrics-spec.md#3-quality-fleet-headline--quality-indexmd-40) · [quality-index.md](quality-index.md) |
| **Velocity** | How much hard work are we delivering per unit of capacity? | An LLM scores each work package's diff on six complexity axes → points; points ÷ (FTE × business days) | [spec §4](metrics-spec.md#4-velocity-fleet-headline--velocity-indexmd) · [velocity-index.md](velocity-index.md) |
| **Security** | How exposed are we? | Dependency CVEs (OSV-Scanner) weighted by severity → 0–100; any open Critical or live high-confidence secret caps it at D | [spec §7](metrics-spec.md#7-security-fleet-headline--security-indexmd-own-code-securitymd) · [security-index.md](security-index.md) · internal notes (not published) |
| **Multi-stack devs** | Are developers growing across stacks? | % of developers with ≥ 40 added lines in ≥ 2 stack roles in the selected period | [spec §5](metrics-spec.md#5-multi-stack-developers-fleet-headline--fullstack-metricsmd) · [fullstack-metrics.md](fullstack-metrics.md) |
| **AI-assisted** | How agentic has our work become? | % of commits with a `Co-Authored-By: Claude` trailer (hidden where trailers are stripped) | [spec §6](metrics-spec.md#6-ai-assisted-fleet-headline) · [ai-agentic-productivity.md](ai-agentic-productivity.md) |
| **Peer review** | Does code get reviewed? | % of merged PRs/MRs with ≥ 1 human non-author review | [spec §8](metrics-spec.md#8-code-host-prmr-flow--peer-review-fleet-column--project-panels) |
| *FTE* | Real capacity on the project | Manual per-project value for now; will come from the utilization platform | [spec §10.2](metrics-spec.md#102-fte-provider-external-source-stub-in-v1) |

## Project dashboard (the drill-down)

| Area | Panels |
|---|---|
| Activity | commits, lines added, commit size (median/p90), commits per dev, active devs, stack mix |
| Quality | rework %, tests/docs shipped with code, duplication %, complexity/KLOC (trend only), hotspots |
| Velocity | points per FTE-day per week/month; points/commit, points/KLOC |
| AI | AI-assisted commits/lines %, AI vs human comparison (size, tests, docs) over 12 months |
| Security | open dependency vulns by severity over time; own-code SAST findings (trend only); new/fixed findings, live secrets, suppressions |
| Flow & people | PR cycle time, review wait, mentoring pairs; multi-stack %, breadth index, techs per dev |

## Where the data comes from
**git** (all flow metrics) · **scc / jscpd** (complexity, duplication) · **OSV-Scanner / gitleaks /
semgrep** (security) · **GitHub/GitLab API** (PR flow) · **LLM** (Velocity scoring) · **FTE source**
(manual now). Missing inputs show as "not measured", never as 0 — see [spec §10](metrics-spec.md#10-inputs--contracts).

## Not yet measured
Change-failure rate (convention specified in [change-failure-proxy.md](change-failure-proxy.md)),
vulnerability remediation age, deeper expertise/truck-factor, cross-file SAST, DevEx survey — see
[spec §9](metrics-spec.md#9-not-in-v1-designed-or-placeholder).
