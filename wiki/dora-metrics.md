# DORA Metrics

## Summary

The DORA metrics are a small set of software-delivery performance measures that came out of the **DevOps Research and Assessment** program (Nicole Forsgren, Jez Humble, Gene Kim; the *Accelerate* book and the annual *State of DevOps* reports, now run by Google Cloud). They are built around **four keys** — Deployment Frequency, Lead Time for Changes, Change Failure Rate, and Failed Deployment Recovery Time (historically "Time to Restore Service"/MTTR) — later joined by a **fifth signal, Reliability** (operational performance). DORA's central empirical claim is that software delivery has two dimensions, **throughput (speed)** and **stability (quality)**, and that high performers score well on *both* at once — they are not a trade-off. The metrics are deliberately outcome-oriented and team/system-level, not a measure of individual developer productivity. Two of the four keys (Deployment Frequency, Lead Time) are largely derivable from git + CI/CD data; the other two require deployment and incident data. Below are precise definitions, benchmark bands from recent (2023–2024) reports, computation formulas, data sources, gaming risks with their counter-signals, and how the framework applies to teams *without* mature CI/CD (e.g. app-store-released mobile/desktop apps).

---

## The Four Keys (plus the fifth)

DORA groups the keys into two axes. Note that in the **2024 "DORA Core" model** the grouping was refined: *Failed Deployment Recovery Time* is now treated as a **throughput** signal, and a new **Deployment Rework Rate** joins Change Failure Rate on the **instability** side.

| Axis | Metric | One-line definition |
|---|---|---|
| Throughput (speed) | **Deployment Frequency** | How often the team successfully releases to production. |
| Throughput (speed) | **Lead Time for Changes** | Time from code committed to that code running in production. |
| Throughput (speed) | **Failed Deployment Recovery Time** (MTTR) | Time to restore service after a failed deployment / degradation. |
| Stability (quality) | **Change Failure Rate** | Share of deployments that cause a failure needing remediation. |
| Stability (quality) | **Deployment Rework Rate** *(2024 addition)* | Share of deployments that are unplanned and triggered by a production incident. |
| Operational outcome | **Reliability** *(5th signal)* | Whether the running service meets user expectations (availability, latency, error budget / SLOs). |

> **On the "fifth metric":** DORA added *Reliability* in 2021 and initially called it "the fifth metric." It later clarified that reliability measures **operational performance**, not **software-delivery performance**, so it sits *alongside* the delivery metrics in the DORA Core model rather than as a peer fifth delivery key. It is typically measured against **Service Level Objectives (SLOs)** — availability, latency, error rate, correctness.

---

## Benchmark bands (2023–2024 State of DevOps)

The reports cluster teams into **Elite / High / Medium / Low** performance groups. Exact band edges have shifted between report years (and DORA increasingly cautions against treating them as a league table), but the recent published bands are approximately:

| Metric | Elite | High | Medium | Low |
|---|---|---|---|---|
| **Deployment Frequency** | On-demand (multiple per day) | Daily to weekly | Weekly to monthly | Monthly to once every ~6 months |
| **Lead Time for Changes** | Less than one day | One day to one week | One week to one month | One to six months |
| **Change Failure Rate** | 0–15% | 16–30% | 16–30% | 46–60% |
| **Failed Deployment Recovery Time (MTTR)** | Less than one hour | Less than one day | One day to one week | One week to one month |

**Caveats about the bands:**
- **Change Failure Rate does not cleanly separate the top clusters** — High and Medium share the same 16–30% band, so CFR is a weak discriminator in isolation. In the 2024 report the Medium cluster even posted a *lower* CFR (~10%) than the High cluster (~20%), breaking the usual pattern where all keys move together.
- Band definitions differ across report years; some years phrase Elite deployment frequency as "on-demand" rather than a fixed count. Always cite the report year.
- **2024 cluster shift:** the High-performing cluster shrank (~31% → ~22% of respondents) and the Low cluster grew (~17% → ~25%), i.e. performance polarized.

---

## How each metric is computed

For every metric: **what it measures · formula · data source · gaming risk · counter-signal.**

