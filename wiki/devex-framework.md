# The DevEx Framework & DX Core 4

## Summary

**DevEx (Developer Experience)** is a developer-centric model of productivity introduced by Abi Noda, Margaret-Anne Storey, Nicole Forsgren, and Michaela Greiler in the 2023 ACM Queue paper *"DevEx: What Actually Drives Productivity."* It distills roughly 25 sociotechnical factors into **three dimensions — Feedback Loops, Cognitive Load, and Flow State** — and argues you must measure each one **twice**: once as a *perception* (how developers feel, via surveys) and once as a *workflow* signal (what systems actually do, via telemetry), because the two frequently disagree. **DX Core 4** (GetDX, published late 2024 and refined through 2025/2026) is the operating model built on top of DevEx: it wraps DevEx together with **DORA** and **SPACE** into **four counterbalanced dimensions — Speed, Effectiveness, Quality, Impact** — each with a headline metric and secondary metrics. Its two signature metrics are the **Developer Experience Index (DXI)**, a survey composite of 14 drivers used for Effectiveness, and **Diffs per Engineer (DPE)**, a deliberately blunt, executive-legible throughput number for Speed. The framework's core design principle is *anti-gaming by construction*: pair a speed metric with a quality metric and a satisfaction/experience metric so that gaming any one dimension shows up as damage in another. This lineage — **DORA (2018) → SPACE (2021) → DevEx (2023) → DX Core 4 (2024)** — is threaded together by Nicole Forsgren.

---

## Part 1 — The DevEx framework (three dimensions)

### The three dimensions

| Dimension | Definition | Why it drives productivity |
|---|---|---|
| **Feedback Loops** | The **speed and quality of responses to developers' actions** (running tests, opening a PR, deploying). | Fast, high-quality feedback lets developers iterate and course-correct quickly. Slow loops force context switching and waiting; the cost compounds across every cycle in a day. |
| **Cognitive Load** | The **amount of mental processing required to perform a task** — understanding systems, tools, and processes. | High *extraneous* load (confusing platforms, poor docs, sprawling systems) steals mental capacity from the actual problem. Lowering it frees capacity for real engineering. |
| **Flow State** | A **mental state of full immersion and focus** in the work. | Frequent, satisfying flow correlates with higher productivity, better learning, and innovation. Interruptions, fragmented calendars, and unplanned work destroy it. |

**Practical appeal:** each dimension maps to something a leader can actually change. Slow CI is a *feedback loop* problem; a confusing internal platform is a *cognitive load* problem; a calendar full of status meetings is a *flow state* problem.

### Measure both perceptions AND workflows

The paper's central methodological claim: for **each** dimension, capture two kinds of measure, because they routinely diverge.

- **Perceptions** (attitudinal, from surveys) — how developers *feel* about the experience. Subjective, but often the leading and most honest signal.
- **Workflows** (behavioral/system, from telemetry) — what the systems *actually do*. Objective, but blind to how it feels.

The paper's own illustrations of the gap:
- Code review turnaround can look fast in the data yet *feel* disruptive if reviews keep interrupting work.
- Developers can *feel* content with a build process while workflow data shows the feedback loop is objectively slow.

The recommendation is to combine both (and add a third human lens — KPIs/outcomes — where possible), never to rely on system data alone.

### Concrete example metrics for each DevEx dimension

| Dimension | Perception (survey) | Workflow / system telemetry |
|---|---|---|
| **Feedback Loops** | Satisfaction with speed of CI/automated tests; perceived disruptiveness of code reviews; satisfaction with deploy process | **Code review turnaround** (PR open → first substantive review); **CI/build duration & reliability**; **lead time for changes** (commit → prod); **deploy time / deployment frequency**; PR cycle length; ticket/response times |
| **Cognitive Load** | Ease of finding information/docs ("I can find what I need in <5 min"); perceived complexity of the codebase/platform; ease of understanding systems | **Time to onboard** (first meaningful change shipped); **documentation coverage/findability**; **code/deploy complexity** (manual steps to ship); services owned per developer; number of distinct collaborators required |
| **Flow State** | Frequency/ability to reach flow over a 2-week window; perceived control over the day; satisfaction with focus time | **Focus time** (uninterrupted blocks ≥90 min/week); **meeting load** (hours/week, recurring vs ad-hoc); **interruptions** during deep work; context switches; fragmented/"lost" time (sub-1-hour blocks) |

