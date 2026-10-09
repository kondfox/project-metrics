# Goodhart's Law, Metric Gaming, and Gaming-Resistant Measurement

## Summary

Any single metric used to measure software developer productivity will eventually be gamed: this is the practical consequence of **Goodhart's Law** — "when a measure becomes a target, it ceases to be a good measure." Developers (and, increasingly, AI agents) optimize whatever number is watched, so lines-of-code targets breed bloat, PR-count targets breed trivial PRs, ticket-closure targets breed ticket-splitting, and coverage targets breed assertion-free tests. Decades of critique — Martin Fowler's *CannotMeasureProductivity*, Tom DeMarco's public recantation of "you can't control what you can't measure," W. Edwards Deming's insistence that ~95% of outcomes are the system not the individual, and the 2023 Beck/Orosz rebuttal of McKinsey — converge on the same conclusion: measure **outcomes not outputs**, measure **the system not the individual**, and never let one number stand alone. The reliable defense, borrowed from product analytics, is the **guardrail / counter-metric**: pair every throughput metric with a quality or stability metric that moves in the *opposite* direction when the first is gamed, then triangulate several independent signals and watch trends rather than absolutes. This page catalogues the failure modes, states the design principles, and gives a concrete "tempting metric → how it's gamed → counter-metric" table.

---

## 1. The three foundational "laws"

### Goodhart's Law
Formulated by economist Charles Goodhart (1975) and sharpened by anthropologist Marilyn Strathern into its famous phrasing: **"When a measure becomes a target, it ceases to be a good measure."** A metric is a *proxy* for something we actually care about (quality, value, productivity). The moment the proxy is rewarded, effort flows into moving the proxy rather than improving the underlying reality, and the correlation that made the proxy useful collapses.

### Campbell's Law
Social scientist Donald T. Campbell (1979): **"The more any quantitative social indicator is used for social decision-making, the more subject it will be to corruption pressures and the more apt it will be to distort and corrupt the social processes it is intended to monitor."** Campbell's emphasis is that corruption comes not only from the people being measured but from everyone in the system, including those who administer the measurement.

### The Cobra Effect (perverse incentives)
Coined by economist Horst Siebert from a British-Raj anecdote: a bounty on dead cobras led people to *breed* cobras for the reward; when the scheme was cancelled the breeders released their now-worthless snakes, leaving more cobras than before. The general form is a **perverse incentive** — an incentive structure that produces the opposite of its intent. In engineering: rewarding bug *fixes* can incentivize shipping bugs to fix later; rewarding tickets-closed incentivizes splitting and superficial closes.

> These three describe the same trap from three angles: Goodhart (the metric decays), Campbell (the process corrupts), Cobra (the incentive inverts).

---

## 2. Why "you can't measure programmer productivity" — the classic critiques

### Martin Fowler — *CannotMeasureProductivity* (2003)
Fowler's core argument: **we cannot measure productivity because we cannot measure software's output.** Productivity = value / effort, and we have no reliable measure of the numerator.
- **LOC is backwards.** Well-designed, refactored code is *shorter* because it removes duplication; copy-paste programming inflates line counts. Two developers who build identical systems, one in 100,000 LOC and one in 10,000, are ranked exactly wrong by LOC.
- **Function Points don't rescue it.** Different certified counters applying the same standard vary by ~300%. And even a correct count misleads: delivering 100 function points is worthless if only 30 are useful to the customer, while a colleague's 50 are all useful — the colleague's *true* productivity is higher.
- **Output ≠ outcome.** One developer may ship features worth \$5M; another fewer features worth \$10M. Fowler's conclusion: **"false measures only make things worse."** Businesses manage plenty of hard-to-measure functions (legal, marketing) without inventing fake numbers.

