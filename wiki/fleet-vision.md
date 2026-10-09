# Fleet View — Vision & the Five Headline Metrics

The long-term goal for the top-level **fleet dashboard** (all projects on one screen) is to
consolidate to **five headline metrics** that give leadership a high-level read on the areas the
company cares about most, each drilling down into the detailed per-project dashboard. This page
records that vision, the status of each metric, and the guardrails on reading them.

> **Owner's framing (2026-09-14):** *"These five metrics would give us a great, high-level overview
> of our progress in the areas we care about most."*

---

## The five headline metrics

| # | Metric | What it answers | Status | Notes |
|---|--------|-----------------|--------|-------|
| 1 | **FTE** | How much *capacity* is really on this project (paid, dedicated effort) — replaces the current raw **Active devs** count | **Deferred (step 1 done)** | Needs the **utilization platform** for real developer-hours (a dev at 10% ≠ 100%). **Step 1 shipped: external-developer exclusion** (see below) so the stats show *our* work. Fractional FTE-hours still deferred. |
| 2 | **Velocity** | Are we productive / shipping? | **🔨 v1 design + pilot** | `Velocity = six-axis-complexity points ÷ engineer-days` (points/FTE-day); commits & lines become the ratios points/commit & points/KLOC, **not** addends. Beyond commit/line counts by weighting *how hard* changes were. Needs an LLM diff-scoring pass (first non-git input). See [velocity-index.md](velocity-index.md). |
| 3 | **Quality** | Is the code good — and is it getting **worse as AI usage rises**? | **✅ v1 live** | Forged composite = geomean(rework, tests-with-code, docs-with-code, duplication) vs fixed targets; 0–100 + band A–E. **Live on the fleet** (`tools/quality_score.py`); constituents removed from the fleet table (they live on project dashboards); **vulnerability shown separately**. See [quality-index.md](quality-index.md) §4.0. |
| 4 | **Multi-stack devs** | Are developers growing across stacks (full-stack progression)? | **Largely done** | Already shipped: % of active devs working across ≥2 stack roles, with per-dev breadth/techs drill-down. See [fullstack-metrics.md](fullstack-metrics.md). |
| 5 | **AI-assisted** | How agentic has our work become? | **Deferred (detection)** | Shipped as % of commits carrying a `Co-Authored-By: Claude` trailer, but **detection needs work** (invisible where the trailer is stripped by policy, e.g. Project C; other tools/Copilot not covered). Needs tool telemetry. *Skip the detection work for now.* |

This session's scope is **#3 Quality only.** #1, #2 and #5's detection are explicitly out of scope
here.

---

## Core principle — top-level numbers can mislead; the drill-down is the safeguard

> **Owner (2026-09-14):** *"I know that these top-level metrics can be misleading. That's why we need
> the detailed metrics on the project level, to understand the full picture."*

This is a first-class design rule, not a caveat:

- Every headline number is a **compression** of many project-, stack-, and dev-level signals, and any
  compression can hide a bad story (a great fleet-average Quality can mask one project quietly
  degrading; a rising AI-assisted number says nothing about whether quality held).
- Therefore **every fleet headline must remain a link into the project dashboard**, where the full,
  un-compressed picture lives (the existing "front-face number → zoom-in" pattern in
  [synthesis.md](synthesis.md) §3). The fleet view is for *noticing where to look*, never for
  *concluding*.
- Corollary for the Quality index specifically: **never show the composite alone.** Always render its
  constituents beside it, and let a click open the per-project, per-stack breakdown. A forged score
  that can't be decomposed on screen is a Goodhart trap waiting to happen.

---

## Where the current fleet columns map

The live fleet dashboard (draft) today shows: Active devs · Commits/mo · Lines/mo · Commits/dev ·
Commit size · Rework · Tests-w/code · Docs-w/code · Multi-stack · AI-assisted · PRs reviewed · Vulns.
Under the five-metric vision these regroup as:

- **FTE** ← replaces *Active devs* (once utilization hours are integrated).
- **Velocity** ← **v1 = six-axis complexity points ÷ engineer-days**; *Commits/mo, Lines/mo* stop being
  headline counts and become the interpretive ratios (points/commit, points/KLOC). Deliberately **not** a
  sum of raw counts, which are Goodhart-prone ([synthesis.md](synthesis.md) §8). See [velocity-index.md](velocity-index.md).