### 1. Deployment Frequency
- **Measures:** release cadence to production; a proxy for **batch size** and organizational agility (frequent deploys ⇒ small, contained changes ⇒ lower risk per deploy).
- **Formula:** `count(production deployments) / time period` (or the median time between deployments).
- **Data source:** CI/CD deployment events (Jenkins, GitLab CI, GitHub Actions, Argo/Spinnaker) or, as a proxy, **git** — merges/tags to a release branch, or release tags. Largely **git-/CI-derivable**.
- **Gaming risk:** split one change into many trivial deploys to inflate the count; deploy no-op or feature-flagged-off changes that add no user value.
- **Counter-signal:** track **batch size / PR size** and **Change Failure Rate** alongside it — inflated deploy counts of tiny no-ops don't move business or user-value signals; pair with a value metric (features shipped, tickets closed, user/transaction volume).

### 2. Lead Time for Changes
- **Measures:** end-to-end delivery pipeline speed — mostly *process* time (review queues, approvals, deploy gates), not typing speed.
- **Formula:** `deployment_timestamp − commit_timestamp` (commonly the **median** or 85th/95th percentile over a window). Often measured first-commit-of-change → live-in-prod.
- **Data source:** **git** commit/PR timestamps + CI/CD deploy timestamp. The commit half is **git-derivable**; the "deployed to prod" half needs a deployment record (CI/CD or a deploy log). Partially git-derivable.
- **Gaming risk:** measure from PR *merge* instead of first commit to hide review latency; cherry-pick which changes count; commit late to shrink the window.
- **Counter-signal:** measure from **first commit** (not merge) and report a **distribution/percentile**, not just the mean, so long-tail changes stay visible; cross-check against cycle-time / flow metrics.

### 3. Change Failure Rate (CFR)
- **Measures:** quality of what ships — the fraction of deployments that degrade service and need a hotfix, rollback, or patch.
- **Formula:** `failed_deployments / total_deployments` over a period (a percentage).
- **Data source:** CI/CD deploy events **joined to incident data** (PagerDuty, Opsgenie, incident tracker) or rollback/hotfix markers, plus monitoring (Grafana/alerting). **Not git-derivable** — requires deploy + incident linkage.
- **Gaming risk:** relabel incidents as "planned maintenance"; inflate the denominator with many trivial deploys so the percentage looks low; under-report failures.
- **Counter-signal:** cross-check against **Failed Deployment Recovery Time** and **Reliability/SLO** breaches — genuinely low CFR should coincide with few incidents and healthy error budgets, not just a big denominator.

### 4. Failed Deployment Recovery Time (MTTR)
- **Measures:** resilience — how fast the team restores service after a failed deployment.
- **Formula:** `recovery_timestamp − failure_detected_timestamp` (median across incidents).
- **Data source:** **incident management** systems (PagerDuty, Opsgenie, on-call/incident tickets) and monitoring. **Not git-derivable.**
- **Gaming risk:** close incidents early ("resolved") before real recovery; only log incidents that were fast to fix; classify slow recoveries as something other than deployment failures.
- **Counter-signal:** reconcile incident close times against **monitoring/SLO recovery** (when metrics actually returned to normal) and customer-impact duration; audit incident-open rates against CFR.

### 5. Reliability (operational performance / 5th signal)
- **Measures:** whether the live service meets user expectations — availability, latency, error rate, correctness.
- **Formula:** performance against **SLOs / error budgets** (e.g. % uptime, p99 latency, error ratio).
- **Data source:** monitoring/observability + SLO tooling. **Not git-derivable.**

---

## The two axes: throughput vs stability — and why they're not a trade-off

DORA frames delivery along two dimensions:

- **Throughput (speed):** Deployment Frequency + Lead Time (+ Recovery Time in the 2024 model).
- **Stability (quality):** Change Failure Rate (+ Deployment Rework Rate in the 2024 model).

The core, repeated research finding is that **speed and stability enable each other rather than compete**: elite performers score well on *all* keys simultaneously, and low performers do poorly on all of them. The mechanism is **small batch sizes** — frequent, small deployments are easier to review, test, and roll back, so they raise throughput *and* stability together. The common intuition that "moving faster means breaking more" is contradicted by the data; the report explicitly warns against treating the two as a dial to trade off. (The 2024 CFR anomaly, where Medium beat High on failure rate, is a notable exception that DORA flags rather than a reversal of the overall pattern.)

---

## Limitations, criticisms, and gaming risks

