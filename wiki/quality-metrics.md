# Software Quality, Maintainability & Outcome Metrics

## Summary

This page covers the "quality of the outcome" half of software-development measurement: metrics that tell you whether work is *good*, not just whether it is *fast* or *plentiful*. The central lesson is that quality metrics split into two families with very different comparability properties. **Outcome/stability metrics** (Change Failure Rate, defect escape rate, MTTR, incident frequency, reopen/reverts) describe what actually happened in production; they are largely process-agnostic and therefore the *only* quality numbers you can honestly compare across a mobile app, a web app, and a backend service. **Internal code-quality metrics** (test coverage, cyclomatic/cognitive complexity, maintainability index, duplication, static-analysis warning density) describe the source itself; they are stack-specific, threshold-arbitrary, and easy to game, so they are meaningful almost exclusively as *trends within a single repository over time*, never as cross-project rankings. The one internal metric with published cross-language validation is CodeScene's **Code Health** (Tornhill & Borg, "Code Red", TechDebt 2022). Throughout, quality metrics earn their keep as **counter-metrics to speed** — paired with throughput so a team cannot win on velocity by quietly shipping defects — and should be read as trends, distinguishing **output → outcome → impact**, rather than as absolute cross-project comparisons.

---

## Framing: Output vs Outcome vs Impact, and why quality is a counter-metric

- **Output** = the immediate product of activity: commits, PRs, releases, story points, lines of code. Cheap to count, weakly correlated with value.
- **Outcome** = the change produced: fewer failures, faster recovery, happier users, less rework. This is where quality lives.
- **Impact** = the durable business/user effect: retention, revenue, cost, reputation.

Two consequences drive the rest of this page:

1. **Quality metrics are counter-metrics ("guardrails") to speed.** Frameworks like DORA and DX's *Core 4* deliberately pair a speed dimension (deployment frequency, lead time) with a quality dimension (change failure rate, recovery time). The pairing is the point: if you optimize throughput alone, teams game it by skipping tests, reviews, and refactoring; a quality counter-metric makes that trade-off visible. A speed metric is only trustworthy when reported *next to* its quality guardrail.
2. **Quality should be read as trends, not absolute cross-project comparisons.** Almost every internal code metric depends on language, framework, domain, and team convention, so a raw score is only interpretable against the same repo's own history. Outcome metrics travel better but still reward same-team trend analysis over league tables (Goodhart's Law: "when a measure becomes a target, it ceases to be a good measure").

---

## Part A — Outcome / Stability Quality Metrics (cross-project comparable)

These describe production reality. Because "did it break, and how fast did we recover" means the same thing on any stack, these are the metrics you *can* compare across projects — and the best guardrails against speed-gaming.

### A1. Change Failure Rate (CFR)
- **Definition:** % of deployments/changes to production that result in degraded service requiring remediation (hotfix, rollback, patch, or fix-forward). A DORA "Four Keys" metric.
- **Data source:** Deployment pipeline (CI/CD) count of releases + incident/rollback tracker. **Without a formal system:** approximate the numerator from git — count of revert commits, hotfix branches/tags, and commits whose message matches `fix|hotfix|revert` that land shortly after a release — divided by total deploys.
- **Cross-stack comparability:** **Yes.** A percentage of failing changes is stack-agnostic; DORA benchmarks (elite ≈ 5%, low ≈ 40% in the 2024 report) apply broadly.
- **Gaming risk:** Redefining "failure" narrowly; batching many changes into one deploy to shrink the denominator; not recording small hotfixes.
- **Counter-signal:** Pair with deployment frequency and MTTR — a suspiciously low CFR beside falling deploy frequency signals under-reporting, not quality.

### A2. Defect Escape Rate / Escaped Defects
- **Definition:** Defects that reach production divided by total defects found (production + pre-production), ×100. Measures how much leaks past your quality gates. Distinct from incidents: an escaped defect is counted whether or not it triggered an incident.
- **Data source:** Issue tracker tagged by environment/severity (Jira/Linear/GitHub Issues), support tickets confirmed as bugs, error monitors (Sentry/Datadog) linked back to the originating release/PR. **Without a formal system:** count issues labeled `bug` created after a release; or use git heuristics — the SZZ-style approach flags bug-fixing commits (message matches `fix/bug/defect`) and treats the fix as evidence a defect escaped; ratio of bug-fix commits to feature commits is a crude escape proxy.
- **Cross-stack comparability:** **Yes, as a ratio** (raw counts are not comparable; the normalized rate is). Benchmarks cluster around low-single-digit-% for strong teams.
- **Gaming risk:** Mislabeling bugs as "improvements/tasks"; not logging production defects; classifying escaped bugs as "won't fix."
- **Counter-signal:** Reopen rate and customer-reported vs internally-found ratio; a low escape rate with a high share of *customer*-reported bugs means detection, not quality, is the problem.