### Tom DeMarco — recanting his own maxim (2009)
DeMarco coined "You can't control what you can't measure" (*Controlling Software Projects*, 1982). In IEEE Software's *"Software Engineering: An Idea Whose Time Has Come and Gone?"* (2009) he asked whether that advice was still correct, still relevant, and whether metrics are a must — and answered **"no, no, and no."** Software projects are fundamentally *experimental* and only loosely controllable; strict measurement-and-control can actively steer teams away from the high-value, exploratory work that produces things like Google Earth or Wikipedia. Tight control optimizes cost, but the projects that matter are the ones where value, not cost, dominates.

### W. Edwards Deming — the misquote and the real point
The popular slogan **"you can't manage what you can't measure"** is *not* Deming (nor really Drucker). Deming said almost the opposite: **"It is wrong to suppose that if you can't measure it, you can't manage it — a costly myth."** (*The New Economics*, 1993.) His deeper teaching: **~95% of performance variation comes from the system, not the individual.** Ranking, rating, and firing individuals for what the system produces ("blame the person, fire the person, then wonder why the next person has the same problem") is a management error. This directly undercuts individual productivity scoreboards.

### The McKinsey controversy (2023) and the Beck/Orosz rebuttal
McKinsey published *"Yes, you can measure software developer productivity,"* proposing an effort/output-based framework (including opportunity-focused and "inner/outer loop" activity metrics) already used by ~20 companies.

**Kent Beck and Gergely Orosz** replied in a two-part *Pragmatic Engineer* series. Key criticisms:
- The framework measures **effort and output**, but value creation runs **Effort → Output → Outcome → Impact.** Measuring the early, easy-to-count stages misses the half that customers and executives actually care about, and *invites gaming.*
- **Individual metrics backfire.** From Beck's Facebook experience: once survey scores became targets, "managers started negotiating with individual contributors — *'Give me a 5 & I'll make sure you get an exceeds-expectations.'*" Goodhart in the wild.
- **Real gaming already observed** (Uber-style dashboards): when diff-count and review-count became visible targets, engineers produced *more, smaller* diffs and drove up CI cost — activity rose, outcomes did not.
- **Danger in downturns:** productivity scores creep into performance reviews and layoff decisions, distorting them further. A common (unspoken) reason CTOs want the numbers is to decide whom to fire — exactly the Deming failure mode.
- Their prescription: measure closer to outcomes/impact (e.g., a customer-facing deliverable per team per week; committed business impact), accepting the harder attribution rather than retreating to easy-to-game activity counts. They point to **DORA** (outcome-oriented) and **SPACE** (multi-dimensional, explicitly warns against LOC) as better-shaped than McKinsey's model.

---

## 3. The master table — tempting metric → how it's gamed → counter-metric

> The design rule behind this table: **the counter-metric must be something that gets *worse* precisely when the first metric is gamed.** If you can raise X only by degrading Y, then watching X and Y together removes the incentive to cheat.

| Tempting metric (X) | How it gets gamed | Counter-metric / guardrail (Y) that catches it |
|---|---|---|
| **Added lines of code** | Verbose code, copy-paste, avoiding refactors, dead abstractions → **bloat** | Change failure rate; code churn / rework rate; maintainability & duplication trend; reviewer-rated code quality; feature value delivered |
| **Commits** | Micro-commits, trivial "fix typo" commits, commit padding | Deployment frequency to prod; % commits tied to a delivered outcome; PR revert rate |
| **Pull-request count** | Splitting work into many **tiny PRs**; cosmetic PRs | PR size distribution *and* review latency; % PRs behind a shipped feature; change failure rate; rework rate |
| **Tickets / issues closed** | **Ticket-splitting**, closing without truly resolving, re-opens | Reopen rate; escaped-defect / production incident rate; customer-reported bug rate; cycle time on *meaningful* tickets |
| **Story points / velocity** | **Point inflation** (estimate the same work higher over time) | Delivered outcome / business impact per sprint; predictability (committed vs done) trend, not absolute velocity; DORA lead time |
| **Test coverage %** | **Assertion-free tests**, tests that execute but verify nothing, testing trivial getters | **Mutation score** (do tests fail when code is broken?); escaped-defect rate; change failure rate |
| **Bug counts (found/fixed)** | Gaming *found*: log trivial bugs. Gaming *fixed*: ship bugs to fix later (Cobra) | Escaped defects reaching customers; defect severity mix; MTTR; reopen rate; % dev time on new capabilities vs firefighting |
| **Documentation volume** | **Over-documentation**, auto-generated filler, docs nobody reads | Doc usefulness / findability (developer-experience survey); time-to-onboard; support-ticket deflection; doc staleness rate |
| **Code-review comment count** | Nitpick padding, bikeshedding to hit a "thoroughness" number | Review latency + change failure rate of reviewed code; defect-escape rate; developer-experience/satisfaction on reviews |
| **AI token usage / % AI-assisted** | **"Tokenmaxxing"** — burning tokens for the leaderboard, forced AI use, **AI slop** (polished but wrong code) | Merged output & value per token (cost per merged PR); change failure rate; review-and-rework time on AI code; escaped defects; developer satisfaction |
| **Deployment frequency (alone)** | Deploy trivial/empty changes to inflate the count | **Change failure rate + time-to-restore** (the DORA stability pair); rework rate |
| **Individual "productivity score"** | Every behavior above, plus survey-score negotiation | Don't score individuals. Use team/system metrics + qualitative context (Deming) |

