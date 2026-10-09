# Synthesis — Toward Our Metric Set

This page distills the eight research pages into (a) the durable lessons, (b) a concrete **candidate
scorecard**, (c) how to compute it, and (d) the open decisions to settle with the team. It is the
"start here" page for building the actual dashboard.

---

## 1. The seven durable lessons (what everyone agrees on)

1. **No single number. Ever.** Goodhart's Law is not avoidable, only *manageable*. Every serious
   framework (DORA, SPACE, DevEx, DX Core 4) is multidimensional *by design* so that gaming one
   number shows up as regression in another. → We build a **small balanced set**, not a leaderboard.

2. **Pair every speed metric with an opposing quality/stability metric.** This is the single most
   important structural rule. Speed is only a "win" when its quality guardrail holds at the same
   time. DORA (throughput × stability), DX Core 4 (speed × quality), product analytics
   (north-star × guardrail) are all the same idea. → **Speed and quality always appear together.**

3. **Measure the team/system, not the individual.** Deming: ~95% of variation is systemic.
   Individual scoreboards are the industry's documented cautionary tale (GitPrime/Pluralsight Flow).
   The vendors with the clearest ethics (Swarmia, Haystack, DX, Allstacks) *refuse* per-developer
   ranking. → **Unit of analysis = project/team.**

4. **Outcomes > outputs; ratios > raw counts.** Effort → Output → Outcome → Impact; push measurement
   as far right as attribution allows. Raw counts (LOC, commits, PRs, tickets, tokens) reward volume
   and are trivially gamed. → **Prefer rates, distributions, and outcomes.**

5. **Two comparability tiers.** *Outcome/stability* metrics (change failure, revert rate, defect
   escape, recovery time) mean the same thing on any stack → **safe to compare across projects.**
   *Internal code* metrics (coverage, complexity, duplication) are language/stack-specific → **only
   valid as within-repo trends**, never cross-project rankings.