- **Quality** ← **v1 live**: forges *Rework, Tests-w/code, Docs-w/code, Duplication* into one geomean
  composite (0–100 + band); these constituent columns were **removed from the fleet table** to avoid
  double-showing. *Vulnerability* stays a **separate** column/badge (not folded in until remediation-age
  data enables a hard gate); a change-failure guardrail joins later. See [quality-index.md](quality-index.md) §4.0.
- **Multi-stack devs** ← already the *Multi-stack* column.
- **AI-assisted** ← already the *AI-assisted* column (detection to be hardened).
- *PRs reviewed* and the raw volume counts stay as **project-level drill-down context**, not headlines.

---

## External developers — exclusion (FTE step 1, 2026-09-14)

The stats should show **our** results, so **external (non-company) developers are excluded** from all
author-attributable metrics. This is a recurring per-project task as new projects join, so it's a
config, not a code change:

- **`tools/externals.json`** — the single source of truth: per-project (and `*`-global) list of external
  identities, matched case-insensitively by **name or email** (add one entry per alias a person commits
  under). First entry: **Project B → Sam Contractor**.
- **`tools/authors.py`** — shared filter every git tool imports (`is_excluded(name,email,…)`); also drops
  bots (now incl. GitLab CI/Runner, Jenkins, …). Remove someone here → removed everywhere.

**Critical distinction — what exclusion applies to:**
- **Author-attributable metrics** (commits, lines, rework, tests-with-code, docs-with-code, **Velocity
  points & engineer-days**, multi-stack, AI-assisted) → externals **are** excluded.
- **Codebase-snapshot metrics** (**duplication %, complexity/KLOC, dependency vulnerabilities**) →
  **NOT** excluded, because they measure the code that exists in the repo regardless of who wrote it.
  You can't cleanly subtract one author from a duplication scan.

**Manual FTE (stopgap):** `tools/fte.json` holds a per-project FTE until the utilization platform lands.
Velocity uses `points / (FTE × business-days)` where FTE is known, else falls back to active-engineer-days
(flagged `~`). Known: **Project B = 1, Project C = 3** (Project A TBD). This flipped the velocity ranking
(Project C 3.46→1.5 per-FTE; Project B 1.46→3.5) — the canonical "headcount ≠ capacity" example. See
[velocity-index.md](velocity-index.md) §5b.

**Applied so far:** the **Velocity** pipeline (`six_axis_score.py`, `velocity_score.py`) reads the config
now — Project B velocity recomputed excluding the contractor (1.37 → 1.46 pts/eng-day; small, confirming his
share was minor). **Still to wire:** the per-project dashboard git-series (rework, tests/docs, commits,
lines, multi-stack, AI) are produced by the **collector**, which must read the same `externals.json` to
apply the exclusion there too — that's the integration point (the collector is not in this repo).

*Known gotcha:* work-package ids must come from git **verbatim** — the pilot scorer mis-transcribed an
accented name ("Müller"→"Muller"), which would have dropped a real dev's work if we'd filtered by name
match. Match externals by the config, not by re-deriving names.

---

## The Quality metric — what "forging" has to achieve

The owner's plan is to **forge an overall Quality metric per project** (and possibly per stack within a
project) that is representable on the fleet view, and whose primary job is to **monitor whether code
quality gets worse as AI usage grows**. Open design questions being researched in
[quality-index.md](quality-index.md):

1. **How to combine** heterogeneous sub-metrics (rework rate, tests-with-code, docs-with-code,
   vulnerability, and likely a change-failure guardrail) into one comparable number — **gated, not
   naively averaged**, so a gain in one component can't mask a quality drop in another (the
   [synthesis.md](synthesis.md) §4 lesson).
2. **Per-project / per-stack constituents.** Different projects may forge *different* measured signals
   into their Quality number (e.g. a project with a real CI/incident feed can use change-failure; a
   pure-lockfile project leans on rework + vulnerability). The forging recipe must tolerate missing
   constituents without becoming incomparable.
3. **AI-regression visibility.** Each constituent should be sliceable by **AI-attributed vs
   human-authored** commits, so the index can answer "does quality fall where AI rises?" —
   difference-in-differences against control projects (see [ai-agentic-productivity.md](ai-agentic-productivity.md)).
4. **Rework and tests-with-code are explicitly in scope** to be forged into Quality (owner's note).

The internal-metrics / SonarQube question is **resolved** (do not put stack-relative internal metrics
on the fleet; admit them only as per-project trend-only ratchets; prefer CodeScene Code Health if
buying) — see [quality-metrics.md](quality-metrics.md) → "Decision — SonarQube".

---

*Status: vision recorded 2026-09-14. Quality-index design in progress (research dispatched). FTE,
Velocity, and AI-detection deferred by owner.*