### A3. MTTR / Failed-Deployment Recovery Time (FDRT)
- **Definition:** Time to restore service after a change degrades production. DORA renamed the old "MTTR/Time to Restore" to **Failed Deployment Recovery Time** in 2023 to scope it strictly to change-induced failures (not external outages).
- **Data source:** Incident tool timestamps (detected → resolved). **Without a formal system:** time between a failure-introducing deploy and the corresponding revert/hotfix commit or fix release.
- **Cross-stack comparability:** **Yes.** Elite teams recover in <1 hour; low performers take a week+.
- **Gaming risk:** Closing incidents early; declaring "resolved" at mitigation rather than fix; cherry-picking which incidents count.
- **Counter-signal:** Reopen rate of incidents; incident frequency (fast recovery from constant fires is not health).

### A4. Incident Frequency / Production Bug Rate
- **Definition:** Count of production incidents (or production bugs) per unit time or per release, ideally normalized (per deploy, per active user, per KLOC changed).
- **Data source:** Incident tool / error monitor / issue tracker. **Without a formal system:** frequency of hotfix and revert commits; spikes in error-monitor events per release.
- **Cross-stack comparability:** **Within-repo trend for raw counts; comparable when normalized per-deploy or per-change.** Absolute counts across a 10-person and 200-person system are meaningless.
- **Gaming risk:** Raising the bar for what counts as an "incident"; splitting/merging incidents.
- **Counter-signal:** Severity mix and MTTR — fewer incidents but rising severity is a regression.

### A5. Reopen Rate / Revert Rate
- **Definition:** % of resolved bugs/tickets reopened, or % of merged PRs/commits later reverted. A direct signal of first-time-fix quality and rushed work.
- **Data source:** Tracker (reopen events) and git (`git revert`, revert-message commits). **Without a formal system:** git revert commits are a first-class, always-available signal — no tracker needed.
- **Cross-stack comparability:** **Yes, as a rate.** "% of fixes that didn't stick" is process-agnostic.
- **Gaming risk:** Closing-and-cloning tickets instead of reopening; force-push/amend to hide reverts.
- **Counter-signal:** Escape rate and CFR; used together they triangulate rushed quality.

> **These five are the cross-project-comparable quality core.** They mean the same thing regardless of language or platform, and each acts as a counter-metric to speed.

---

## Part B — Internal Code-Quality Metrics (mostly within-repo trend only)

These describe the *source*. They are useful for guiding refactoring inside a repo, but their thresholds are language- and domain-specific, so **cross-project comparison is invalid** unless a normalized/validated model (see Part C) is used. Test coverage is the canonical Goodhart trap: making it a target produces dummy tests (getters/setters, assertion-free tests) that raise the number without raising quality.

### B1. Test Coverage (line/branch)
- **Definition:** % of code executed by the test suite.
- **Data source:** Coverage tooling in CI (JaCoCo, Istanbul, coverage.py, etc.).
- **Cross-stack comparability:** **Within-repo trend only.** 80% in a UI-heavy mobile app ≠ 80% in a pure-logic backend; framework/generated code distorts it.
- **Gaming risk:** **Very high.** Tests written only to touch lines, without assertions; excluding hard files from the report.
- **Counter-signal:** Escaped defect rate and mutation-test score — coverage measures execution, not verification.

### B2. Cyclomatic Complexity
- **Definition:** Number of independent execution paths through a function (≈ decision points + 1). Also the count of test cases needed for path coverage.
- **Data source:** Static analysis (SonarQube, radon, ESLint plugins, lizard).
- **Cross-stack comparability:** **Within-repo trend only.** Idiomatic thresholds differ by language; a switch-heavy parser looks "complex" but may be simple to read.
- **Gaming risk:** Splitting one function into many trivial ones to lower per-function scores while *raising* overall cognitive load (fragmentation).
- **Counter-signal:** Cognitive complexity and Code Health, which penalize the fragmentation cyclomatic complexity rewards.