---

## Part 2 — DX Core 4 (the unified operating model)

DX Core 4 is GetDX's productivity operating model that **explicitly unifies DORA + SPACE + DevEx** into one measurable system. Its motivation: DORA is prescriptive but narrow (delivery only), SPACE is comprehensive but hard to operationalize, and DevEx focuses on experience without the broader productivity/business picture. Core 4 takes the strengths of each and organizes them into **four counterbalanced dimensions**, each with one **primary (headline) metric** plus **secondary metrics**.

### The four quadrants

| Dimension | Primary metric | Secondary metrics | Data source | Gaming risk & guard |
|---|---|---|---|---|
| **Speed** — pace of work through the system | **Diffs per Engineer (DPE)** / PRs per engineer per week (90-day lookback, bots excluded, normalized per contributor) | Lead time; deployment frequency; **perceived rate of delivery** (survey); PR cycle time; time to first review | System (git/SCM); one survey signal | **Highest gaming risk** (split PRs, inflate count). Guard: team/org only — never individual, never a target/reward; counterbalanced by DXI + Change Failure Rate + perceived quality. |
| **Effectiveness** — developer experience & friction | **Developer Experience Index (DXI)** — composite of 14 survey drivers | Time to 10th PR (onboarding); Ease of Delivery; regrettable attrition | **Survey / self-reported** (no system equivalent yet) | Hard to game individually; acts as the *cultural safeguard* — if speed pressure hurts developers, DXI drops. Risk: lags real change; is a proprietary composite ("black box" critique). |
| **Quality** — reliability of changes | **Change Failure Rate (CFR)** — % of prod changes that degrade service | Failed-deployment recovery time (≈MTTR); **perceived software quality** (survey); operational health & security metrics; defect ratio (% of PRs that are bug fixes) | System + survey | Counterbalances Speed directly — shipping faster but breaking prod raises CFR. Risk: under-reporting incidents; guard via consistent incident definitions. |
| **Impact** — business value / innovation mix | **% of time spent on new capabilities** (Innovation Ratio, 90-day lookback) | Initiative progress & ROI; revenue per engineer; R&D as % of revenue | Survey / self-reported (system classification "coming") | Guards against "fast but only firefighting." Risk: subjective classification of new-vs-maintenance work. |

### The Developer Experience Index (DXI)

- A **composite score built from 14 standardized Likert-scale survey items** ("drivers"), each a known lever of engineering efficiency. Named/known drivers include: **ease of release, cross-team collaboration, build & test, documentation, code review** (plus focus/deep-work time, CI/CD, requirements clarity, codebase health, tooling, onboarding, and related factors — GetDX markets 14 drivers, publicly naming a subset).
- Serves as the **Effectiveness** headline and the framework's **anti-gaming anchor**.
- GetDX's headline claim: **a one-point DXI improvement ≈ 13 minutes saved per developer per week (~10 hours/developer/year)**, used to translate experience into ROI.
- Cadence: **quarterly or semi-annual** survey. Benchmarks drawn from **4M+ data samples across thousands of organizations**, segmented by industry, region, and company size.

### Diffs per Engineer (DPE) — controversy and safeguards

**Why it exists.** DPE is deliberately blunt because it is *executive-legible*: telling a CFO "each engineer ships ~4.3 changes per week" moves the conversation in a way "lead time" cannot. It is used at Meta, Microsoft, and Uber.

**The controversy.** DPE is a per-engineer throughput number, and critics note it is trivially gameable — engineers can split work into **smaller, more frequent PRs** to inflate the count without producing more value, incentivizing quantity over quality, fragmentation, and tech debt. It does not natively normalize for complexity. (A forthcoming *TrueThroughput®* aims to weight diffs by AI-assessed complexity.) Researcher **John Flournoy** adds that context matters — for a mature product, *slower/more deliberate* may be correct, so DPE is not a "higher is always better" number.

**The three preconditions GetDX attaches to DPE (the guardrails):**
1. **Counterbalance it** with oppositional metrics — above all the DXI, plus Change Failure Rate.
2. **Never set targets or rewards** tied to it.
3. **Roll it out and communicate it carefully**, at **team/organization level only — never the individual**, and never tied to performance reviews.