- **Goodhart's Law / league tables.** "When a measure becomes a target, it ceases to be a good measure." The 2023 report states outright that *creating league tables leads to unhealthy comparisons and counterproductive competition.* DORA is meant for a team to track its own trend, not to rank teams against each other.
- **Deployment Frequency is easily gamed** by splitting work into trivial deploys or shipping padded/no-op batches — inflating the count without delivering value.
- **They explain *that*, not *why*.** Metrics move for reasons unrelated to engineering quality — sickness, ramp-up at sprint start, holidays, team size changes. Raw numbers lack context.
- **Two of four need data many teams don't have cleanly.** CFR and MTTR depend on disciplined incident tracking and deploy↔incident linkage; without it they're noisy or absent.
- **Poor fit for non-continuous delivery.** Daily deploys suit web apps but not native mobile/desktop apps gated by app-store review, embedded/firmware, or regulated releases. Lead Time and CFR get distorted on long release cycles.
- **Not a productivity metric (see below).** Using DORA to evaluate individuals invites gaming and misuse.
- **Optimization past diminishing returns.** Teams can chase metric perfection long after it stops adding value, creating developer anxiety.

### AI's impact on delivery (2024 DORA report)
The 2024 report studied AI adoption and found a **counterintuitive split**: AI **raised individual productivity, flow, and job satisfaction** (≈75% of respondents reported productivity gains, ~76% used AI for tasks) **while degrading system-level delivery performance**. The modeled effect: for every **25% increase in AI adoption**, **throughput fell ~1.5%** and **delivery stability fell ~7.2%**. The leading hypothesis is that AI generates more code faster, which **increases batch/PR size** (reports cite large PR-size and review-time increases), and larger changesets are riskier and slower to review — reinforcing DORA's small-batch thesis. Takeaway: AI productivity gains don't automatically translate to delivery gains without small batches, strong testing, and fast review.

---

## DORA vs. individual/team productivity

DORA measures **system-level software-delivery outcomes**, not individual output. It deliberately avoids per-developer activity counts (commits, lines, PRs) because those are easy to game and don't reflect value delivered. Key distinctions:

- **Unit of analysis:** the delivery system / team, not a person. Deployment Frequency and CFR are properties of a pipeline, not an engineer.
- **Outcomes vs. activity:** DORA counts *deployed, working change* and *recovery*, not effort expended.
- **Complementary frameworks:** DORA is often paired with **SPACE** (Satisfaction, Performance, Activity, Communication, Efficiency) and **DevEx** to capture developer experience and productivity holistically. Using DORA alone to rate individuals is a well-documented misuse that triggers gaming.

---

## Applicability to teams without mature CI/CD (mobile/desktop, app-store cycles)