### B3. Cognitive Complexity
- **Definition:** SonarSource metric estimating how hard code is to *understand* (penalizes nesting and breaks in linear flow more than raw branch count). Better aligned with human comprehension than cyclomatic complexity; empirically validated as a measure of understandability.
- **Data source:** Static analysis (SonarQube/SonarLint).
- **Cross-stack comparability:** **Within-repo trend only** (thresholds still language-relative).
- **Gaming risk:** Moderate — refactors that hide nesting behind indirection.
- **Counter-signal:** Review depth and defect density on the touched files.

### B4. Maintainability Index
- **Definition:** Composite (Halstead volume + cyclomatic complexity + lines of code, sometimes comments) yielding a 0–100 maintainability score.
- **Data source:** Static analysis (Visual Studio, radon).
- **Cross-stack comparability:** **Within-repo trend only, and weakly.** The formula's constants are dated and language-biased; widely criticized as opaque.
- **Gaming risk:** Shrinking files/LOC to move the index without improving design.
- **Counter-signal:** Code Health / defect density.

### B5. Code Duplication
- **Definition:** % of code that is copy-paste/near-duplicate.
- **Data source:** Static analysis (SonarQube CPD, jscpd, PMD).
- **Cross-stack comparability:** **Within-repo trend only.** Generated code, boilerplate-heavy frameworks, and DSLs inflate it differently per stack.
- **Gaming risk:** Trivial edits to defeat token-matching; over-abstracting to remove "duplication" that was actually independent.
- **Counter-signal:** Change coupling (do the "duplicates" actually change together?).

### B6. Static-Analysis Warning Density
- **Definition:** Linter/analyzer warnings per KLOC (or per file).
- **Data source:** Linters/analyzers (ESLint, SonarQube, RuboCop, SwiftLint, detekt).
- **Cross-stack comparability:** **Within-repo trend only.** Rule sets differ entirely across ecosystems; density is meaningless between a Swift and a TS project.
- **Gaming risk:** Suppressing rules / `// noqa` / baseline-ignoring instead of fixing.
- **Counter-signal:** Trend in *newly introduced* warnings on changed lines (a "ratchet"), plus escaped defects.

### B7. Linter / Type-Check Adoption (binary/coverage)
- **Definition:** Whether the repo enforces a linter, formatter, and static type checking (and what % of code is under strict typing, e.g. TS `strict`, mypy, Sorbet).
- **Data source:** Repo config + CI gate presence; type-coverage tools.
- **Cross-stack comparability:** **Adoption (yes/no) is comparable as a hygiene checklist; the internal % is within-repo.**
- **Gaming risk:** Enabling the tool but disabling most rules / adding blanket ignores.
- **Counter-signal:** Escaped defect rate; strictness level of the config, not just its presence.

> **Why B-metrics don't compare across a mobile vs web app:** thresholds, idioms, generated code, and framework boilerplate differ by language and platform, so the same raw number encodes different realities. The salvageable signal is the **direction of change within one repo** (a "ratchet": don't let it get worse; require new/changed code to meet a bar), never a cross-project leaderboard.

---

## Part C — CodeScene "Code Health": a validated composite

**Code Health** (Adam Tornhill / CodeScene) is the notable exception among internal metrics: a composite maintainability score with published, cross-codebase validation.

- **What it is:** A per-file score from **1 (red) to 10 (green)** aggregating **25+ factors ("code biomarkers")** across three levels — *module smells* (low cohesion, God Class, complex legacy), *function smells* (God Method, DRY violations, complex/large functions), and *implementation smells* (nested/"bumpy road" complexity, complex conditionals, duplicated assertion blocks).
- **Why it's more comparable than raw metrics:** It is calibrated across many languages and codebases rather than being a single raw count, so the 1–10 band carries similar meaning across stacks — though CodeScene still recommends using it primarily to prioritize hotspots and track trends.
- **The validating research — Tornhill & Borg, "Code Red: The Business Impact of Code Quality" (IEEE/ACM TechDebt 2022, Best Paper):** a quantitative study of **39 proprietary production codebases** found that, relative to healthy code, low-Code-Health (red) code is associated with:
  - **~15× more defects** (higher defect density),
  - **~2× longer time-in-development** for a comparable change (and higher *maximum* completion times → unpredictability),
  - lower on-time-delivery likelihood.
  - Reported ROI framing: moving files from ~6.0 → ~8.0 correlates with ~30% faster iteration; a "43% faster development" refactoring benchmark.