**AI-specific note.** Token usage is "the new lines of code": it measures an *input*, not output or outcome. Observed data shows cost-per-merged-PR ranging from ~\$0.28 (lightest token users) to ~\$89 (heaviest) — heavy usage does not return proportional value. METR's 2025 study even found experienced developers on familiar codebases were ~19% *slower* with AI tools once review-and-correction time was counted. Amazon reportedly had to tell staff to stop "using AI just for the sake of using AI" to climb internal leaderboards — a live Cobra Effect.

---

## 4. Design principles for gaming-resistant metric sets

1. **Pair every throughput metric with an opposing quality/stability counter-metric.** The gold standard is the four DORA metrics: two throughput (deployment frequency, lead time for change) explicitly *counterbalanced* by two stability metrics (change failure rate, time-to-restore). You cannot inflate speed without the stability pair showing the damage. DX Core 4 builds the same "oppositional metrics" idea in by design (see §5).

2. **Measure outcomes and impact, not effort and output.** Follow the chain Effort → Output → Outcome → Impact and push measurement as far right as attribution allows. Counting activity is easy and gameable; counting delivered customer/business value is hard and robust. (Fowler, Beck/Orosz, DORA.)

3. **Measure the team and the system, not the individual.** Deming: ~95% of variation is systemic. Individual scoreboards invite gaming, punish people for system faults, and corrupt performance reviews. DX Core 4 explicitly marks its speed metric (diffs per engineer) as **not** for individual evaluation.

4. **Triangulate multiple independent signals.** No single number is trustworthy; a *set* that would all have to move together to fake progress is far harder to game. SPACE (Satisfaction, Performance, Activity, Communication, Efficiency) and DX Core 4 (Speed, Effectiveness, Quality, Impact) exist precisely to force multi-dimensional reads.

5. **Prefer ratios and normalized metrics over raw counts.** Raw counts (LOC, commits, PRs, tickets, tokens) reward volume. Ratios — value per token, defects per release, rework as a % of capacity, committed-vs-delivered — reward efficiency and are much harder to inflate by doing more low-value work.

6. **Combine perceptual (survey) and system (telemetry) data.** Self-reported experience catches what instrumentation misses and vice-versa; gaming one source shows up as divergence from the other. DX Core 4's Developer Experience Index (14 Likert items) deliberately sits alongside hard system data.

7. **Watch trends and distributions, not absolute values or point-in-time snapshots.** A velocity of "42" is meaningless; a *predictability trend* is informative. Watch PR-size *distributions*, not average PR count. Sudden metric jumps after a metric becomes visible are the fingerprint of gaming, not improvement.