The stated logic: *"If there are cultural problems created with diffs per engineer, they will be reflected in the Developer Experience Index."*

### The anti-gaming design: pair speed WITH quality WITH satisfaction

This is the philosophical heart of DX Core 4. The dimensions are **intentionally oppositional**: pushing one tends to strain another, so no single metric can be safely maximized in isolation.

- Push **Speed** (DPE) recklessly → **Quality** (CFR) worsens and/or **Effectiveness** (DXI) drops.
- Optimize **Quality** alone → **Speed** stalls.
- Every dimension **pairs a "hard" system metric with a "human" perceptual metric**, so the number and the lived reality stay connected.

Because gaming one dimension surfaces as damage in another, the *balanced set itself is the anti-gaming mechanism* — you read the four together, never one alone.

### How Core 4 maps back to DORA / SPACE / DevEx

- **DORA** → chiefly the Speed and Quality dimensions (lead time, deployment frequency, change failure rate, failed-deployment recovery time).
- **SPACE** → satisfaction/well-being, performance, activity, communication, efficiency — surfaced as the perceptual metrics (perceived delivery rate, perceived quality) and Effectiveness.
- **DevEx** → the Effectiveness dimension and the *measure-perception-and-workflow* discipline, operationalized as the DXI (feedback loops, cognitive load, flow all live inside its drivers).
- **New in Core 4** → the **Impact** dimension (percentage of time on new capabilities, revenue/engineer, R&D % of revenue), which frames engineering in direct business terms — its most novel contribution.

### Rollout, cadence, and benchmarking guidance

- **Start with baselines, don't stall for perfect telemetry.** Establish self-reported baselines via surveys ("DX Snapshots") where system instrumentation isn't ready; layer in system metrics as integrations land. Survey-based rollout is achievable in **weeks, not months**.
- **Cadence:** survey/perceptual metrics **quarterly** (semi-annual acceptable); system metrics update continuously as leading indicators.
- **Level:** report at **team and organization level, never the individual** — a hard rule, not a suggestion.
- **Benchmark to the 75th percentile** as an initial goal (top-quartile performance). Benchmarks are segmented tech vs non-tech and by eng org size (<100, 100–500, 500–2500, 2500+), reported at P50/P75/P90.
- **Example benchmark band** (tech companies, <100 engineers): **DPE ≈ 3.8–5.5 PRs/week**, **DXI ≈ 66–83**, **CFR ≈ 3.0–4.0%**, **Innovation ratio ≈ 58–65%** across percentiles.
- **Communicate transparently** how metrics are collected and used — framed as *reducing developer friction*, not surveilling individuals.

---

## Relevance to our goal

Our goal is a **stack-agnostic, git-derivable, anti-gaming** scorecard. DevEx/DX Core 4 map onto that as follows.

### What is git-derivable and stack-agnostic (no survey required)

These signals can be computed from SCM/git + CI + deploy metadata alone, across any language or stack:

| Signal | DevEx / Core 4 home | Git/CI-derivable? |
|---|---|---|
| Code review turnaround (PR open → first review; open → merge) | Feedback Loops / Speed | **Yes** — PR timestamps |
| PR cycle time, PRs (diffs) per engineer, PR size | Speed | **Yes** — SCM |
| Lead time for changes (commit → deploy) | Feedback Loops / Speed | **Yes** — commit + deploy timestamps |
| Deployment frequency | Speed / DORA | **Yes** — deploy events |
| CI build duration & pass/fail reliability | Feedback Loops | **Yes** — CI logs |
| Change Failure Rate; failed-deployment recovery time | Quality | **Partly** — needs deploy + incident/revert signal |
| Defect ratio (% PRs that are bug fixes) | Quality | **Partly** — needs PR labels/classification |
| Time to onboard (first/10th PR by a new author) | Cognitive Load / Effectiveness | **Yes** — first-commit dates from git history |
| % time on new capabilities (via commit/PR classification) | Impact | **Partly** — needs labeling heuristics/AI |

### What is fundamentally survey-dependent (honest limitation)

**Cognitive load and flow state are mostly *felt*, not logged.** The following have **no reliable git equivalent** and require surveys or calendar/comms telemetry we may not have:
- **Effectiveness / DXI** — entirely self-reported; there is *no* system proxy today.
- **Cognitive load perception** — doc findability, perceived complexity, "can I understand this system."
- **Flow state** — focus time, interruptions, meeting load (needs calendar data, not git).
- **Perceptual counterparts** of every dimension (perceived delivery rate, perceived quality).