- **Independent-ish benchmarking:** CodeScene reports Code Health as substantially more accurate than SonarQube-style scoring when compared against manually reviewed code (their own benchmark — treat vendor-run numbers with the usual caution).
- **Prior validation:** Earlier work links Code Health to file-level defect density and development time; the TechDebt 2022 paper has a public replication package (empear-analytics/code-health-study-tech-debt-2022).

**Takeaway:** Code Health is the internal metric most defensible for cross-project use *because* it was validated across codebases — but treat it as a prioritization/trend tool, and remember the strongest evidence is correlational.

---

## Part D — Review Quality Signals

Process signals about how changes are scrutinized. Useful as guardrails, but all are easily gamed and none prove a reviewer *understood* the change.

| Metric | Definition | Data source | Cross-stack? | Gaming risk | Counter-signal |
|---|---|---|---|---|---|
| **Review coverage** | % of PRs (or files) that received ≥1 review/comment | Git host (GitHub/GitLab) | **Yes** (process metric) | Rubber-stamp approvals; self-review | Escaped defects on reviewed vs unreviewed PRs |
| **Review depth** | Avg review comments per merged PR | Git host | Within-team trend | Comment padding (nits to inflate count) | Defect escape rate; rework after merge |
| **Time-to-review** | PR open → first review | Git host | **Yes** (process metric) | "Reviewing" instantly without reading | Review depth; reopen/revert rate |

- **Cross-stack note:** Review *process* metrics (coverage %, time-to-review) are comparable across stacks because they measure workflow, not code. Depth is better read as a within-team trend.
- **General caution:** These "work best as prompts for team-level investigation," combined with review samples, escaped defects, and rework — not as individual performance scores.

---

## Part E — Test Health Beyond Coverage

Coverage says nothing about whether the suite is *trustworthy*. These do:

| Metric | Definition | Data source | Cross-stack? | Gaming risk | Counter-signal |
|---|---|---|---|---|---|
| **Flaky-test rate** | % of tests with non-deterministic pass/fail under identical conditions | CI history / test analytics | **Yes** (target <2%) | Quarantining/deleting flaky tests instead of fixing | Escaped defects; quarantine count trend |
| **CI pass rate** | % of pipeline runs (or first attempts) that pass | CI | **Yes** (aim ≥95%) | Auto-retries masking real failures; disabling tests | Retry rate; flaky rate |
| **Build stability** | Composite health (pass rate + flakiness + consistency), often 0–100 | CI | Within-repo trend | Excluding unstable jobs from the index | Flaky rate; MTTR of red main |
| **Test execution time** | Wall-clock for the suite | CI | Within-repo trend (aim <~1h for CI regression) | Skipping/parallel-hiding slow tests | Coverage/flakiness (fast because tests were cut?) |

Flaky tests are a *productivity* tax too: teams report 10–20% more throughput after eliminating them (false investigations and reruns vanish). Flake rate is often called the single most important test-instability signal.

---

## Relevance to Our Goal — the quality shortlist

Goal: pick quality metrics that (a) are **stack-agnostic or trend-normalizable** and (b) function as **counter-metrics to speed** so throughput can't be gamed by shipping worse software.

**Tier 1 — Cross-project comparable AND strong speed counter-metrics (use these as guardrails):**
1. **Change Failure Rate** — the definitive speed guardrail; approximable from git (revert/hotfix/fix commits per deploy).
2. **Defect Escape Rate (as a ratio)** — quality of what ships; approximable from `bug`-labeled issues or SZZ-style bug-fix commit heuristics.
3. **MTTR / Failed-Deployment Recovery Time** — resilience; approximable from deploy→revert/hotfix time.
4. **Reopen / Revert Rate** — first-time-fix quality; **always available from git**, no tracker needed.
5. **Incident/production-bug frequency, normalized per deploy** — pairs directly against deployment frequency.

**Tier 2 — Cross-stack process guardrails (comparable, cheap, but rubber-stampable):**
6. **Review coverage (% PRs reviewed)** and **time-to-review** — pair with escaped defects so coverage isn't rubber-stamped.
7. **Flaky-test rate** and **CI pass rate** — comparable, and a direct counter to "go faster by disabling tests."

**Tier 3 — Within-repo trend only (guide refactoring, never rank projects):**
8. **CodeScene Code Health** — the most defensible internal metric cross-project (validated), but best used for trends/hotspots.
9. **Cognitive complexity, duplication, static-analysis warning density** — use as a **ratchet on new/changed code** within each repo; do not compare mobile vs web.
10. **Test coverage** — track its *trend*, never set it as a target (Goodhart); read it beside mutation score/escaped defects.

