# Forging the Quality Metric — Design & Research

**Goal:** turn a project's several measured quality signals into **one overall "Quality" number** that
(a) sits on the fleet dashboard as headline metric #3 (see [fleet-vision.md](fleet-vision.md)),
(b) drills down to per-project / per-stack detail, and (c) exists primarily to reveal **whether code
quality degrades as AI usage rises**. Constituents may differ per project/stack; the forging recipe
must tolerate that and stay comparable.

Prerequisites already settled:
- Two comparability tiers and the counter-metric principle — [quality-metrics.md](quality-metrics.md).
- SonarQube / internal-metrics decision (not on the fleet; trend-only ratchets only; prefer CodeScene
  Code Health if buying) — [quality-metrics.md](quality-metrics.md) → "Decision — SonarQube".
- No naive weighted average — must be **gated** so one component can't mask another — [synthesis.md](synthesis.md) §4.

---

## 1. The constituents to forge

**Live today (git + lockfiles):**
- **Rework Rate** — lines rewritten within 21 days ÷ added (healthy 15–25%; two-sided guardrail).
- **Tests with code** — % source-changing commits also touching tests.
- **Docs with code** — % source-changing commits also touching docs.
- **Vulnerability** — open CVEs by severity (OSV-Scanner).

**Owner wants forged in:** rework rate and tests-with-code explicitly; others as they prove out.

**To add (research §3B):** a **change-failure proxy** (the one missing outcome guardrail) as a fourth
continuous constituent; **duplication %** as the best-evidenced AI-regression signal; structural
signals only as per-project trend-only ratchets.

---

## 2. Design questions (the forging recipe)

1. Normalization — different scales & "good" directions.
2. Combination — gated vs weighted-average.
3. Missing constituents — comparability must survive a project lacking a signal.
4. Aggregation — repo → project → fleet.
5. Output shape — 0–100 vs letter grade.
6. AI-regression slice — segment by AI vs human authorship.

All six are answered below.

---

## 3. Research findings (2026-09-14)

Three research agents were run; full source lists at the bottom of each subsection.

### 3A. Index construction & prior art

**Normalization — use fixed targets, never the fleet.**

| Approach | Verdict |
|---|---|
| Min-max / z-score / percentile **across projects** | ✗ Breaks stack-agnosticism (a Kotlin app and a Terraform repo have different "natural" rates) and makes every project's score a function of the current fleet — one outlier rescales everyone. Percentile-of-fleet is also **zero-sum** (a project can only rise if another falls → sabotage incentive). |
| Z-score / percentile vs the **project's own rolling baseline** | ✓ The **trend layer** ("is this project moving the right way"). This is what "read as trends vs each project's own baseline" already means. |
| **Target / band-based scoring** (fixed target, documented once, not fleet-derived) | ✓ The **cross-project comparable layer**. Every project scored against the same fixed yardstick → comparable *because* they're measured against a constant, not each other. Rework's 15–25% band already works this way; extend to every sub-metric. This is how SonarQube ratings, SIG stars (percentile vs a **static** 30k-system benchmark), and CISQ/ISO 5055 all work. |

**Combination — gate + geometric mean, never a plain weighted average.**
A weighted arithmetic mean is *compensatory*: a big gain in one dimension fully offsets a collapse in
another (constant marginal rate of substitution). With equal weights a project could let a Critical CVE
sit open (Vuln→0) while padding Tests/Docs and still score ~78 "looks fine." This is the exact Goodhart
failure the dashboard exists to prevent — the same defect that made the UN switch HDI from arithmetic to
**geometric** mean in 2010, and the reason the [OECD Handbook on Composite Indicators](https://www.oecd.org/content/dam/oecd/en/publications/reports/2008/08/handbook-on-constructing-composite-indicators-methodology-and-user-guide_g1gh9301/9789264043466-en.pdf)
classes additive aggregation as "compensatory," geometric as "partially compensatory," and gates/thresholds as "non-compensatory."
Every credible code-quality tool gates rather than blends: **SonarQube** quality gate is hard pass/fail and its ratings are worst-case-driven (one Critical bug caps Reliability at E); **SIG** reports all sub-characteristic stars and drives the system rating by the worst; **CodeScene** avoids the problem by *scope* (Code Health only fuses signals within one dimension).

**Aggregation — pool, don't average averages.** `Tests_with_code(project) = Σ(commits-with-tests across repos) / Σ(source commits across repos)`, never `mean(per-repo ratios)` (Simpson's-paradox trap). Gate components (vulns) aggregate by **worst-case (min)**, not pooling — a Critical in 1 of 10 repos isn't "1/10th of a problem." Fleet level: prefer the **distribution** (median, P10/P25, count below threshold, trend arrows) over a single fleet scalar, which is the most gameable level.