So a purely git-derived scorecard can populate **Speed and Quality strongly, Cognitive Load/Effectiveness weakly (onboarding time only), and Flow State barely.** We should be explicit that git signals cover the *workflow* half of DevEx and are structurally blind to the *perception* half — which the framework's authors consider equally important. Where we cannot survey, we should label those dimensions as *partially observed*, not claim to measure DevEx wholesale.

### How "pair speed with quality with satisfaction" informs our anti-gaming scorecard

The single most transferable idea: **never score a throughput metric alone.** Build the scorecard as a **balanced, oppositional set** so gaming one axis visibly degrades another:

1. **A speed axis** (PRs/diffs per engineer, lead time) — the gameable one.
2. **A quality counterweight** (change failure rate, defect ratio, revert rate) — splitting/rushing PRs to inflate speed should raise this.
3. **A satisfaction/health counterweight** (ideally a lightweight survey/DXI-style signal; if surveys are impossible, a git-derivable proxy such as rework rate, review-load concentration, or after-hours commit share) — pressure to hit speed targets should show here.

Concrete rules to carry over:
- **Report team/org level, never individual.** DPE at the person level is the framework's cardinal sin.
- **Never turn a metric into a target or reward** (Goodhart / DPE guidance).
- **Read the set together.** A speed gain is only "real" if quality and satisfaction hold. Flag any speed improvement that co-occurs with a quality or health regression as *suspected gaming*.
- **Normalize for size/complexity** where possible (PR size, complexity weighting) to blunt the "many tiny PRs" exploit.

---

## Sources

- Noda, Storey, Forsgren, Greiler — *DevEx: What Actually Drives Productivity*, ACM Queue Vol. 21 No. 2, 2023: https://dl.acm.org/doi/10.1145/3595878 and https://queue.acm.org/detail.cfm?id=3595878
- *DevEx in Action: A study of its tangible impacts*, ACM Queue 2023/Microsoft Research: https://queue.acm.org/detail.cfm?id=3639443 · https://www.microsoft.com/en-us/research/publication/devex-in-action-a-study-of-its-tangible-impacts/
- Develocity — summary of the DevEx paper: https://develocity.io/a-summary-devex-what-actually-drives-productivity-by-noda-et-al-2023/
- Worklytics — DevEx metrics (concrete perception/workflow metric lists): https://www.worklytics.co/blog/developer-experience-a-developer-centric-approach-to-productivity
- Abi Noda — *Introducing the DX Core 4* (GetDX newsletter): https://newsletter.getdx.com/p/introducing-the-dx-core-4
- GetDX — *Applying the Core 4 framework (Part 2)*: https://newsletter.getdx.com/p/applying-the-core-4-framework-part
- GetDX docs — *Guide to the DX Core 4*: https://docs.getdx.com/dx-core-4/ · https://docs.getdx.com/dx-core-4.md
- GetDX — *Measuring developer productivity with the DX Core 4*: https://getdx.com/research/measuring-developer-productivity-with-the-dx-core-4/
- GetDX — *Guide to the Developer Experience Index (DXI)*: https://getdx.com/blog/guide-to-developer-experience-index/ · https://getdx.com/developer-experience-index/
- GetDX — *DX Core 4 Benchmarks*: https://getdx.com/research/benchmarks/
- InfoQ — *DX Unveils New Framework for Measuring Developer Productivity*: https://www.infoq.com/news/2025/01/dx-core-4-framework/
- LeadDev — *How DX Core 4 aims to unify developer productivity frameworks*: https://leaddev.com/reporting/dx-core-4-aims-to-unify-developer-productivity-frameworks
- LinearB — *DX Core 4 deep dive* (full secondary-metrics table + gaming critique): https://linearb.io/blog/dx-core-4-deep-dive
- DX Heroes — *DX Core 4: the framework for measuring developer experience*: https://dxheroes.io/insights/dx-core-4
- Brian Houck / GetDX — *Revisiting the DX Core 4 in the Age of AI*: https://getdx.com/blog/revisiting-the-dx-core-4-in-the-age-of-ai/