App-store-released apps break several DORA assumptions: you **can't deploy on demand** (review adds 24–48h on iOS, variable on Android), you **can't roll back** a released binary (recovery = detect → hotfix → resubmit → wait for review → wait for user update adoption), and **Lead Time to production is partly outside your control** (the store's review queue). Direct DORA scoring makes even excellent mobile teams look "Medium/Low." Recommended approximations:

- **Measure against an internal/beta release channel, not the public store.** Create an internal release (TestFlight, Play internal/beta track, enterprise/ad-hoc build) for **every change that passes CI + QA**, and compute **Deployment Frequency** and **Lead Time** against *that* boundary. This restores a controllable, high-cadence signal.
- **Deployment Frequency →** cadence of builds promoted to the internal/beta channel (or store submissions). Best mobile teams land around weekly public cadence, so judge against a mobile-appropriate baseline, not the web "multiple per day."
- **Lead Time →** first commit → build available on the internal/beta channel (exclude, or separately report, the store-review wait so process time isn't masked by an external queue).
- **Change Failure Rate →** share of releases needing an **expedited hotfix / emergency release**, backed by **crash-free-session rate** and **crash/ANR** monitoring (Crashlytics, Sentry) as the failure signal.
- **Recovery Time →** time from detecting a bad release (crash spike) to a fixed build reaching users — realistically report *both* "time to fixed build submitted" and "time to adoption," since store propagation + user-update lag dominate. Use **staged/phased rollouts** and **feature flags / remote config** to make "recovery" possible without a new binary (kill-switch a feature instead of resubmitting).
- **Reliability →** crash-free rate, ANR rate, and app-performance SLOs are the natural operational-performance signals for mobile/desktop.

---

## Relevance to our goal

For a stack-agnostic, **git-derivable** metrics effort, the DORA keys split cleanly:

| Metric | Git-derivable? | What's needed |
|---|---|---|
| **Deployment Frequency** | Partially — yes as a proxy | Release **tags** or merges to a release/main branch are readable straight from git history. True production-deploy events need CI/CD, but tags are a solid stack-agnostic proxy. |
| **Lead Time for Changes** | Partially — the commit half is fully git-derivable | `commit_timestamp` → `release_tag/merge_timestamp` is computable from git alone. Adding the real "live in prod" moment needs a deploy record. |
| **Change Failure Rate** | No | Needs deploy↔incident linkage, or a git convention (e.g. `hotfix`/`revert` commits or tags) as a rough proxy. |
| **Failed Deployment Recovery Time** | No | Needs incident/monitoring data (or, as a proxy, time between a release tag and its follow-up hotfix tag). |

**Two of the four keys are approximable from git history alone** (Deployment Frequency via tags/merges; Lead Time via commit→tag). The stability keys (CFR, MTTR) require external data, but **git-convention proxies** exist: count `revert`/`hotfix`-labeled releases for CFR, and measure tag→hotfix-tag intervals for recovery.

**For app-store-released mobile/desktop projects in scope:** anchor DORA on the **internal/beta release boundary** (or on **git release tags**) rather than public-store availability, so Deployment Frequency and Lead Time stay meaningful and controllable; approximate CFR and recovery from **hotfix/revert tags** plus **crash-free-rate** monitoring; and judge results against a **mobile-appropriate cadence baseline**, not the web "multiple deploys per day" elite band. This keeps the metrics stack-agnostic and mostly derivable from git while acknowledging the store-review ceiling that DORA's original model doesn't account for.

---

## Sources

- DORA — *Software delivery performance metrics (Four Keys)* guide: https://dora.dev/guides/dora-metrics-four-keys/
- DORA — *Accelerate State of DevOps Report 2024*: https://dora.dev/research/2024/dora-report/
- Google Cloud Blog — *Announcing the 2024 DORA report*: https://cloud.google.com/blog/products/devops-sre/announcing-the-2024-dora-report
- Google Cloud Blog — *Use the Four Keys to measure DevOps performance*: https://cloud.google.com/blog/products/devops-sre/using-the-four-keys-to-measure-your-devops-performance
- RedMonk — *DORA Report 2024: A Look at Throughput and Stability*: https://redmonk.com/rstephens/2024/11/26/dora2024/
- GetDX — *Highlights from the 2024 DORA State of DevOps Report*: https://getdx.com/blog/2024-dora-report/
- GetDX — *DORA metrics: the complete guide (AI era)*: https://getdx.com/blog/dora-metrics/
- CD Foundation — *The DORA 4 key metrics become 5*: https://cd.foundation/blog/2025/10/16/dora-5-metrics/
- Typo — *The Fifth DORA Metric: Reliability*: https://typoapp.io/blog/dora-metric-reliability
- CI/CD Watch — *DORA Metrics Benchmarks (Elite/High/Medium/Low)*: https://cicd.watch/blog/dora-metrics-benchmarks-2026
- Aviator — *Everything Wrong With DORA Metrics*: https://www.aviator.co/blog/everything-wrong-with-dora-metrics/
- TechTarget — *Google DORA: Software delivery caught up to AI coding tools*: https://www.techtarget.com/searchsoftwarequality/news/366631712/Google-DORA-Software-delivery-caught-up-to-AI-coding-tools
- Swarmia — *Should you track DORA metrics for mobile apps?*: https://www.swarmia.com/blog/dora-metrics-for-mobile-apps/
- Glovo Engineering — *How to apply DORA metrics for mobile development*: https://medium.com/glovo-engineering/how-to-apply-dora-metrics-for-mobile-development-ab14aa07a48b
- Digia — *Release Velocity Metrics: What DORA actually means for mobile teams*: https://www.digia.tech/post/release-velocity-metrics-what-dora-actually-means-for-mobile-teams/
- Runway — *Key DevOps metrics for mobile teams*: https://www.runway.team/blog/key-devops-metrics-how-to-measure-mobile-teams