**Prior art — how existing systems roll many signals into one:**

| System | Combination | Cross-language? | Weakness |
|---|---|---|---|
| **Code Climate** (A–F) | complexity+duplication → debt-ratio → letter; coverage separate | in principle | complexity/dup only; heuristic constants; **now dead → Qlty** |
| **CodeScene Code Health** (1–10) | ~25–30 factors, LOC-weighted avg **within one dimension** | yes, benchmark-normalized | doesn't cover tests/docs/vulns; rule-based false positives; **peer-reviewed defect link ("Code Red", TechDebt 2022)** |
| **SonarQube** (A–E + gate) | **gate, not blend**; ratings worst-issue-driven; "Sonar way" gates new code only | yes (ratio-based) | SQALE debt-ratio has an arbitrary 30-min/line constant, dilutes as LOC grows → within-repo trend only; rule-suppression gaming |
| **SIG/TÜViT** (1–5★) | source metrics → percentile vs **fixed 30k-system benchmark** → stars; worst-case aware | **most rigorously comparable** (TÜViT-certified) | needs proprietary benchmark corpus; benchmark drifts over time |
| **CISQ/ISO 5055** | severity-weighted CWE-mapped weakness counts; **4 separate scores**, no default fuse | yes (ISO standard) | structural weaknesses only; spotty niche-stack coverage |
| **Classic Maintainability Index** | one regression formula (Halstead+CC+LOC) | nominally | **discredited** — mostly measures file length; tool constants differ |

**Pattern:** *no credible system fuses orthogonal dimensions via a plain weighted average.* They stay
single-dimension, or gate hard on the worst signal and show the rest as a dashboard, or use a
benchmark-relative worst-case-aware star system. → confirms **gate + geometric mean**.

### 3B. Additional measurable quality signals — ranked shortlist

All evaluated for: git-derivable (± one universal CLI), cross-stack comparable, gaming-resistant,
AI-sensitive. Ranked list of what to add first:

| Rank | Signal | Source | Comparability | Why | AI-sensitivity |
|---|---|---|---|---|---|
| **1** | **Change-failure proxy** = revert rate + fix-commit ratio + fix latency | **pure git** | ratio → yes | The one missing **outcome/lagging** guardrail (everything current is leading/process). Revert = a distinct git op, near-zero gaming. Fix-commit = SZZ *fix-identification* half only (robust; skip the expensive bug-introducing trace). | **High** — "did the AI change actually break something" |
| **2** | **Duplication %** | `jscpd` (universal) | ratio → yes | **Best-evidenced AI-degradation signal:** GitClear — duplicate blocks ↑ ~8× in 2024, copy-paste first exceeded moved code, block duplication +81% by 2026 vs 2023. Covers TS/JS/Kotlin/C#/SQL. | **Very high** |
| **3** | **Relative code churn** (churn ÷ size, Nagappan-Ball) | pure git | ratio → yes | Strongest peer-reviewed defect-density link; tracks the GitClear AI churn spike. **Same family as existing Rework Rate** — treat as a refinement, not additive. | Very high |
| **4** | **Hotspot** (churn × complexity) | git + `lizard`/`scc` | rank within repo | Turns two weak signals into one strong "where defects concentrate" list; AI tends to pile into already-large files. | High |
| **5** | **PR/commit batch-size trend** | pure git | ratio → yes | Leading indicator that **agentic velocity is outrunning review** (field report: PR size ↑3.5× with AI, fewer bugs caught; small PRs 15% less likely reverted). | High |
| **6** | **Single-author-file share** (bus-factor) | pure git (`git blame`) | % → yes | Risk *companion*, **not folded into Quality**: is AI code concentrating under too few reviewing humans. | Med (risk, not quality) |