**Rule of thumb for the wiki:** Report Tier 1–2 as the cross-project quality guardrails paired 1:1 with every speed metric. Report Tier 3 only as per-repo trends with a ratchet. Always show quality *next to* speed so neither can be gamed in isolation.

---

## As-built — the live quality metrics (2026-09)

The shipped dashboards (fleet + per-project artifacts, see [README](README.md)) currently carry **four** quality signals, all computed from git + lockfiles, all as team-size-independent ratios or point-in-time counts, all pooled across a project's repos (numerators and denominators summed, never averaging per-repo averages). Exact as-built definitions:

| Metric | As-built definition | Source | Comparability tier | Notes / limits |
|---|---|---|---|---|
| **Rework Rate** | lines deleted or rewritten **within 21 days** of being added ÷ lines added, per month. Healthy band **15–25%**; there is a *floor* as well as a ceiling (near-0% on an active repo means almost no iteration, often trivial/abandoned work). | git diff lineage | Trend + ratio; the main **AI-slop / instability** proxy | Excludes generated files, lockfiles, images, docs, whitespace-only churn. |
| **Tests with code** | % of **source-changing commits** (frontend/backend) that also change a **test file** (`.test` / `.spec` / e2e / `.feature`). Deliberately **not** coverage %. | git | Stack-agnostic **discipline** ratio → cross-project comparable | Multi-repo blind spot: tests living in a *separate* repo (dedicated e2e/QA repo) don't register as "shipped with code." |
| **Docs with code** | % of source-changing commits that also touch **docs** (`.md` / `.mdx` / `/docs` / `README`). Measures **freshness**, not volume. | git | Discipline ratio → comparable | Doc *usefulness* is invisible to git; needs the DevEx survey. Same separate-repo blind spot. |
| **Vulnerability** | Open known-CVE advisories in third-party dependencies, by highest severity (**critical / high / moderate / low**). Point-in-time from lockfiles; also tracked over time at each month-end state of `main`. | **OSV-Scanner** over lockfiles | **Cross-stack comparable, enforceable** ("drive critical → 0"), hard to game | Ecosystems without a scannable lockfile (**Gradle/Android, .NET/NuGet**) are **not yet covered** → those projects' real exposure is understated. |

**Gaps in the as-built set (feed the quality-index research):**
- **No outcome/stability guardrail is live.** A **change-failure proxy** (revert/hotfix commit share) is computed but reads ~0 because these teams don't use revert/hotfix commit conventions — so the single most important Tier-1 quality metric is effectively missing. Fixing the convention (or adding a code-host/deploy adapter) matters more than any internal metric.
- **No internal structural-quality signal.** None of the four measures the *structure of the source* (complexity, duplication, cohesion). Rework is the closest proxy. This is the real hole the SonarQube question below is pointing at.
- Tests/Docs-with-code are **discipline** signals, Vulnerability is a **security/outcome** signal — a grab-bag across axes rather than a coherent "code quality" construct. Forging them into one **Quality** index (see [fleet-vision.md](fleet-vision.md)) is the next design step.

---

## Decision — SonarQube & internal code metrics (resolved 2026-09-14)

**Question raised:** are Rework / Tests-with-code / Docs-with-code / Vulnerability enough for code quality, or should we add SonarQube code-quality metrics?

**Decision:** **Do not put SonarQube (or any Part-B internal code metric) on the cross-project front-face.** It directly conflicts with the four locked design constraints:

| Constraint (see [README](README.md) / [synthesis](synthesis.md)) | SonarQube-style internal metrics |
|---|---|
| **Stack-agnostic** | Need a per-language analyzer; coverage/quality varies by stack; some of our stacks (Terraform, Gradle specifics) are weakly covered — **not uniform** across web/mobile/desktop. |
| **Cross-project comparable** | **Invalid** — complexity/duplication/warning-density thresholds are language- and domain-relative (this page, Part B). "15 cyclomatic" means different things in Kotlin vs TS. Only within-repo trend is meaningful. |
| **Gaming-resistant** | Suppress rules, baseline-ignore, over-split functions to cut per-function complexity — several land on the [synthesis §8 banned list](synthesis.md). |
| **Git-only, buildable in-house** | A server + per-repo config dependency outside the git spine. |

**But the instinct is right — there is a genuine structural-quality hole** (see as-built gaps). We fill it **without** breaking the design:

1. **Off the front-face.** Any internal structural metric (complexity, duplication) is **within-repo trend only**, applied as a **ratchet on new/changed code** ("don't let it get worse"), never a cross-project leaderboard number. It may feed a project's own **Quality index** as a *trend-vs-baseline* component, not as a raw cross-project value.
2. **Prefer a validated composite over raw SonarQube metrics.** Where we want a maintainability layer, **CodeScene Code Health** (Part C) is more defensible than SonarQube for cross-project use because it has published cross-codebase validation ("Code Red", TechDebt 2022); SonarQube's ratings have no equivalent validation. Universal OSS CLIs (`lizard`, `scc`, `jscpd`) are the cheap in-house alternative for a trend-only complexity/duplication signal. *(Build-vs-buy under active research — see [quality-index.md](quality-index.md).)*
3. **Add the outcome guardrail first.** A working **change-failure / revert rate** closes a bigger quality gap than SonarQube would, is cross-project comparable, and is git-derivable — do it before adding any internal code metric.

**Net:** SonarQube is not adopted as a fleet metric. Internal code metrics are admitted only as per-project, trend-only, ratchet-style inputs, and CodeScene Code Health is the preferred vehicle if we buy a maintainability layer. This is the resolution of open decision — internal-metrics question.

---

## Sources

- CodeScene — *Code Health metric* (product): https://codescene.com/product/code-health
- CodeScene — *Measuring the business impact of low code quality*: https://codescene.com/blog/measuring-the-business-impact-of-low-code-quality
- CodeScene — *What is Code Health / biomarkers*: https://codescene.com/blog/code-biomarkers/ and https://codescene.com/blog/measure-code-health-of-your-codebase
- CodeScene — *Refactoring speeds development by 43% (benchmark)*: https://codescene.com/blog/benchmarking-code-health-refactoring-roi
- Tornhill & Borg, *Code Red: The Business Impact of Code Quality* (TechDebt 2022) replication package: https://github.com/empear-analytics/code-health-study-tech-debt-2022
- CodeScene docs — *Code Health guide*: https://codescene.io/docs/guides/technical/code-health.html
- DORA — *A history of DORA's software delivery metrics*: https://dora.dev/insights/dora-metrics-history/
- Change Failure Rate & MTTR guide (CodePulse): https://codepulsehq.com/guides/change-failure-rate-mttr-guide
- Bug/Defect Escape Rate — Count.co: https://count.co/metric/bug-escape-rate ; DevStats: https://www.devstats.com/glossary/escaped-defects ; Opsera: https://opsera.ai/knowledge-base/change-and-quality-metrics/what-is-defect-escape-rate-der-and-why-it-matters-a-comprehensive-guide/
- *What Makes Software Bugs Escape Testing?* (empirical study, arXiv): https://arxiv.org/html/2604.26672
- Sonar — *Cognitive Complexity: because testability != understandability*: https://www.sonarsource.com/blog/cognitive-complexity-because-testability-understandability/
- *Empirical Validation of Cognitive Complexity* (arXiv): https://arxiv.org/pdf/2007.12520
- Sonar — *Cyclomatic Complexity guide*: https://www.sonarsource.com/learn/cyclomatic-complexity/
- LinearB — *Cyclomatic Complexity (how it misleads)*: https://linearb.io/blog/cyclomatic-complexity
- Packmind — *Cyclomatic vs Cognitive Complexity (Goodhart, over-splitting)*: https://dev.to/packmind/cyclomatic-complexity-and-cognitive-complexity-4c03
- Output vs Outcome vs Impact — Product Masterclass: https://www.product-masterclass.com/blog/output-vs-outcome-vs-impact ; LeadDev: https://leaddev.com/velocity/focus-outcomes-over-outputs
- DX — *Core 4 / measuring velocity without gaming*: https://getdx.com/blog/developer-velocity/
- Review metrics — LinearB *Review Depth*: https://linearb.helpdocs.io/article/v9s73q7d93-review-depth-metric ; Weave *Review coverage*: https://weaveos.com/glossary/review-coverage ; gitrolysis *PR metrics that improve quality*: https://gitrolysis.com/posts/2025/11/pull-request-metrics-that-actually-improve-code-quality/
- Test health — minware *Flaky Test Rate*: https://www.minware.com/guide/metrics/flaky-test-rate ; EM-Tools *Test Pass Rate*: https://www.em-tools.io/engineering-metrics/test-pass-rate ; Harness *Flaky Tests*: https://www.harness.io/blog/flaky-tests-the-quiet-killer-of-productivity-in-your-ci-pipeline
