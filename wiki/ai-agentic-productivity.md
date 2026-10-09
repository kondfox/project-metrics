# Measuring the Impact of AI Coding Assistants & Agentic Workflows

**Summary.** The evidence on whether AI coding assistants improve software development is genuinely mixed and, crucially, *context-dependent*. Controlled lab and field experiments show large speedups on greenfield, well-scoped tasks (~55% faster to build an HTTP server; ~26% more tasks completed across ~5,000 devs), yet a 2025 METR randomized trial found *experienced* developers on *mature* repositories were **19% slower** with AI — while believing they were 20% faster. At the system level, two consecutive DORA reports found that higher AI adoption correlates with *worse* delivery stability (a modeled ~7.2% drop in stability per 25% adoption increase), and GitClear's analysis of 200M+ lines shows rising code churn, cloning, and collapsing refactoring. The lesson for a metrics program is sharp: **do not measure AI usage as a success target** (acceptance rate, % AI-authored code, tokens — all game trivially and none correlate with quality). Instead, hold a **balanced scorecard** (speed *and* quality/stability/rework) constant, introduce the agentic-workflow change as an *intervention*, and use before/after comparison against control projects to see whether throughput rises **without** rework, revert, and change-failure guardrails degrading.

---

## Key empirical findings

### AI can speed up narrow, greenfield tasks (the optimistic evidence)
- **GitHub Copilot lab RCT (Spring 2022, 95 professional developers):** writing an HTTP server in JavaScript, the Copilot group finished **55.8% faster** (1h11m vs 2h41m; P=.0017, 95% CI on speedup [21%, 89%]). ([GitHub Blog](https://github.blog/news-insights/research/research-quantifying-github-copilots-impact-on-developer-productivity-and-happiness/), [arXiv 2302.06590](https://arxiv.org/pdf/2302.06590))
- **Cui et al. (2024) field experiments** across Microsoft, Accenture, and an electronics manufacturer, ~5,000 developers, 2–8 months: **~26% increase in completed tasks** with Copilot. Gains were real but the authors stress **strong context-dependence**. ([arXiv 2302.06590](https://arxiv.org/pdf/2302.06590))

These are the results most often cited by vendors. Note what they share: *scoped tasks, often greenfield, measured at the individual level.*

### AI slowed experienced developers on mature code — METR 2025 (the counterintuitive result)
- **Randomized controlled trial**, 16 experienced open-source developers, **246 real tasks** on large mature repos (avg 22,000+ stars, 1M+ LOC), each task randomly allowing or disallowing early-2025 AI (mostly **Cursor Pro + Claude 3.5/3.7 Sonnet**).
- **Allowing AI increased completion time by 19%** — i.e., AI *slowed them down*.
- **The perception gap:** developers *forecast* a 24% speedup beforehand, and *still estimated* a 20% speedup afterward — the opposite of the measured result. ([METR blog](https://metr.org/blog/2025-07-10-early-2025-ai-experienced-os-dev-study/), [arXiv 2507.09089](https://arxiv.org/abs/2507.09089), [Simon Willison](https://simonwillison.net/2025/Jul/12/ai-open-source-productivity/))
- **Caveats the authors stress (do not over-generalize):** result is specific to *highly experienced* devs on *mature, high-quality* codebases with implicit standards (docs/tests/lint) and repos they know deeply. It is *not* a claim that AI slows most developers, does not cover other domains, does not rule out future improvement, and cannot exclude strong learning effects beyond several hundred hours of tool use (participants had only ~50h of Cursor each). ([METR blog](https://metr.org/blog/2025-07-10-early-2025-ai-experienced-os-dev-study/))

**Takeaway:** perceived productivity is an unreliable measure. This alone justifies measuring *outcomes*, not self-reported time saved.

### System-level delivery got *less stable* with AI — DORA 2024
- Modeled effect of a **25% increase in AI adoption**: delivery **throughput −1.5%**, delivery **stability −7.2%**, and time on valuable work **−2.6%**. ([RedMonk on DORA 2024](https://redmonk.com/rstephens/2024/11/26/dora2024/), [DORA 2024 report](https://dora.dev/research/2024/dora-report/))
- Simultaneously, **~75.9%** of respondents use AI and **~75%** report individual productivity gains — the **productivity paradox**: individuals feel faster while the *system* delivers less stably. ([getDX summary](https://getdx.com/blog/2024-dora-report/), [RedMonk](https://redmonk.com/rstephens/2024/11/26/dora2024/))
- **Proposed mechanism:** AI makes it easy to write *more* code, producing **larger, riskier changesets**; DORA's long-standing finding is that large batches raise failure risk. The 2025 follow-up frames a **"verification tax"** — reviewing plausible-but-unverified AI code costs as much scrutiny as writing it. ([RedMonk 2025](https://redmonk.com/rstephens/2025/12/18/dora2025/), [getDX](https://getdx.com/blog/2024-dora-report/))
- This was the **second consecutive year** DORA found AI correlating with worsened delivery performance — a persistent, not transient, signal.

### Code-quality drift under AI assistance — GitClear
Analysis of **200M+ changed lines** (2020–2024; contributors include Google/Microsoft/Meta):
- **Refactoring collapsed:** "moved" (refactored/reused) lines fell from **~25% of changed lines in 2021 to under 10% in 2024**.
- **Cloning rose:** copy/pasted lines went from **8.3% to 12.3%**; **2024 was the first year on record where within-commit copy/paste exceeded "moved" code** — a shift away from reuse toward duplication.
- **Short-term churn up:** code revised within two weeks of commit rose from **3.1% (2020) to 5.7% (2024)** — a proxy for premature/low-quality commits.
- GitClear links cloned blocks to more defects, and projects continued cloning growth (headline figure "**4x more code cloning**"). ([GitClear 2025 research](https://www.gitclear.com/ai_assistant_code_quality_2025_research), [GitClear 2023 report](https://www.gitclear.com/coding_on_copilot_data_shows_ais_downward_pressure_on_code_quality))

### Downstream cost signals from industry
- **PR revert rate** for AI-touched code runs materially higher than human PRs in vendor benchmarks (~**8% vs ~3%**). ([exceeds.ai summary](https://blog.exceeds.ai/code-quality-metrics-ai-roi/))
- A Network Perspective analysis found AI-assisted work produced apparent productivity ≈ **14% of engineering capacity**, but only ~**6% remained as net delivery improvement after review and rework**. ([Jellyfish/industry](https://jellyfish.co/blog/ai-impact-framework/))
- DX's own longitudinal study found AI drove only a **10–15% PR-throughput increase over more than a year** — and that **~28% of committed code is now AI-authored** while quality impact is "**varied and volatile**," with some orgs seeing change-failure rates swing by up to **2 percentage points**. ([getDX framework guide](https://getdx.com/blog/ai-measurement-framework-guide/))

---

## How to measure agentic-workflow impact

The empirical picture above means a single number ("we adopted AI, velocity is up") is nearly worthless. The defensible design is an **experiment on a balanced scorecard**.

### 1. Define a balanced scorecard (measure speed AND quality together)
Track a small paired set so a gain in one dimension can't hide a loss in another:

| Dimension | Guardrail / signal (git-derivable) |
|---|---|
| **Throughput / speed** | PR throughput, lead time for changes, cycle time |
| **Stability** | change failure rate, revert/hotfix rate, MTTR |
| **Rework / quality** | rework rate (% of code re-touched within 7–21 days of merge), short-term churn, duplicate/clone rate, refactor ("moved") share |
| **Review load** | review iterations per PR, time-in-review, PR size/batch |
| **Escaped defects** | post-merge incidents / defect escape rate |

Speed metrics are only "wins" if the **quality/stability guardrails hold or improve at the same time**. This mirrors DORA's throughput-*and*-stability pairing and DX Core 4's speed-quality balance.

### 2. Run it as an intervention (baseline → change → track)
1. **Baseline (start state):** capture the full scorecard for each project *before* the agentic-workflow change, over enough history to establish a trend (e.g., 8–12 weeks).
2. **Intervention:** introduce the specific agentic change (e.g., agent-authored PRs, a new review workflow) on selected projects.
3. **Track continuously:** recompute the scorecard weekly/monthly and watch *movement*, not absolute level.

### 3. Use control projects + difference-in-differences
Because so many things move at once (team, domain, seasonality), a bare before/after is confounded. Prefer:
- **Control projects** that did *not* adopt the agentic workflow, on comparable stacks/teams.
- **Difference-in-differences:** compare the *change* in the treated projects' scorecard against the *change* in controls over the same window. This isolates the workflow effect from company-wide trends — the same regression/longitudinal logic DX recommends for AI impact.

### 4. Read the result through the guardrails
The question is never "did we write more code / accept more suggestions." It is:

> **Did throughput rise *without* rework rate, revert/hotfix rate, change failure rate, review iterations, or defect escape getting worse?**

If speed rises and guardrails hold/improve → the agentic workflow is helping. If speed rises but rework/reverts/failures climb (the DORA + GitClear pattern) → you're shipping faster-to-fix-later, and the "win" is illusory.

---

## What NOT to measure as a target

These are **usage/utilization** metrics. They are fine as *context* ("is anyone even using the tool?") but become actively harmful the moment they are made a **goal** — classic Goodhart's law.

- **Suggestion acceptance rate.** Trivially gamed: when one team set a Copilot acceptance-rate goal, within ~6 weeks developers were accepting suggestions they'd previously have rejected, review cycles lengthened, and defect rate crept up. Acceptance says nothing about whether accepted code survives review or production. ([Goodhart/AI-metrics writeups](https://medium.com/@seba_hurtado/were-measuring-ai-productivity-wrong-and-goodhart-warned-us-8b0a068f379f), [RockB](https://baeseokjae.github.io/posts/ai-coding-accepted-code-quality-2026/))
- **% of code that is AI-generated.** Rewards volume of AI output, not value. GitClear's data shows more AI code coincided with *more cloning and churn*. DX explicitly finds ~28% AI-authored code with quality impact "varied and volatile" — the percentage predicts nothing on its own.
- **Lines of code / tokens consumed.** Pure volume; incentivizes quantity over quality and "malicious compliance." **DX's AI Measurement Framework deliberately excludes acceptance rate and lines of code** for exactly this reason. ([DX framework guide](https://getdx.com/blog/ai-measurement-framework-guide/), [DX research](https://getdx.com/research/measuring-ai-code-assistants-and-agents/))
- **Self-reported "time saved."** METR shows perception is *inverted* from reality (felt +20%, actually −19%). Use surveys for sentiment/adoption, never as the ROI number of record.

**The failure mode these produce is "AI slop":** high output, high acceptance, lots of AI code — and rising review burden, rework, and escaped defects downstream. Measure the **downstream cost** (review effort, rework time, defect rate, post-merge incidents), not the upstream activity.

### Where usage metrics *do* belong
Frameworks like **DX** (Utilization / Impact / Cost) and **Jellyfish AI Impact** keep usage as a *denominator for interpreting impact*, never as the success metric: utilization tells you whether an impact reading is even meaningful ("utilization is necessary but not sufficient"), while the actual verdict comes from impact metrics (throughput, change confidence, change-fail %, maintainability) and cost/ROI (net time gain minus AI spend). ([DX](https://getdx.com/blog/ai-measurement-framework-guide/), [Jellyfish](https://jellyfish.co/blog/ai-impact-framework/))

---

## Relevance to our goal

Our project's whole reason for building metrics is to **experiment with agentic engineering workflows and learn whether they help**. This literature maps directly onto that:

- **Our anti-gaming stance is validated.** The README already bans LOC, PR count, coverage %, token usage as single metrics. The AI literature adds the specific traps to keep out: **acceptance rate, % AI-authored code, and self-reported time saved**. None go on the dashboard as targets.
- **The start-state + weekly/monthly tracking design *is* the correct experimental design.** Our "measure at a start state, then track continuously to see forward/backward/stagnation" is exactly the **baseline → intervention → track** loop needed to detect agentic-workflow effects. Capture each project's balanced scorecard as its **start state before** turning on an agentic workflow.
- **Use our multi-project fleet as controls.** We have many projects on different stacks. Projects *not* yet on the agentic workflow are natural **controls** for a difference-in-differences read — comparing the treated projects' scorecard movement to controls' over the same period, which is far more trustworthy than a single project's before/after.
- **Pick guardrails that are git-derivable and stack-agnostic** (fits our git-first, stack-irrelevant constraint): **rework rate** (re-touch within 7–21 days), **revert/hotfix rate**, **change failure rate**, **review iteration count**, **PR/batch size**, **clone/duplication and refactor-share** trends. These are precisely the signals that moved *the wrong way* in DORA and GitClear, so they are the ones most likely to catch agentic workflows shipping speed-at-the-expense-of-stability.
- **Front-face number, honest construction.** A single dashboard number is fine *only if* it is a composite that cannot rise while guardrails fall — e.g., a throughput signal **gated** by stability/rework guardrails, so "faster" only counts when quality holds. That is the operational form of "watch whether speed moves up while quality/rework/stability hold or improve."

Bottom line for the experiment: **agentic workflows are worth adopting only if the balanced scorecard shows throughput rising while rework, reverts, change failure, and review load stay flat or improve — measured against control projects, not against our own optimism.**

---

## Sources

- GitHub / Copilot lab RCT (55.8% faster): https://github.blog/news-insights/research/research-quantifying-github-copilots-impact-on-developer-productivity-and-happiness/ and https://arxiv.org/pdf/2302.06590
- METR 2025, experienced-dev slowdown (19%): https://metr.org/blog/2025-07-10-early-2025-ai-experienced-os-dev-study/ · https://arxiv.org/abs/2507.09089 · https://simonwillison.net/2025/Jul/12/ai-open-source-productivity/
- DORA 2024 report: https://dora.dev/research/2024/dora-report/ · analysis https://redmonk.com/rstephens/2024/11/26/dora2024/ · https://getdx.com/blog/2024-dora-report/
- DORA 2025 follow-up ("verification tax"): https://redmonk.com/rstephens/2025/12/18/dora2025/
- GitClear AI code quality research (churn, cloning, refactoring decline): https://www.gitclear.com/ai_assistant_code_quality_2025_research · https://www.gitclear.com/coding_on_copilot_data_shows_ais_downward_pressure_on_code_quality
- DX AI Measurement Framework: https://getdx.com/blog/ai-measurement-framework-guide/ · https://getdx.com/research/measuring-ai-code-assistants-and-agents/ · https://getdx.com/blog/measure-ai-impact/
- Jellyfish AI Impact Framework: https://jellyfish.co/blog/ai-impact-framework/ · https://jellyfish.co/platform/jellyfish-ai-impact/
- Goodhart's law / acceptance-rate gaming / AI slop: https://medium.com/@seba_hurtado/were-measuring-ai-productivity-wrong-and-goodhart-warned-us-8b0a068f379f · https://baeseokjae.github.io/posts/ai-coding-accepted-code-quality-2026/ · https://blog.exceeds.ai/code-quality-metrics-ai-roi/
