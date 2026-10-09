# Commercial Tools — What Engineering-Intelligence Vendors Actually Measure

## Summary

Surveying ~16 commercial "engineering intelligence" / software-delivery-analytics (SEI) vendors
reveals a striking convergence: almost all of them lead with **DORA's four keys** (deployment
frequency, change lead time, change failure rate, MTTR) and a **cycle-time decomposition** (coding →
pickup/review → deploy), pulling primarily from **git + pull requests + issue trackers (Jira)**, with
CI/CD as the source for deploy/incident data. Above that shared base, vendors differentiate along two
axes: *code-centric* quality analysis (CodeScene's Code Health, GitClear's Diff Delta/churn) versus
*flow/business-centric* analysis (Jellyfish allocations & R&D spend, Allstacks forecasting, Faros
AI-spend). The loudest philosophical debate is **individual-developer surveillance**: a clear camp
(Swarmia, Haystack, Allstacks, DX, Keypup) explicitly refuses leaderboards and per-developer LOC/commit
ranking as gameable and harmful, while the GitPrime → Pluralsight Flow lineage (and Waydev, Code Climate
Velocity) still computes per-developer "Impact/Efficiency" scores — the industry's cautionary tale. For a
small company measuring mostly from git, the trustworthy, hard-to-game, git-derivable subset is small and
well-agreed: **cycle time (four phases), PR size, review latency, rework/churn rate, deploy frequency,
and change failure rate** — used at team level, never as individual scorecards.

---

## Per-vendor notes

Each entry: signature metrics · data sources · philosophy on gaming/surveillance · notable benchmarks.