8. **Keep some metrics for reflection, not as targets — separate "north star" from "guardrail."** The instant a metric becomes a target it starts to decay (Goodhart). Use most engineering metrics for team *reflection and conversation* ("why did rework spike?"), reserve a single high-level north star for direction, and use guardrails as non-negotiable *thresholds* rather than things to maximize. If a number is never a target, it stays honest.

---

## 5. Guardrail metrics, counter-metrics, and balanced scorecards — the product-analytics import

Product analytics solved a version of this problem first, and its vocabulary maps cleanly onto engineering.

- **North Star metric** — the single measure of primary success the org steers toward (e.g., activated users, delivered business impact).
- **Guardrail / counter-metric** — secondary metrics monitored *while* chasing the north star, "acting as guardrails, preventing you from going too far in pursuit of your North Star and causing unintended harm." Classic product guardrails: churn, support-ticket volume, NPS, latency, refund rate. They function as **non-negotiable thresholds**: if a guardrail crosses its line, you pause and rethink, regardless of what the north star did.
- **Balanced scorecard** — a deliberately multi-category set (business, user-experience, strategic) so no single dimension can be optimized in isolation.

**Mapping to engineering:**

| Product-analytics concept | Engineering equivalent |
|---|---|
| North Star | Business impact / % time on new capabilities (DX Core 4 "Impact") |
| Throughput/growth metric | Deployment frequency, lead time, diffs per engineer |
| Guardrail / counter-metric | Change failure rate, time-to-restore, rework rate, escaped defects, developer-experience index |
| Balanced scorecard | DORA (throughput × stability), SPACE (5 dimensions), DX Core 4 (Speed × Effectiveness × Quality × Impact) |

**DX Core 4 (Abi Noda / Laura Tacho, DX, 2024)** — unifies DORA + SPACE + DevEx into four balanced dimensions, one key metric plus three secondary each:
- **Speed** — Diffs per engineer (explicitly *not* for individuals) — must be "counterbalanced with oppositional metrics."
- **Effectiveness** — Developer Experience Index (perceptual, 14 Likert items).
- **Quality** — Change Failure Rate — "ensures a focus on speed does not come at the cost of stability."
- **Impact** — % of time spent on new capabilities.
The design intent is exactly principle #1: pushing one dimension while gaming shows up as a regression in another.

**DORA 2025** even adds an explicit gaming-catcher: **rework rate** — the share of deployments that are unplanned and triggered by production incidents — surfacing capacity spent fixing instead of building.

---

## 6. Relevance to our goal

The user's candidate metrics and their specific failure modes, each with the guardrail that neutralizes it:

- **Added LOC → bloat.** Never reward volume of code. Pair with change failure rate, code churn/rework, duplication and maintainability trend, and (ultimately) delivered value. Remember Fowler: the *better* solution is usually the *smaller* one, so an LOC target rewards the wrong direction outright.
- **PR count → tiny PRs.** Watch the PR-size *distribution* alongside review latency and change failure rate, and tie PRs back to shipped features. Count *outcomes behind* PRs, not PRs.
- **Tickets closed → ticket-splitting / superficial closes.** Guard with reopen rate, escaped-defect / incident rate, and cycle time on meaningful tickets. A split ticket raises count but also raises reopen/escape signals.
- **Coverage / "quality" → slower dev and hollow tests.** 100% coverage is a vanity metric — achievable with zero assertions. Replace the *target* with **mutation score** and escaped-defect rate; keep coverage as a reflection metric, never a gate to maximize.
- **Documentation → over-documentation.** Volume of docs is an output; guard with doc *usefulness*: findability, onboarding time, support-deflection, staleness — all perceptual/outcome measures that filler cannot fake.
- **Token usage (AI) → forced token waste / AI slop.** Treat tokens as a *cost input*, never a productivity output. Guard with value/merged-PR per token, change failure rate on AI-authored code, review-and-rework time, and developer satisfaction. "Tokenmaxxing" is a textbook Cobra Effect.