6. **Watch trends and distributions, not absolutes.** A start-state + weekly/monthly re-measure to
   detect forward/backward/stagnation (exactly the user's plan) is the correct design. Sudden jumps
   after a metric becomes visible are the fingerprint of gaming, not improvement.

7. **The agentic question is an experiment, not a metric.** The literature (METR: −19% for experts;
   DORA: −7.2% stability per +25% AI adoption; GitClear: churn ↑, refactoring ↓, cloning ↑) says AI
   *raises individual output while often degrading system stability/maintainability*. So the only
   honest way to know if our agentic workflows help is: **hold a balanced scorecard, introduce the
   workflow as an intervention, and check whether speed rises WITHOUT rework/reverts/failures rising —
   measured against control projects (difference-in-differences).** Never measure AI *usage*
   (acceptance rate, % AI code, tokens) as a success target.

---

## 2. The consensus git-derivable spine

Across ~16 commercial vendors, the metrics that recur almost universally — and are computable from
git + PR metadata alone (our constraint) — are:

- **Cycle time** (and its phases: coding / pickup / review / deploy) — the single most-emphasized
  signal; pickup/review-wait is usually 40–60% of it and the real bottleneck.
- **PR size / batch size** — the most-agreed *leading* health indicator ("keep it small"); also the
  thing AI most inflates.
- **Review latency** (time to first review).
- **Rework / churn rate** (code re-touched within ~21 days) — the most-agreed code-quality proxy.
- **Deployment frequency** (via release tags/merges) and **change failure rate** (via revert/hotfix
  proxies) — the git-approximable half of DORA.

This spine is buildable in-house, keeps us stack-agnostic, and puts the gaming-resistance design in
our own hands. No vendor purchase is required to start.

---

## 3. Candidate scorecard (starting proposal)

Design: **4 headline "front-face" numbers, always shown as 2 speed/quality pairs**, plus **2
slow-moving context gauges**, plus **1 human-signal pulse**. All team/project-level, all read as
trends vs the project's own baseline. Each headline number drills down to its distribution + phase
breakdown ("zoom in").

### Front-face pairs (the dashboard)

| # | Metric | Axis | Source | Why it resists gaming |
|---|--------|------|--------|------------------------|
| **1** | **Delivery Lead Time** — median first-commit → merge/release (show p50 + p85) | Speed | git + PR | Paired with #3/#4; gaming it (splitting, late clock-start) shows up as rising rework/reverts and shrinking PR size in a bad way |
| **2** | **Review Responsiveness** — median pickup time (PR open → first review) | Speed/flow | PR meta | Hard to fake meaningfully; a rubber-stamp to beat it raises post-merge rework (#4) |
| **3** | **Change Failure Proxy** — % of merges/releases that are reverts or hotfixes | Quality/stability | git msg heuristics | Counter to #1/#2: you cannot buy speed by shipping breakage without this rising |
| **4** | **Rework Rate** — % of merged lines re-touched within ~21 days | Quality | git diff lineage | Counter to #1/#2 and *the* AI-slop detector; rises exactly when speed is bought with low-quality code |

> Rule: a project is "improving" only when **a speed number improves AND neither quality number
> worsens.** Speed up + quality down = illusory win (the DORA/GitClear AI pattern).

### Context gauges (slower cadence, monthly/quarterly; trend only)

| # | Metric | Purpose | Source |
|---|--------|---------|--------|
| **5** | **PR batch size** (median lines changed, generated/lockfiles excluded) | Leading indicator behind #1 and the main thing AI inflates | git diff |
| **6** | **Knowledge concentration / hotspots** — single-author-file share; churn×complexity hotspot count | Maintainability & bus-factor risk; near-impossible to game without genuinely improving code | git blame/log (+ optional complexity) |

### Human signal (the one non-git counterweight)

| # | Metric | Purpose | Source |
|---|--------|---------|--------|
| **7** | **Developer Experience pulse** — short quarterly survey → 0–100 index | Catches "numbers look great, team is burning out / fighting the tools"; the Satisfaction/Flow dimension git cannot see. DX's evidence that it counterbalances speed is strong. | 5–10 Likert items |

### Optional (only where reliable tickets exist)

- **Investment balance** (% effort on new capability vs maintenance vs bugs) — needs consistent Jira
  labels or commit classification; great for the "are we shipping value" question but not uniform
  across our projects.
- **Defect escape rate** (bug-labeled issues reaching prod / total) — stronger than the git revert
  proxy where a tracker is disciplined.

---

## 4. Composite "Development Health Index"? — proceed with caution

The user wants simple front-face numbers, ideally few. A single composite is possible but risky. If
we build one, it MUST be **gated, not averaged**: a weighted average lets a big speed gain mask a
quality drop (re-introducing Goodhart). Safer construction:

- Compute each of #1–#4 as a 0–100 sub-score vs the project's own rolling baseline.
- **Health = speed sub-score, but capped/penalized if either quality guardrail is below its
  baseline.** i.e. speed only "counts" while quality holds. (Operationalizes lesson #2.)
- Always display the composite *next to* its component pairs, never alone.

Recommendation: **start with the 4-number paired panel** (transparent, un-gameable to read); add a
single gated index later only if leadership needs one glance.

---

## 5. How to measure it (implementation sketch)

- **Data pull:** `git log --numstat --no-renames --date=short` per repo for churn/lead-time/ownership;
  GitHub/GitLab/Bitbucket **PR API** for pickup/review timestamps and PR size; **release tags** for
  deploy-frequency/lead-time endpoints; commit-message regex for reverts/hotfixes/bugfix proxies.
- **Normalize out** generated files, lockfiles, vendored dirs, and whitespace-only churn, or they
  dominate the numbers. Handle **squash-merge/rebase** by recovering original first-commit time from
  the PR API (see git-derived-metrics.md §4).
- **Mobile/desktop (app-store):** anchor deploy-frequency & lead-time on the **internal/beta release
  channel** (TestFlight / Play internal track), not public-store availability; use **crash-free
  session rate** as the stability signal where available (see dora-metrics.md).
- **Tooling options:** open-source `code-maat`, `git-quick-stats`, `hercules` for the git-behavioral
  metrics; a small custom script for the scorecard; consider **GitClear** (lineage-aware churn / AI
  code quality) or **CodeScene** (Code Health, hotspots) if we want the maintainability layer without
  building it. Emulate **LinearB**'s cycle-time model and **Swarmia/Haystack**'s team-level-only ethic.
- **Cadence:** recompute weekly; review monthly; re-baseline quarterly. Capture each project's full
  scorecard as its **start state** before any agentic-workflow rollout.

---

## 6. The agentic experiment design (the actual point of all this)

1. **Baseline (8–12 weeks of history)** the scorecard for every project = start state.
2. **Intervention:** turn on the agentic workflow in a subset of projects.
3. **Controls:** comparable projects *not* yet on the workflow.
4. **Difference-in-differences:** compare the *change* in treated projects' scorecard to the *change*
   in controls over the same window — isolates the workflow effect from company-wide/seasonal trends.
5. **Verdict rule:** the workflow helps **iff throughput (#1/#2) rises while rework (#4), change
   failure (#3), review load, and PR size stay flat or improve.** Speed-up with guardrails degrading =
   shipping faster-to-fix-later.

---

## 7a. Decisions locked (2026-09-13)

1. **Survey?** → **Git + quarterly DevEx pulse.** Metric #7 is in scope as the human counterweight.
2. **Front-face shape** → **4-number paired panel** (two speed ↔ two quality), per project.
3. **Granularity** → **Team/project-level on the dashboard; individual drill-down available to the
   direct lead only.** ⚠️ This is the historically dangerous choice, so it is fenced with hard rules:
   - Individual views are **private to the lead**, never on any shared dashboard, never ranked
     person-vs-person.
   - **Never** fed into performance reviews, compensation, or layoff decisions — that is the exact
     mechanism that corrupted the metric at Facebook/Uber (Beck/Orosz).
   - **Only flow/collaboration signals** are shown at individual level (e.g. "this person's PRs wait
     a long time for review" → a system problem to fix *for* them). **Quality metrics (rework #4,
     change-failure #3) are NEVER shown per-individual** — at person level they become blame and
     drive concealment. Quality stays team-level only.
   - Framed as conversation-starters, never scores.
4. **Deploy/incident fidelity** → **Pure git proxies everywhere to start** (revert/hotfix
   classification for change-failure). Uniform across all stacks; can add real deploy/incident/crash
   data per project later.

## 7a-bis. Decisions locked (2026-09-14)

5. **SonarQube / internal code-quality metrics?** → **Not on the cross-project front-face.** They are
   stack-relative, cross-project-incomparable, and gameable (Part B of [quality-metrics.md](quality-metrics.md)),
   which breaks three locked constraints. Admitted only as **per-project, trend-only, ratchet-on-changed-code**
   inputs; if we buy a maintainability layer, **CodeScene Code Health** (validated cross-codebase) is preferred
   over SonarQube. **Add a working change-failure / revert-rate guardrail first** — it closes a bigger quality
   gap and is cross-project comparable. Full rationale in quality-metrics.md → "Decision — SonarQube".

6. **Fleet direction → five headline metrics.** The long-term fleet view consolidates to **FTE, Velocity,
   Quality, Multi-stack devs, AI-assisted** (see [fleet-vision.md](fleet-vision.md)). **Quality** is a *forged
   composite* per project (constituents may differ per project/stack); this session's focus. FTE, Velocity and
   AI-assisted detection are deferred. Top-level numbers are explicitly treated as **potentially misleading on
   their own** — every headline must drill down to the project-level detail that explains it.

## 7b. Remaining open decisions

1. **Survey or git-only?** Adding metric #7 (quarterly DevEx pulse) restores the Satisfaction/burnout
   counterweight that git cannot see — but it's the one non-automatable, non-git input. Fully git-only
   is simpler and uniform but blind to whether the team is suffering. *(Biggest fork.)*
2. **Front-face shape:** transparent 4-number paired panel (rec) vs a single gated Health Index vs
   both.
3. **Individual visibility:** team/project-level only (strong rec) vs allow drill-down to individuals
   (invites gaming + mistrust; the documented failure mode).
4. **Deploy/incident fidelity:** pure git proxies everywhere (uniform, simplest) vs invest in
   per-project deploy + incident + crash data where it exists (higher-fidelity CFR/MTTR, less uniform).
5. **Tickets:** use them where present (adds investment-balance & escaped-defects) vs ignore for
   cross-project uniformity.
6. **Build vs buy:** ~~DIY the git spine vs buy~~ → **RESOLVED 2026-09-14: DIY for now.** For the quality
   layer specifically, stay in-house (`git + OSV-Scanner + scc + jscpd + semgrep`, ~$0 license) rather than
   buy CodeScene/Sonar; revisit CodeScene Pro if the DIY proxy proves insufficient. Rationale + cost
   comparison in [quality-index.md](quality-index.md) §5–6.

---

## 8. What to explicitly NOT do (the banned list)

LOC / added lines · commits per day / coding days · PR **count** · tickets **closed** · story-point
velocity as a target · test-coverage % as a target · documentation volume · **AI: acceptance rate,
% AI-authored code, tokens, self-reported time-saved** · any per-individual "productivity score."
These are all Very-High gaming risk, several are actively inflated by AI, and every one maps to a
failure mode in goodharts-law-and-gaming.md. Keep any of them only as *context on a zoom-in screen*,
never as a headline target.