### LinearB (linearb.io)
- **Metrics:** **Cycle Time** decomposed into 4 phases — **Coding Time** (first commit → PR opened),
  **Pickup Time** (PR opened → first review), **Review Time** (review → merge), **Deploy Time** (merge →
  prod). Brands DORA as its **"4 Golden Metrics."** Heavy emphasis on **PR Size** ("the most significant
  driver of velocity," target <100 lines) and **Rework Rate** (code changed while <21 days old).
  **WorkerB** = Slack/Teams bot for personal PR alerts + auto reviewer routing / guardrails.
- **Sources:** git, PRs (GitHub/GitLab/Bitbucket), Jira, CI/CD. Delivery-data-centric; no surveys.
- **Philosophy:** Aggregated/team-level goal-setting and workflow automation rather than individual
  leaderboards, but anti-surveillance is *not* a central marketing pillar (contrast Swarmia).
- **Benchmarks (2026 report: 8.1M+ PRs, 4,813 teams):** Elite Cycle Time **<25h**; Coding **<54min**;
  Pickup **<1h**; Review **<3h**; PR Size **<100 lines**; Rework **<3%**; Deploy Freq **>1.2/service/day**.
  Finding: AI-generated PRs are **~2.5× larger** at p75.

### Swarmia (swarmia.com)
- **Metrics:** Four balanced areas — **Business impact** (investment tracking), **Flow** (DORA, PR cycle
  time, review time, batch size, WIP limits, CI insights), **Code quality** (CFR, bug resolution),
  **Team health** (developer-experience **surveys**). Signature: **Investment Balance** allocation (often
  using Dropbox's Balance Framework) + auto DORA + surveys correlated with DORA. Publishes "SPACE in
  practice" — roll out to a few motivated teams first, not org-wide at once.
- **Sources:** GitHub, GitLab, Jira, Linear, CI/CD, Slack/Teams, calendars, first-party surveys.
- **Philosophy (strongest anti-surveillance stance):** *"You won't find developer leaderboards or other
  harmful activity metrics in Swarmia."* Deliberately **excludes LOC, code churn/rework, and stack
  ranking** because they "lead to developers gaming the system." *"Just because something is easy to
  measure doesn't mean it should be tracked."* Metrics to ask better questions, not to judge individuals.
- **Benchmarks:** Uses standard DORA bands in-product; by design publishes **no proprietary numeric report**.

### Jellyfish (jellyfish.co)
- **Metrics:** Flagship is **Allocations** (patented model) — where engineering effort goes across teams,
  initiatives, and **investment categories**. Plus **DORA**, delivery (cycle/lead/PR review time),
  **DevFinOps** (software capitalization, R&D cost/tax-credit reporting), and a percentile **Benchmarks**
  module. Positioned to translate engineering into **business/finance language** without time tracking.
- **Sources:** Broadest — git, PRs, Jira/Azure/Linear, CI/CD, incidents, planning tools, **and calendars,
  finance, and HR systems**.
- **Philosophy:** Team/executive-facing, implicit rather than explicit anti-gaming; *does* surface
  individual-level allocation views, so less individual-averse than DX/Sleuth.
- **Benchmarks:** In-product percentile comparisons; public numbers are customer outcomes (22%
  predictability gain, 32 hrs/month saved on reporting).

### DX / GetDX (getdx.com)
- **Metrics (positioning):** "Engineering intelligence platform designed by researchers" (Abi Noda;
  Forsgren, Storey). **DX Core 4** (encapsulates DORA + SPACE + DevEx into Speed / Effectiveness / Quality
  / Impact) and the survey-derived **DXI** (Developer Experience Index, 14 drivers) — "the one number to
  increase ROI per engineer." Heavy 2026 push on **measuring AI-generated code** by commit/PR/team/agent.
  *(Framework detail lives on another wiki page.)*
- **Sources:** Blends **surveys/self-report** (DXI is survey-only) with system data (git/SCM, PRs, Jira);
  recommends system data "wherever possible."
- **Philosophy (explicit & strongest on principle):** *"Exceptional teams measure systems, not
  individuals."* Never measure diffs-per-engineer individually; **never tie targets/rewards to
  throughput**; always counterbalance speed with DXI. "Transparency over surveillance, signals rather
  than verdicts."
- **Benchmarks (500+ companies):** **Diffs per engineer ~3.0–5.5 PRs/week** — notably flat across all
  company sizes; **DXI 64–83**; change-fail **3–5.3%**; innovation ratio **55–65%**.
- **2026 status:** Reportedly **acquired by Atlassian** (see Compass below) — DX becomes Atlassian's
  DevEx/"DX Fabric" layer.

### Sleuth (sleuth.io)
- **Metrics:** DORA-first **deploy tracking** — the four DORA metrics *are* the product. Deploy Frequency
  auto-tracked per environment with batch-size breakdown; **feature-flag changes treated as first-class
  deploys** (LaunchDarkly). Markets itself as "lightweight, not intrusive."
- **Sources:** git, CI/CD, APM/observability, error trackers, issue trackers, feature flags. **Auto-detects
  deploys.** No surveys/HR/finance.
- **Philosophy:** Developer-facing "actionable feedback, not monitoring"; non-intrusive framing; no formal
  anti-gaming manifesto.
- **Benchmarks:** Cites DORA tiers (Elite: deploy on-demand, lead time <1 day, CFR ~5%, restore <1h).

### Haystack (usehaystack.io)
- **Metrics:** Four "North Star" metrics — **Change Lead Time** (p85), **Deployment Frequency**, **Change
  Failure Rate**, **Throughput** (PRs merged/member/week). Plus Cycle Time, Review Time (p85), PR Size,
  and Slack alerts for stuck PRs / burnout.
- **Sources:** git + Jira **metadata only** — *"will never touch, read, or access your source code."*
  Slack; deploys derived from git-provider APIs.
- **Philosophy (strongest technical anti-individual stance):** *"We actively don't look at individual
  engineers... evaluating individuals, stack ranking teams... [is] an ultimately futile effort."* Doesn't
  consider # commits / # PRs good productivity proxies. Team-level only.
- **Benchmarks (published tiers):** Cycle Time top-5% **1 day** / low **6.5 days**; Change Lead Time
  **1.5 → 10 days**; Throughput **>8 → <2.5** PRs/member/wk; PR Size fastest **<200 LOC**, avoid >600.

### Waydev (waydev.co)
- **Metrics:** Retains classic **per-contributor git metrics** (GitPrime lineage): **Impact**, **Code
  Churn** (<3-week rewrites), **Efficiency**, New Work, Legacy Refactor, Help Others, Commits/Active Day.
  Plus DORA, Agile/delivery (Cycle Time, Velocity, Sprint Commitment, **Engineering Overhead** ~20%
  target), and **software capitalization / R&D cost** (FTE reports). "130+ metrics."
- **Sources:** git (GitHub/GitLab/Bitbucket/Azure), Jira (also drives capitalization rules), CI/CD
  (Actions/Jenkins/CircleCI), calendars.
- **Philosophy (policy layered over individual-capable tooling):** *"We didn't build Waydev to be a
  tracking tool... a clarity tool. Measure the system before you measure people."* But the product still
  exposes granular per-developer metrics; the safeguard is positioning, not removal.
- **Benchmarks:** Standard DORA tiers; study of 1,971 teams / 847k branches scored at P50/P75/P90.

### Code Climate Velocity (codeclimate.com — Velocity, distinct from its Quality product)
- **Metrics:** Four DORA metrics using **real incident/deploy data** (a differentiator vs proxy-based
  tools). **Cycle Time** decomposed (Time to Open / First Review / Approve / Deploy). Rich glossary: PR
  Throughput, PR Size, Review Coverage/Speed, **Rework** (own code rewritten within 3 weeks), **Impact**
  (estimated change difficulty), Defect Rate, Innovation/Maintenance Rate.
- **Sources:** git (all four majors), Jira, **PagerDuty + Incident API**, deploy events via API, Slack.
- **Philosophy (context-focused, NOT explicitly anti-individual):** "No single metric tells the whole
  story"; pair metrics with conversations. But docs frame Rework/Impact as **individual-actionable**
  ("if a contributor has high Rework relative to peers, pair them with a senior") — has drawn "mistrust"
  criticism.
- **Benchmarks (12 mo to Oct 2023):** median Cycle Time **83h**; Time to First Review **15h**; PR Size
  **197 LOC**; PR Success **93%**; Weekly Coding Days **3.1**; Defect Rate **17%**; Rework **7%**.

### Pluralsight Flow — formerly GitPrime (pluralsight.com/product/flow → Appfire Flow)
- **Metrics:** Modern layer of DORA + Cycle/Lead Time + **Flow Efficiency** (value-adding / lead time),
  over the **legacy GitPrime engine**: per-developer **Impact**, **Efficiency** (productive vs churned
  code), **Code Churn**, Commits/day, **Active Days**, New Work / Legacy Refactor / Help Others.
- **Sources:** git (all majors + Assembla/Beanstalk), Jira/Azure, CI/CD & incident systems for DORA.
- **Philosophy (the cautionary tale):** GitPrime scored *individuals* and shipped leaderboards →
  documented gaming (cherry-picking easy tasks, padding review comments to hit a "recommended minimum"),
  collaboration collapse, fear/micromanagement. Bryan Finster: *"Under no circumstances should this be
  used."* Repositioned since 2019 toward team "Health Insights" and added misuse warnings ("don't use
  Impact as an absolute comparative metric") — but **the per-developer engine and leaderboard history
  remain**. Caveats are guidance, not guardrails.
- **Benchmarks:** Re-reports DORA thresholds; internal rule: investigate weekly Impact shifts >50 points.

### CodeScene (codescene.com)
- **Metrics:** **"Behavioral Code Analysis"** — flagship **Code Health™** (1–10 from 25+ code smells),
  **Hotspots** (git change-frequency × complexity), **Change/Temporal Coupling**, **Technical Debt
  prioritization** (weighted by hotspot activity), **Knowledge Maps / Bus Factor / off-boarding risk**,
  PR quality gates.
- **Sources:** **git** (primary behavioral source), PRs (all majors), Jira, IDE extensions, CI/CD.
- **Philosophy:** Knowledge distribution measured at **team level** ("usually even more valuable than
  individual metrics"); deliberately avoids "quality/maintainability" absolutes as gameable, emphasizes
  **trend over time**.
- **Benchmarks — the "Code Red" study** (peer-reviewed, 39 codebases / 30,737 files): low-quality code has
  **15× more defects**, takes **124% more dev time**, and has **9× longer max cycle times**. Up to
  **23–42% of dev time** wasted on low-quality code.

### GitClear (gitclear.com)
- **Metrics:** Flagship **Diff Delta™** — "lineage-aware" scoring of *durable, meaningful* change (cancels
  churn, discounts duplication/reformatting; human & AI lines on one yardstick). Commit-quality line
  classification (added/updated/moved/copy-pasted/churn), **code churn** (<2-week reverts). 65+ metrics
  incl. DORA; AI-ROI framings.
- **Sources:** git (full diffs), PRs, Jira, **AI-tool telemetry** (Claude Code, Copilot, Cursor, Codex),
  CI/CD via DORA.
- **Philosophy:** **LOC is a vanity metric** (~4% of LOC add value); good metrics resist gaming when
  "gaming the system serves the business' long-term interests." Retrospective team improvement, not
  real-time individual judgment.
- **Benchmarks — annual AI code-quality reports** (100s of millions of lines): 2023 "Coding on Copilot"
  (churn projected to **2×**); 2025 "4× Growth in Code Clones" (copy-paste 8.3%→12.3%, refactoring
  24.1%→9.5%); 2026 "Maintainability Gap" (623M changes — line-moves/refactoring **−70%** vs 2022,
  copy-paste **9.4%→15.7%**, devs now ~**5× more likely to copy-paste than refactor**). Common finding:
  **AI raises output while eroding maintainability.**

### Faros AI (faros.ai)
- **Metrics:** Full-SDLC graph. DORA + SPACE + DevEx + **DX Core 4**; velocity/quality/security/
  satisfaction/business categories. 2026 flagship is **Token Intelligence** — AI **cost per verified
  outcome / task**, spend attribution. "Only solution measuring cause-and-effect of AI assistants."
- **Sources:** 60–100+ connectors — git, CI/CD (Jenkins/CircleCI/Actions/Harness), Jira/Linear/Shortcut,
  PagerDuty, Datadog/Splunk/SonarQube, AI assistants (Copilot/Cursor/Claude Code/Amazon Q).
- **Philosophy:** **Team-level over individual** ("collective goal rather than individual wins");
  individual output metrics **undercount** value when engineers direct AI agents.
- **Benchmarks:** *AI Productivity Paradox* (10k devs): +21% tasks, +98% PRs merged, **PR review time
  +91%**, PR size +154%, **no company-level productivity correlation**. 2026 *Acceleration Whiplash* (22k
  devs): tasks/dev +34%, bugs/dev +54%, in instrumented subset **deployments −11.7%, lead time +480%**.
  Names **sub-8h PR cycle time** as elite. Token: 30% of spend produces no useful outcome.

### Allstacks (allstacks.com)
- **Metrics:** **Value Stream Intelligence** (initiatives → commits/PRs) with flagship **predictive
  delivery forecasting** (ML forecasts completion dates; "catch delivery risk weeks early"), automated
  risk alerts, 120+ metrics, DORA/SPACE/Flow, DevEx sentiment (via Slack), software cost capitalization.
- **Sources:** ~24 named integrations + webhooks — SCM (all majors), Jira/Linear/Azure Boards, CI/CD,
  Slack/Teams/Calendar/Confluence, AI assistants, Gong.
- **Philosophy (most explicit anti-surveillance quotes):** *"We spend a lot of time thinking about... ways
  to NOT be big brother. You'll never see a stack rank, grades, scores... in Allstacks."* On gaming:
  *"Gaming all of [multiple correlated metrics] elicits the right behaviors towards progress."*
- **Benchmarks:** No headline forecast-accuracy %; holds multiple US patents on delivery prediction.
  Example: high-comment-activity items take ~2.2× longer to complete.

### Uplevel (uplevelteam.com)
- **Metrics:** The differentiated one — **WAVE** framework (Ways of Working / Alignment / Velocity /
  Environment). Signature **Deep Work** = **2+ hrs meeting-free focus time**; **"Always On"** (evening/
  weekend work) as burnout signal; meeting load; effort allocation; Velocity Score; DORA/quality.
- **Sources:** Only vendor here reading **calendars and Slack/Teams** (meeting titles/durations, message
  timestamps) alongside git, PRs, Jira, CI/CD; ML/NLP classifies meetings & context-switches.
- **Philosophy:** Team/org-level aggregation as leading indicators of friction, not individual scorekeeping.
- **Benchmarks:** Avg focus **~2.24 h/day**, **~31.6 interruptions/day**, ~3 h/day in meetings; high
  performers get **3–4+ h/day** deep work; only **~20%** of eng time on new value; 23-min context-switch
  recovery.

### Keypup (keypup.io)
- **Metrics:** Customizable **DORA dashboards** (deploy freq, lead time split issues/PRs, CFR, MTTR) with
  an AI/NLP layer that generates dashboards and prescribes actions; "Engineering Insights Library."
- **Sources:** git (GitHub/GitLab/Bitbucket/Azure) + Jira/Trello/ClickUp, **metadata only — no source
  code**.
- **Philosophy (explicit Goodhart's Law stance):** Raw git metrics must never drive individual reviews
  (gaming → PR-splitting, story-point inflation, quality sacrifice, knowledge hoarding). Individual
  metrics should be role-adjusted, contextual, collaborative. *"Stop counting commits. Start recognizing
  impact."*
- **Benchmarks:** Cites elite DORA only (daily deploys, ~4h lead time); no proprietary table.

### Atlassian Compass / DevEx (atlassian.com/software/compass)
- **Metrics/capabilities:** Software **Component Catalog**, **Scorecards** (weighted engineering-standard
  criteria → % score), Component Health incl. **DORA**. Atlassian's DevEx story adds **DevEx 360**
  qualitative surveys (rooted in DORA/SPACE research).
- **Sources:** Ingests toolchain via integrations/app ecosystem (git, CI/CD, incident/monitoring).
- **Philosophy:** Standards-enforcement (scorecards) + survey-based DevEx rather than activity
  surveillance.
- **2026 status:** Reportedly acquired **DX**; a **"Next Chapter for Compass"** announcement (Apr 2026)
  transitions Compass's catalog/scorecards into DX / "DX Fabric" for an AI-native SDLC. Treat Compass as
  legacy/transitioning.
- **Benchmarks — Atlassian/DX "State of Developer Experience 2024"** (~2,100 respondents): **69% of
  developers lose 8+ hrs/week (~20% of time)** to inefficiencies; leaders and developers disagree on the
  cause (tech debt/docs vs understaffing).

### Adjacent DevProd / IDP-analytics tools (one-liners)
- **Cortex / OpsLevel / Port** — Internal Developer Portals: service catalog + maturity **scorecards**
  (OpsLevel's "Checks" can gate CI/CD); the commercial answer to open-source **Backstage**.
- **Multitudes** — team-health / wellbeing-focused collaboration analytics.
- **Oobeya** — value-delivery SEI (cycle/lead time, sprint accuracy, DORA).
- **Screenful / Plandek / Logilica** — lightweight dashboards + predictive delivery/predictability.
- **Minware** — data platform with a query language (minQL) + "time model."
- **Echoes (echoes.hq)** — connects engineering work to its **intent/business value** rather than activity.

---

## Synthesis: which metrics recur across vendors

Tally across the 16 named vendors above (how many *lead with / prominently feature* each metric).
"Type" flags whether it primarily indexes **Speed** (throughput/flow), **Quality/Stability**, or
**Alignment/Health**.

| Metric | Vendors featuring it | Primary data source | Type |
|---|---|---|---|
| **Deployment Frequency** (DORA) | ~14 (LinearB, Swarmia, Jellyfish, DX, Sleuth, Haystack, Waydev, Code Climate, Flow, Faros, Keypup, Allstacks, Compass, Uplevel) | CI/CD + git | Speed |
| **Change Lead Time / Cycle Time** | ~15 (nearly all) | git + PRs (+CI/CD) | Speed |
| **Change Failure Rate** (DORA) | ~13 | CI/CD + incidents | Quality/stability |
| **MTTR / Time to Restore** (DORA) | ~13 | Incidents + CI/CD | Quality/stability |
| **Cycle-time phase breakdown** (coding/pickup/review/deploy) | ~6 (LinearB, Code Climate, Flow, Haystack, Swarmia, Uplevel) | git + PRs | Speed |
| **PR Size** | ~7 (LinearB, Haystack, Code Climate, GitClear, Faros, Swarmia-batch, Flow) | PRs/git | Speed enabler / quality-adjacent |
| **Review latency** (pickup/time-to-first-review) | ~6 (LinearB, Code Climate, Haystack, Flow, Swarmia, Waydev) | PRs | Speed |
| **Rework / Code Churn rate** | ~7 (LinearB, Code Climate, Waydev, GitClear, Flow, CodeScene, Faros) — *Swarmia deliberately excludes* | git diffs | Quality |
| **Throughput** (PRs merged / engineer) | ~6 (Haystack, DX, Faros, Jellyfish, Code Climate, Waydev) | PRs | Speed/activity (contested) |
| **Investment / Effort Allocation** | ~7 (Jellyfish★, Swarmia, Uplevel, Waydev, Allstacks, Code Climate, DX) | git+Jira classification | Alignment |
| **Developer Experience surveys** | ~6 (Swarmia, DX★, Compass/DevEx360, Uplevel, Allstacks, Faros) | Surveys | Health |
| **Code Health / maintainability** | ~3 (CodeScene★, GitClear, Code Climate Quality) | git + static analysis | Quality |
| **R&D capitalization / cost** | ~4 (Jellyfish★, Waydev, Allstacks, Faros-tokens) | Jira + finance/HR | Business |
| **AI-code impact** (AI-generated %, cost/outcome) | ~6 and rising (GitClear★, Faros★, DX, LinearB, Waydev, Allstacks) | git + AI-tool telemetry | New frontier |
| **Deep work / focus & meeting load** | 1 (Uplevel★) | Calendars + Slack | Health (distinctive) |

★ = the vendor for whom this is the flagship/signature metric.

### Consensus metric set

The handful that appear across *almost all* vendors — the industry's de-facto trusted core:

1. **Deployment Frequency** (DORA)
2. **Change Lead Time / Cycle Time** — usually the single most emphasized metric
3. **Change Failure Rate** (DORA)
4. **MTTR / Time to Restore** (DORA)
5. **PR Size** — the most-agreed *leading* health indicator (smaller = better)
6. **Review latency + Cycle-time phase breakdown** — to localize the bottleneck
7. **Rework / Churn rate** — most-agreed code-quality proxy (with Swarmia the notable dissenter)

In short: **the four DORA keys + cycle time + PR size + rework** form the shared spine. Everything else
(allocations, DXI/surveys, code health, R&D spend, AI-cost) is differentiation on top.

### Where they DISAGREE

- **Individual vs team measurement.** Swarmia / Haystack / Allstacks / DX / Keypup **refuse** per-developer
  ranking and leaderboards as gameable and harmful; the GitPrime → **Pluralsight Flow** lineage, **Waydev**,
  and **Code Climate Velocity** still compute per-developer Impact/Efficiency/Rework. This is the sharpest
  split and the clearest lesson.
- **Is churn/rework a valid metric?** Most feature it; **Swarmia explicitly excludes it** (plus LOC) as
  gameable. GitClear rebuilds it as "lineage-aware" Diff Delta to *resist* gaming.
- **Surveys vs system data.** DX centers survey-derived DXI; Sleuth/Haystack use no surveys at all. Most
  agree you cannot capture *experience* from system data alone, but disagree how central surveys should be.
- **Do you need business/finance data?** Jellyfish/Waydev/Allstacks pull finance & HR for capitalization;
  pure delivery tools (Sleuth, Haystack) stay strictly in the engineering toolchain.
- **What "elite cycle time" means.** Numbers vary by an order of magnitude depending on whether it's
  PR-cycle or commit-to-prod: LinearB elite **<25h**, Faros elite **<8h PR cycle**, Code Climate "elite"
  **<1h**, Haystack **1 day**. Benchmarks are directional, not comparable across vendors.
- **Does AI help?** GitClear and Faros both publish large-N evidence that **AI raises raw output while
  eroding maintainability / downstream throughput and showing no company-level productivity gain** — a
  direct counter to vendor marketing that sells AI-velocity uplift.

---

## Relevance to our goal

Our constraints: **stack-agnostic, git-primary (optionally Jira), simple front-face numbers,
gaming-resistant, team-level, and able to tell whether agentic workflows help or hurt.**

**Feasible from git/PRs alone (compute ourselves — no vendor needed):**
- **Cycle Time + phases** (coding, pickup, review, deploy-ish) — pure git/PR timestamps. The single most
  valuable, universally-trusted signal.
- **PR Size** — trivial from diffs; the most-agreed leading health indicator; strongly gaming-resistant as
  a *"keep small"* target (unlike LOC-as-output).
- **Review latency** (time to first review / merge) — from PR metadata.
- **Rework / Churn rate** (code rewritten within ~2–3 weeks) — from git diff history. Useful as a
  *quality* counterweight, but note Swarmia's warning: keep it team-level and pair with context, or it
  becomes gameable/punitive. GitClear's lineage-aware approach is worth emulating conceptually.
- **Deployment Frequency & Change Failure Rate** — feasible *if* we can infer deploys (tags/releases/CI
  events) and failures (revert commits, hotfix patterns, incident labels). Partially git-derivable.

**Needs sources we may not have (defer or approximate):**
- **MTTR** — needs incident data; approximate via revert/hotfix timing only.
- **Investment/effort allocation** — needs reliable ticket labeling (Jira) or commit classification.
- **DXI / developer surveys** — cheap and high-signal; a lightweight periodic survey is worth adding even
  though it's not git-derived (DX's evidence that it counterbalances speed is compelling).
- **Code Health** — needs per-stack static analysis, so it violates our stack-agnostic constraint; use
  git-behavioral proxies (hotspots = change frequency × file churn) instead, à la CodeScene.

**Vendors worth *emulating* (borrow the model, build it ourselves):**
- **LinearB** — the cycle-time 4-phase decomposition and PR-size focus are directly reproducible from git
  and are the best-validated speed model.
- **Swarmia / Haystack** — copy the *philosophy*: team-level only, no leaderboards, no LOC, metrics as
  questions. This directly serves our Goodhart's-law constraint.
- **GitClear / CodeScene** — for the *agentic-workflow* question specifically: their churn/refactoring and
  code-health trend methods are the most relevant published approach to detecting whether AI output is
  durable. Their finding (AI ↑ output, ↓ maintainability, flat company productivity) is the hypothesis our
  own metrics should be designed to test.
- **DX** — borrow the "always counterbalance a speed/throughput metric with an experience metric, never set
  targets on throughput" discipline.

**Vendors worth *buying* (only if we outgrow DIY):**
- If we later need **cross-team benchmarking + Slack workflow automation** with low effort → **LinearB** or
  **Swarmia** (Swarmia if the anti-surveillance culture matters most; LinearB for the richest benchmarks).
- If **finance/R&D capitalization** ever becomes a requirement → **Jellyfish**.
- For a **small company, none are necessary initially** — the trusted consensus subset (cycle time, PR
  size, review latency, rework, deploy freq, CFR) is all git-derivable and buildable in-house, which also
  keeps us stack-agnostic and in full control of the gaming-resistance design.

**Avoid emulating:** per-developer **Impact / Efficiency / Active Days / commit counts** (GitPrime /
Pluralsight Flow / Waydev lineage) — the industry's documented cautionary tale for gaming and mistrust,
and squarely against our stated goals.

---

## Sources

**LinearB:** https://linearb.io/blog/cycle-time · https://linearb.helpdocs.io/article/d2v8kqzxzd-cycle-time ·
https://linearb.helpdocs.io/article/d2v8kqzxzd-metrics-community-benchmarks ·
https://linearb.io/resources/software-engineering-benchmarks-report ·
https://linearb.io/blog/dora-metrics · https://linearb.io/blog/workerb-developer-automation

**Swarmia:** https://www.swarmia.com/developer-productivity/ · https://www.swarmia.com/git-analytics/ ·
https://www.swarmia.com/dora-metrics/ · https://www.swarmia.com/space/ ·
https://help.swarmia.com/getting-started/configuration/investment-balance

**Jellyfish:** https://jellyfish.co/platform/engineering-management-platform/ ·
https://jellyfish.co/solutions/business-alignment/ · https://jellyfish.co/platform/devops-metrics/

**DX / GetDX:** https://getdx.com/ · https://getdx.com/dx-core-4/ · https://getdx.com/research/benchmarks/ ·
https://getdx.com/research/measuring-developer-productivity-with-the-dx-core-4/ ·
https://getdx.com/blog/pitfalls-of-developer-activity-metrics/

**Sleuth:** https://www.sleuth.io/metrics/ · https://www.sleuth.io/resources/dora-metrics-complete-guide/

**Haystack:** https://usehaystack.io/ · https://support.usehaystack.io/en/articles/5728063-benchmarks ·
https://support.usehaystack.io/en/articles/6609240-metrics-101 · https://news.ycombinator.com/item?id=26413311

**Waydev:** https://waydev.co/ · https://waydev.co/dora-metrics/ ·
https://waydev.co/measuring-developer-productivity-beyond-traditional-metrics/ ·
https://waydev.co/introducing-cost-capitalization/

**Code Climate Velocity:** https://codeclimate.com/blog/combine-dora-metrics ·
https://codeclimate.com/blog/roll-out-engineering-metrics · docs.velocity.codeclimate.com (cycle-time,
industry-benchmarks, rework, impact)

**Pluralsight Flow / GitPrime:** https://www.pluralsight.com/product/flow (→ appfire.com/flow) ·
https://getdx.com/blog/pitfalls-of-developer-activity-metrics/ · https://lawler.io/scrivings/on-gitprime/ ·
help.pluralsight.com (Impact, code-fundamentals)

**CodeScene:** https://codescene.com/product/code-health · https://codescene.com/product/behavioral-code-analysis ·
Code Red study https://arxiv.org/abs/2203.04374 · https://codescene.com/blog/measuring-the-business-impact-of-low-code-quality

**GitClear:** https://www.gitclear.com/ · https://www.gitclear.com/help/technical/diff_delta_calculation ·
https://www.gitclear.com/coding_on_copilot_data_shows_ais_downward_pressure_on_code_quality ·
https://www.gitclear.com/ai_assistant_code_quality_2025_research · https://www.gitclear.com/the_ai_code_quality_maintainability_gap ·
https://www.gitclear.com/popular_software_engineering_metrics_and_how_they_are_gamed

**Faros AI:** https://www.faros.ai/dora-metrics · https://www.faros.ai/ai-productivity-paradox ·
https://www.faros.ai/platform/token-intelligence · https://docs.faros.ai/docs/quickstart

**Allstacks:** https://www.allstacks.com/ · https://www.allstacks.com/features ·
https://www.allstacks.com/blog/the-people-who-dont-think-they-need-allstacks ·
https://www.allstacks.com/blog/accurately-predicting-software-delivery-part-1-of-4-series

**Uplevel:** https://uplevelteam.com/resources/engineering-kpis-wave ·
https://uplevelteam.com/blog/deep-work-why-we-measure-in-two-hour-minimum-time-blocks ·
https://uplevelteam.com/product/see

**Keypup:** https://www.keypup.io/solutions/dora-metrics-tool/ ·
https://www.keypup.io/blog/goodharts-law-in-action-why-your-dev-metrics-are-being-gamed-and-how-to-fix-it/

**Atlassian Compass / DevEx:** https://www.atlassian.com/software/compass ·
https://support.atlassian.com/compass/docs/what-are-scorecards/ ·
https://www.atlassian.com/blog/company-news/the-next-chapter-for-compass ·
https://www.atlassian.com/software/compass/resources/state-of-developer-2024

**Adjacent tools:** https://www.oobeya.io/blog/top-software-engineering-intelligence-tools-2025 ·
https://encore.dev/articles/platform-engineering-tools

*Research date: 2026-09-13. Caveats: several benchmark figures are vendor-published (self-selected data);
"elite cycle time" numbers are not comparable across vendors (PR-cycle vs commit-to-prod); the Atlassian↔DX
acquisition and Compass sunset are per Atlassian's own April-2026 announcement; a few Code Climate / Flow
figures came via search snippets where direct fetch returned 403/404.*