**Bottom line for our metric design:** build the metric set as **balanced pairs**, not a leaderboard. For every tempting throughput number, ship its opposing guardrail in the same view; normalize to ratios; report at team/system level; triangulate telemetry with a periodic developer-experience survey; read trends and distributions; and designate up front which numbers are **targets** (few, outcome-shaped) versus **reflection/guardrail** metrics (most of them). That structure is what makes a metric set resistant — not immune — to Goodhart's Law.

---

## Sources

- Martin Fowler, *CannotMeasureProductivity* — https://martinfowler.com/bliki/CannotMeasureProductivity.html
- Kent Beck & Gergely Orosz, *Measuring developer productivity? A response to McKinsey* (Pragmatic Engineer, 2023) — https://newsletter.pragmaticengineer.com/p/measuring-developer-productivity and Part 2 — https://newsletter.pragmaticengineer.com/p/measuring-developer-productivity-part-2
- Kent Beck's newsletter version — https://newsletter.kentbeck.com/p/measuring-developer-productivity
- LeadDev, *What McKinsey got wrong about developer productivity* — https://leaddev.com/career-development/what-mckinsey-got-wrong-about-developer-productivity
- Tom DeMarco, *Software Engineering: An Idea Whose Time Has Come and Gone?* (IEEE Software, 2009) — summary: https://neverindoubtnet.blogspot.com/2009/07/tom-demarco-recants.html and https://lunatractor.com/blog/2011/03/24/tom-demarco-principles-of-control/
- W. Edwards Deming — the measurement myth — https://www.profound-deming.com/blog-1/the-myth-of-measurement and https://curiouscat.com/management/deming/managewhatyoucantmeasure
- Psych Safety, *Goodhart's Law, Campbell's Law, and the Cobra Effect* — https://psychsafety.com/goodharts-law-campbells-law-and-the-cobra-effect/
- Wikipedia, *Perverse incentive* (Cobra Effect) — https://en.wikipedia.org/wiki/Perverse_incentive
- Jellyfish, *Goodhart's Law in Software Engineering and how to avoid gaming your metrics* — https://jellyfish.co/blog/goodharts-law-in-software-engineering-and-how-to-avoid-gaming-your-metrics/
- Axify, *Goodhart's Law: The Hidden Risk in Software Engineering Metrics* — https://axify.io/blog/goodhart-law
- Keypup, *Goodhart's Law in Action: Why Your Dev Metrics Are Being Gamed* — https://www.keypup.io/blog/goodharts-law-in-action-why-your-dev-metrics-are-being-gamed-and-how-to-fix-it/
- Mixpanel, *Guardrail metrics: the complete guide* — https://mixpanel.com/blog/guardrail-metrics/
- Eppo, *What are Counter Metrics?* — https://www.geteppo.com/blog/counter-metrics
- Abi Noda & Laura Tacho, *Introducing the DX Core 4* — https://newsletter.getdx.com/p/introducing-the-dx-core-4
- DORA metrics guides (throughput vs stability, rework rate) — https://getdx.com/blog/dora-metrics/ and https://aws.amazon.com/blogs/devops/balance-deployment-speed-and-stability-with-dora-metrics
- SPACE framework overview via DX Core 4 unification — https://byteiota.com/dx-core-4-unifying-dora-space-and-devex-frameworks/
- ThinkingLabs, *The Fallacy of the 100% Code Coverage* — https://thinkinglabs.io/articles/2022/03/19/the-fallacy-of-the-100-code-coverage.html
- *Code Coverage vs Mutation Testing* — https://journal.optivem.com/p/code-coverage-vs-mutation-testing
- *Is Token Usage the New Lines of Code?* (DEV) — https://dev.to/sayed_ali_alkamel/is-token-usage-the-new-lines-of-code-how-to-measure-developer-productivity-in-the-ai-age-nd8
- Larridin, *What Is AI Slop?* — https://larridin.com/developer-productivity-hub/what-is-ai-slop-detect-prevent-low-quality-ai-code
- Augment Code, *AI Coding Cost Analysis: token spend per merged PR* — https://www.augmentcode.com/guides/ai-coding-cost-analysis-agent-token-spend