**Deprioritized (phase 2+ or never):** cyclomatic/cognitive complexity (partial language coverage —
`lizard` misses SQL/Terraform/Shell — and within-repo-trend-only); test-to-code LOC ratio
(within-repo-trend-only, gameable by shallow tests); TODO/FIXME density (Goodhart trap — declining
density more likely means marker-avoidance); commit-message quality (use as a *classifier feature* for
the fix-ratio, not a KPI); full SZZ bug-introducing trace (expensive/noisy); **true DORA CFR** (needs
uniform deploy+incident data — breaks git-only; use the revert/hotfix proxy instead).

*Sources: GitClear [AI code quality 2025](https://www.gitclear.com/ai_assistant_code_quality_2025_research) / [Maintainability Gap 2026](https://www.gitclear.com/the_ai_code_quality_maintainability_gap); Nagappan & Ball [ICSE'05](https://dl.acm.org/doi/10.1145/1062455.1062514); [revert-commits study](https://link.springer.com/article/10.1007/s10664-019-09688-8); [SZZ evaluation](https://arxiv.org/pdf/2308.05060); [lizard](https://github.com/terryyin/lizard) · [scc](https://github.com/boyter/scc) · [jscpd](https://jscpd.dev/getting-started/supported-formats) · [code-maat](https://github.com/adamtornhill/code-maat); [PR size](https://www.cubic.dev/blog/does-pr-size-actually-matter) / [3.5× PRs with AI](https://brodzinski.com/2026/07/3x-pull-request-size-ai.html).*

### 3C. Cross-stack tooling — build vs buy

Stack coverage is the deciding axis (TS/React/Node, Kotlin/Gradle, .NET/C#, SQL, Terraform, Shell, Python).

| Tool | Single roll-up? | Stack gaps | Self-host | Fleet view | AI-regression features | Price (2026) |
|---|---|---|---|---|---|---|
| **CodeScene** | 3 KPIs (Code Health 1–10) | **no SQL, no Shell; Terraform = X-Ray only** | yes | **yes (Portfolio, Pro+)** | **purpose-built** (AI Perf Framework, Code-Health MCP, published agentic-regression case study); peer-reviewed defect link | ~€18–27/author/mo |
| **SonarQube / Cloud** | no (A–E ratings + gate) | **none — only tool native across all 7** incl. Terraform/Shell/SQL | yes | Enterprise tier only ($$) | "AI Code Assurance" tags AI code + AI gate | Free / ~$34/mo → Enterprise |
| Code Climate | — | **dead** (API off Jul 2025) → Qlty | — | — | — | — |
| Codacy | A–F | partial IaC/SQL | Enterprise | limited | — | ~$18–21/dev/mo |
| DeepSource | PR scorecard A–D | **Kotlin beta** (Android gap) | Enterprise only | limited | agent skill | Free / $24/user/mo |
| Qlty (CC successor) | letter grades | **thin .NET** | no (CLI/cloud) | basic | — | Free / $20/contrib/mo |

**OSS universal analyzers** (build-your-own): only **`scc`** (size/complexity, 322 langs) and **`jscpd`**
(duplication, 220+ langs) are genuinely no-plugin and cover the **entire** stack incl. Terraform/Shell.
`semgrep` is deeper but per-language (good on 5/7). Plus existing **OSV-Scanner** + git-mining.
A defensible zero-license pipeline: **git-mining + OSV + scc + jscpd (+ semgrep as targeted SAST)**.

**Verdict:**
- **DIY is good-enough and on-philosophy** for size/duplication/hygiene/churn trends at zero cost — but
  none of the OSS scores are *validated* against real defect/maintainability cost the way CodeScene's
  Code Health is peer-reviewed to be. It's a reasonable proxy, not a validated one.
- **If buying, CodeScene fits the stated goals best** (single per-file score → clean project roll-up,
  built-in Portfolio fleet view, and the only product line explicitly built around detecting AI-driven
  quality regression). Cover its SQL/Shell/Terraform gap with the existing OSS layer.
- **SonarQube** wins only on raw language completeness; its ratings aren't a single comparable score,
  Portfolio is the pricey tier, and rule-suppression is a known gaming vector.

*Caveats flagged by the researcher: CodeScene's AI-defect statistics are vendor marketing (not
independently verified); SonarQube self-host prices are third-party estimates, not list prices.*

---

## 4. Recommended construction for the fleet Quality metric

### 4.0 Locked v1 recipe (2026-09-14) — what we compute today

Implemented in `tools/quality_score.py`, surfaced on the **fleet view**. The full gate×geomean design
below (§4.1) is the target; v1 is the subset we can compute now, with **vulnerability kept separate**
(not gated in) until we have remediation-age/SLA data — raw transitive-CVE counts would zero almost
every project and stop the score discriminating.

```
Quality = geometric_mean( s_rework, s_tests, s_docs, s_dup )     # four 0–100 sub-scores
```
Each sub-score is measured against a **fixed target** (not against other projects), last full month,
pooled across the project's repos:

| Sub-score | Function | Target |
|---|---|---|
| `s_rework` | 100 inside 15–25%, else `100 − 4·distance` from the band | healthy band 15–25%, both sides penalized (**changed 2026-10-08**; was one-sided ≤25% — `tools/quality_score.py` still one-sided until the tool lands, see [metrics-spec.md](metrics-spec.md) §3.1) |
| `s_tests` | `100 · min(1, x/80)` | 80% tests-with-code |
| `s_docs` | `100 · min(1, x/40)` | 40% docs-with-code |
| `s_dup` | `100 · clamp((15−x)/12, 0, 1)` | 3%→100, 9%→50, 15%→0 |

Bands: **A** 90–100 · **B** 75–89 · **C** 55–74 · **D** 30–54 · **E** 0–29 (fixed cut-points, not fleet
percentiles). Sub-scores floored at 1 so a single 0 drives Quality very low without a literal divide-to-zero.

**Computed on live data (2026-08):**

| Project | rework→ | tests→ | docs→ | dup→ | **Quality** | Band | Vuln (separate) |
|---|---|---|---|---|---|---|---|
| **Project B** | 100 | 100 | 83 | 86 | **92** | A | ⚠ 14 crit |
| **Project C** | 100 | 22 | 40 | 93 | **53** | D | ✅ 0 crit |
| **Project A** | 100 | 53 | 16 | 39 | **43** | D | ⚠ 45 crit |

**Explicitly separate / out:** **Vulnerability** is a badge beside the score (critical count), promoted to
a hard gate later once remediation-age data exists. **Complexity/KLOC** is out (stack-dependent, trend-only).
**Change-failure** joins the geomean once the commit convention ([change-failure-proxy.md](change-failure-proxy.md))
makes it non-zero. Targets (80/40/15) are tunable calibration knobs.

---

### 4.1 Formula — gate × geometric mean (full target design)

```
Quality(project, trailing-window) =
    100 × gate_vuln × ( Π scoreᵢ^wᵢ )^(1 / Σwᵢ)     for i in the present continuous constituents

Continuous constituents (each scored 0–100 vs a FIXED target/band):
    score_rework       — band 15–25%, penalize distance outside the band (two-sided)
    score_tests        — 100 × min(1, x / 80%)     (higher better, ceiling 80%)
    score_docs         — 100 × min(1, x / 40%)     (higher better, ceiling 40%)
    score_changefail   — NEW: 100 × (1 − min(1, revert_hotfix_rate / target))   (lower better)

Gate (non-compensatory, worst-case across repos):
    gate_vuln = 1.0                      if no High/Critical CVE past its SLA
              = 0.6^(n_critical_overdue) otherwise, floored at 0.1
```

Geometric mean (not arithmetic) so a strength can only *partially* offset a weakness: e.g. constituents
(95, 95, 20) → arithmetic 70 "looks C-grade fine" vs **geometric 58.6** "something is broken." Full zero-
tolerance is reserved for the vulnerability **gate**, where a single overdue Critical more than halves the
whole index.

### 4.2 Constituents, per project/stack

- **v1 forge (all git/lockfile, available fleet-wide):** rework · tests-with-code · docs-with-code · **change-failure proxy** (add first, per §3B rank 1) · vulnerability-gate. Owner's "rework + tests forged in" is satisfied and extended.
- **Missing constituents:** **drop the term and renormalize the exponent** over present weights — never impute an average (that's a free-lunch gaming vector: a project drops a signal it's bad at and gets an implicit "average"). **Always display which constituents fed the number** ("Quality 74 — 3/4 signals, no vuln scan for this repo"), on every card, not a tooltip.
- **Later constituents (trend-only, per-project ratchet, never a raw cross-project value):** duplication % (`jscpd`), hotspot, and — if bought — **CodeScene Code Health** for its supported languages.

### 4.3 Output — 0–100 **and** a band

Continuous 0–100 for sparklines/trends (a 2-pt quarterly move is real signal), plus a coarse band for
the fleet at-a-glance, with cut-points from **fixed targets** (not fleet percentiles — otherwise
"everyone improved" is mathematically impossible to show):

| Band | Score | Meaning |
|---|---|---|
| A | 90–100 | Healthy, no gate trip |
| B | 75–89 | Solid, minor drift on one signal |
| C | 55–74 | Watch — one dimension degrading |
| D | 30–54 | At risk — gate tripped or multiple weak signals |
| E | 0–29 | Critical — overdue severe vuln or multi-dimension collapse |

### 4.4 Worked example

Project "Foxtrot" (TS/React), trailing 90 days, pooled across 3 repos:

| Sub-metric | Pooled value | Target/band | Score |
|---|---|---|---|
| Rework | 21% | 15–25% | 100 |
| Tests-with-code | 48% | →80% | 60.0 |
| Docs-with-code | 22% | →40% | 55.0 |
| Vulnerability | 1 Critical open 40d (SLA 14) | gate | ×0.6 |

`base = (100 × 60 × 55)^(1/3) ≈ 69.1` → `Quality = 69.1 × 0.6 ≈ 41.5` → **Band D — At risk.** A naive
weighted average would land Foxtrot in the high-50s "C, watch," masking the overdue Critical. The gate
is what correctly escalates it.

### 4.5 Exposing AI-linked quality regression (the whole point)

1. **Segment every constituent by AI-attributed vs human-authored commits** — at **team/project level only, never per-developer** (per-dev AI-quality callouts violate the never-per-individual-for-quality rule *and* create a direct incentive to strip AI trailers, poisoning the instrumentation). Attribution = commit trailers (`Co-Authored-By: Claude`, `.cursor/` presence) as the primary signal, with a heuristic classifier as an audit layer since trailers can be silently dropped.
2. **Difference-in-differences vs control projects** — run DiD on **each constituent independently** (not just the fused index) so you can see *which* dimension moves. Template: [He et al. 2025 DiD-on-Cursor-adoption](https://arxiv.org/html/2511.04427v2) (staggered adoption, non-adopters as controls; found transient velocity gain but persistent complexity/warning rise).
3. **Report fleet AI-adoption-% vs Quality-Δ as a scatter/regression**, DORA-style ("N% adoption ↔ X% quality change") — a format leadership recognizes and one that resists cherry-picking single projects.
4. **Primary AI canaries:** Rework Rate and duplication (strongest field evidence — GitClear); the AI-vs-human **gap** in tests-with-code is a clean low-noise early warning.
5. **Methodology note for the dashboard:** short-task RCTs (GitHub 2024 — Copilot code rated *higher*) and longitudinal field studies (GitClear/DORA/METR — churn, duplication, instability up) both hold; they measure **different timescales**, and the fleet's git-mined trend view is the one that matters over months. METR 2025 (experienced devs 19% *slower* while believing 20% faster) is why we lean on objective git signals over sentiment.

### 4.6 Principle-compliance guardrails

- **Do NOT** ship a single fleet-wide weighted-average Quality number as the only figure — gate × geometric mean, always with constituent drill-down (echoes the [fleet-vision.md](fleet-vision.md) "top-level can mislead" rule).
- **Do NOT** normalize via cross-project min-max/percentile — breaks stack-agnosticism; makes scores fleet-relative and zero-sum.
- **Do NOT** surface AI-vs-human deltas per developer — project/team only.
- **DO** compute over trailing pooled windows, not point-in-time snapshots — closes the end-of-period "test dump" gaming vector and matches "read as trends."

---

## 5. Proposed rollout order

1. **Add the change-failure proxy** (revert + fix-commit ratio + fix latency) — closes the biggest gap, pure git. Requires nudging teams toward revert/hotfix commit conventions (the current proxy reads ~0 without them — see [quality-metrics.md](quality-metrics.md) as-built gaps). **Concrete convention + detection regex + rollout drafted in [change-failure-proxy.md](change-failure-proxy.md).**
2. **Add duplication %** via `jscpd` (best AI-regression signal) and **PR/commit batch-size** trend; add **single-author-file share** as a separate risk companion (not folded into Quality). *Duplication + size/complexity are now implemented and baselined — see internal notes (not published).*
3. **Forge Quality v1** = gate_vuln × geomean(rework, tests, docs, change-failure); display 0–100 + band + constituents; wire the AI-vs-human segmentation and DiD.
4. **Phase 2:** hotspot (churn×complexity via `scc`/`lizard`) and duplication as per-project trend-only ratchets.
5. **Build-vs-buy — DECIDED: DIY for now (2026-09-14).** Stay in-house: `git-mining + OSV-Scanner + scc
   + jscpd (+ semgrep)`, zero licensing cost, consistent with the build-it-ourselves philosophy and
   already ~most of the way there (the git-mining + OSV pipeline exists; adding `scc`/`jscpd` is
   marginal). Accepted trade-off: the DIY score is a reasonable proxy, **not** peer-reviewed-validated
   against real defect cost the way CodeScene's Code Health is, and we own all tuning/gaming-resistance.
   **Revisit CodeScene Pro** (≈€27/active-author/mo, validated Code Health + fleet Portfolio + AI-regression
   tooling for TS/Kotlin/C#/Python) only if the DIY proxy proves insufficient or budget opens up — see
   [cost comparison](#6-tool-cost-comparison-for-reference).

---

## 6. Tool cost comparison (for reference)

Two pricing models — **per-developer** (CodeScene, Codacy, DeepSource, Qlty) scales with headcount;
**per-LOC** (SonarQube) scales with codebase size *independent of team*. The per-LOC model works against
our AI-volume goal: cost rises exactly as AI inflates code.

| Tool | Model | ~10 devs | ~25 devs | ~50 devs | Fleet-view tier |
|---|---|---|---|---|---|
| **DIY** (git+OSV+scc+jscpd) | — (OSS) | **~$0** | **~$0** | **~$0** | already ours |
| CodeScene Pro | €27/active author/mo | ~€270 | ~€675 | ~€1,350 | Pro (included) |
| CodeScene Standard | €18/active author/mo | ~€180 | ~€450 | ~€900 | Pro tier needed |
| Codacy | ~$19/dev/mo | ~$190 | ~$475 | ~$950 | limited |
| Qlty Pro | $20/contributor/mo | ~$200 | ~$500 | ~$1,000 | basic |
| DeepSource | $24/user/mo | ~$240 | ~$600 | ~$1,200 | limited (Kotlin beta) |
| SonarQube | per-LOC | free→low-$100s/mo (Cloud); self-host ~$210→$1,300/mo by LOC | **fleet Portfolio = Enterprise ($100K+/yr)** |

*Ballpark 2026 list prices; CodeScene "active author" = committed that month (can be < headcount);
Sonar self-host figures are third-party estimates. Code Climate Quality is discontinued.*

**→ DIY chosen: ~$0 license vs ~€700/mo for CodeScene Pro at ~25 active authors.**

---

*Status: research complete 2026-09-14. Build-vs-buy DECIDED (DIY). §4 construction is a proposal for
team review, not yet locked. Next: rollout step 1 — introduce the change-failure / revert-hotfix commit
convention so that guardrail stops reading ~0.*
