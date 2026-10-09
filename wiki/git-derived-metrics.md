# Git-Derived Metrics

**Summary.** Git history is the one data source every project shares, so it is the most reliable
foundation for a stack-agnostic, always-available metric set. From commits, diffs, `git blame`, and
PR/MR metadata you can compute two broad families: **flow/speed** (PR cycle time and its
sub-phases, commit-to-merge lead time, review latency, PR size, rework iterations) and
**quality/stability/maintainability** (code churn and rework rate, revert/hotfix frequency,
bug-fix ratio, code age, hotspots = churn × complexity, temporal coupling, and knowledge
concentration / bus factor). None of these measure business value or whether the *right* thing was
built, and several are distorted by squash-merges, rebases, and — increasingly — AI-generated code
that inflates volume and churn. The robust play is to treat each git metric as a *signal*, always
pair it with a counter-signal, and prefer flow-distribution and stability metrics (which are hard to
game) over raw volume metrics (which are trivially gamed). This page defines each concrete metric,
gives the git commands/data to compute it, and states what it indicates, its gaming risk, and its
counter-signal.

---

## 1. Flow / Speed signals (git + PR/MR metadata)

The dominant framing is **PR cycle time decomposed into phases**. LinearB's "four golden metrics"
(their cycle-time breakdown) and the Google/DORA lead-time decomposition agree on the same four
phases. This maps directly onto DORA's **Lead Time for Changes** (first commit → production).

### 1.1 PR/MR Cycle Time (and its four sub-phases)

- **Definition.** Total elapsed time from the first commit on a branch to the change reaching
  production (or, more narrowly, to merge). LinearB splits it into four phases:
  1. **Coding time** — first commit → PR/MR opened.
  2. **Pickup time** (a.k.a. review-wait) — PR opened → first review activity begins.
  3. **Review time** — first review → merge.
  4. **Deploy time** — merge → released to production.
- **How to compute from git/PR data.**
  - Coding time: first-commit author timestamp (`git log --reverse --format=%at <branch>`, first
    entry) vs. PR `created_at` (GitHub/GitLab API).
  - Pickup time: PR `created_at` vs. timestamp of first review/first review comment
    (`reviews`/`notes` API).
  - Review time: first review vs. `merged_at`.
  - Deploy time: `merged_at` vs. deployment/release timestamp (release tag date `git log --tags`,
    or CI/deploy event). Deploy time is the one phase that often needs a non-git source.
- **What it indicates.** Where delivery flow stalls. Research consistently finds **pickup time is
  40–60% of total cycle time** — i.e., the bottleneck is usually *waiting for a reviewer*, not
  coding or reviewing.
- **Gaming risk.** Medium. Teams can "start the clock late" (code on local branches for days before
  opening a PR, hiding coding time), rubber-stamp reviews to cut review time, or split work to make
  each PR look fast.
- **Counter-signal.** Pair with **PR rework/iterations** and **change-failure / revert rate**: fast
  cycle time with rising reverts means speed bought at the cost of stability.

### 1.2 Commit-to-Merge Lead Time (DORA Lead Time for Changes)

- **Definition.** Time from first commit of a change to it landing in the main branch / production.
  DORA 2024 benchmarks: elite < 1 day, high 1 day–1 week, medium 1 week–1 month, low > 1 month.
- **How to compute.** For each merged PR: `merged_at` − timestamp of first commit. To handle
  squash/rebase you must recover the *original* first-commit time from the PR's commit list before
  it was squashed (see caveats §4).
- **What it indicates.** Overall delivery speed / batch size of change.
- **Gaming risk.** Medium — same "late clock start" trick; also splitting one change into many
  trivial PRs.
- **Counter-signal.** PR size distribution and revert rate.

### 1.3 Review Latency (Pickup Time, isolated)

- **Definition.** Time between PR creation and the first non-trivial reviewer comment or approval:
  `first_review_timestamp − pr_created_at`.
- **How to compute.** PR reviews/comments API; take earliest human review event per PR.
- **What it indicates.** Team responsiveness and WIP/handoff health; the single biggest lever on
  cycle time for most teams (PRs commonly wait 4+ days).
- **Gaming risk.** Low–medium (a quick "LGTM" resets the clock without real review).
- **Counter-signal.** Review depth (comments per PR, review time) and post-merge rework.

### 1.4 PR Size Distribution

- **Definition.** Lines changed (or files touched) per PR, reported as a distribution, not a mean —
  e.g., % of PRs under 200 lines. Review time scales super-linearly with size (a 1,000-line PR is
  far worse than ten 100-line PRs).
- **How to compute.** `git diff --numstat <base>..<head>` per PR, or PR API `additions`/`deletions`.
- **What it indicates.** Batch size / reviewability. Smaller PRs merge faster and review better.
- **Gaming risk.** Medium — trivially padded by counting generated/vendored files, lockfiles, or
  formatting churn as "lines changed."
- **Counter-signal.** Normalize by excluding generated/whitespace changes (GitClear-style Diff
  Delta), and pair with rework rate.

### 1.5 PR Rework / Iterations

- **Definition.** Number of review→revision cycles a PR goes through: commits pushed *after* the
  first review, or count of "changes requested" events, or force-pushes after review.
- **How to compute.** Compare commit timestamps to first-review timestamp; count commits after it.
  PR API: review states, `requested_changes` events, and post-review pushes.
- **What it indicates.** Requirements churn, unclear specs, or quality issues surfacing late.
- **Gaming risk.** Low (annoying to game; more iterations looks bad, fewer requires actually
  cleaner PRs).
- **Counter-signal.** Cycle time — suppressing iterations by merging early shows up as more reverts
  / post-merge fixes.

---

## 2. Quality / Stability / Rework signals

### 2.1 Code Churn / Rework Rate (short-lived code)

- **Definition.** Churn = lines changed shortly after being written. The industry convention
  (GitPrime/Pluralsight Flow): **code a developer rewrites or deletes within ~2–3 weeks (commonly
  21 days) of committing it is "rework/churn"; changes after 21 days are "refactoring."** Software.com
  and others use the three-way split **New work / Churn (rework) / Refactor** (help others / legacy).
  Rework rate = churned lines / total lines changed.
- **How to compute.** For each changed line in a commit, use `git blame`/`git log -L` or line-level
  diff tracking to find when that line was last introduced; if the interval is under the threshold
  (e.g., 21 days), classify it as churn. Practically: walk `git log --numstat` chronologically and
  match deletions/rewrites back to recently-added lines. Tools: GitPrime/Pluralsight Flow, GitClear,
  code-maat `entity-churn`/`abs-churn`, custom scripts.
- **What it indicates.** Instability. High short-lived churn is a defect/uncertainty proxy —
  thrashing on the same lines. Benchmark: "healthy" churn is often cited at **15–25%**; sustained
  higher churn (outside greenfield/major refactor) signals unclear requirements or quality trouble.
- **Why there is a *floor*, not just a ceiling.** Rework rate is a two-sided guardrail, not a
  waste metric to drive to zero. A band's worth of short-term churn is the system working: writing
  code, exercising it, catching your own mistake, and fixing it within days — cheap self-correction
  versus letting the defect ship and become an incident. A rate **below** the band rarely means
  "perfect code"; it means the healthy loop isn't visible, and the likely causes are all bad:
  (a) code that isn't being scrutinized — rubber-stamp reviews that never produce changes, or code
  that merges and is never revisited; (b) a write-once/never-touch culture where problems accumulate
  silently instead of being corrected while cheap; (c) **gaming** — "efficiency" (1 − churn) is
  driven down by never touching old code (appending new files instead of editing existing ones,
  avoiding refactors), which yields a beautiful low number and a bloating codebase (see Gaming risk
  below); (d) **deferred debt** — because the 21-day window splits "rework" from "refactoring,"
  corrections made on day 25 don't count as churn at all, so a suspiciously low short-term rate can
  just mean debt is being pushed *past* the window. Treat a below-band reading as a prompt to check
  which of these is happening, not as a win.
- **Gaming risk.** Low-to-game-down but easy to distort: whitespace/formatting reformats and
  generated files inflate it; conversely "efficiency" (1 − churn) can be gamed by never touching
  old code.
- **Counter-signal.** Distinguish churn on *own recent* code (bad) from refactoring old code
  (often good). Pair with feature-completion / DORA lead time so you don't punish healthy
  refactoring.

### 2.2 Revert Frequency & Hotfix/Emergency Commit Frequency

- **Definition.** Rate of reverts and emergency fixes. Two heuristics:
  - **Reverts:** commits matching `^Revert "` (git's default revert message) or `git log
    --grep='revert' -i`.
  - **Hotfixes/emergencies:** commit messages matching `hotfix`, `emergency`, `urgent`, or fixes
    committed directly to main/hotfix branches outside normal flow.
- **How to compute.** `git log --grep -iE 'revert|hotfix|rollback'` counts over a window; ratio to
  total commits. Revert rate is a cheap proxy for DORA's **Change Failure Rate** when you have no
  incident data.
- **What it indicates.** Stability of what ships. Rising reverts/hotfixes = shipping breakage.
- **Gaming risk.** Medium — people avoid the literal word "revert"/"hotfix" (fixing forward with a
  plain commit) to keep the count down.
- **Counter-signal.** Pair with bug-fix ratio and post-merge churn; a low revert count with high
  bug-fix churn means fixes are being disguised.
- **Implementation spec.** The concrete commit convention (native `git revert` + a `Fixes:`/`Hotfix:`
  trailer to mark fixes of *shipped* code), the broadened linkage-aware detection regex, and the rollout
  plan are drafted in [change-failure-proxy.md](change-failure-proxy.md) — this is Quality rollout step 1.

### 2.3 Bug-Fix Ratio (defect proxy from messages / linked issues)

- **Definition.** Share of commits (or PRs, or changed lines) that are bug fixes rather than new
  work. Proxy for defect load.
- **How to compute.** Where an issue tracker links commits, count commits referencing bug-type
  issues. Where it doesn't, use the standard commit-message heuristic: regex for `fix`, `bug`,
  `defect`, `issue #` (`git log --grep -iE '\bfix(e[ds])?\b|bug|defect'`). This is the front half of
  the **SZZ algorithm**, which then blames the fixed lines back to the **bug-introducing commit**
  (`git blame` on the lines a fix touched) to locate where defects originate.
- **What it indicates.** Defect density trend and (via SZZ) which commits/files introduce defects.
- **Gaming risk.** Medium-high — purely message-based; developers can relabel fixes as "chore" or
  "refactor," and **tangled commits** (a fix bundled with unrelated changes) plus **ghost commits**
  (fixes that don't touch the buggy file) make SZZ noisy.
- **Counter-signal.** Cross-check with revert rate and, where available, real incident/issue data;
  never treat message-derived defect rate as ground truth.

### 2.4 Code Age / Stability

- **Definition.** Time since each file/line was last modified. Old, stable code that rarely changes
  is low-risk; young code churns.
- **How to compute.** code-maat `age` analysis (grades modules by months since last change), or
  `git log -1 --format=%ad -- <file>` per file; line-level via `git blame` commit dates.
- **What it indicates.** Where the codebase has stabilized vs. where active/unstable work
  concentrates. Combined with churn it distinguishes "stable core" from "hotspot."
- **Gaming risk.** Low.
- **Counter-signal.** Age alone can mean "healthy and done" or "abandoned/rotting" — pair with
  hotspot and coupling analysis.

### 2.5 Change Coupling / Temporal Coupling

- **Definition.** Files that repeatedly change together in the same commit/PR, even without a static
  dependency. High temporal coupling = hidden architectural coupling and higher change cost.
- **How to compute.** For every commit, record the set of files changed (`git log --name-only`);
  count co-change frequency for each file pair; coupling = shared revisions / revisions of each.
  Tools: **code-maat `coupling`/`soc`** (sum-of-coupling), **Hercules** (co-occurrence matrices via
  go-git), CodeScene.
- **What it indicates.** Architectural risk; edits to A silently require edits to B. Widespread
  coupling signals tangled design.
- **Gaming risk.** Very low (hard to fake without actually decoupling code).
- **Counter-signal.** Some coupling is legitimate (a file + its test). Filter obvious pairs.

---

## 3. Maintainability / Health signals

### 3.1 Bus Factor / Knowledge Concentration

- **Definition.** Number of developers who'd need to leave before a project/module is orphaned;
  equivalently, how concentrated file ownership is. Flag files owned >80% by a single author;
  healthy modules have a primary author at ~40–60% with 2–3 others sharing the rest.
- **How to compute.** From `git blame` (or `git log --numstat` per author): attribute lines/commits
  per author per file; compute each file's top-author share and count single-author files. code-maat
  `entity-ownership`, `main-dev`, `fragmentation`, and `authors` analyses do exactly this. Empirically
  the risk is at file/module level (~85–90% of files in large repos are single-author), not the whole
  repo, so **sort files by single-author concentration and review the top ~20** as your risk register.
- **What it indicates.** Turnover/knowledge risk; onboarding and review bottlenecks.
- **Gaming risk.** Low-medium — can be inflated by trivial "ownership-spreading" commits (whitespace
  edits to touch files you don't understand).
- **Counter-signal.** Weight by meaningful contribution (Diff Delta / lines survived), not raw commit
  touches; pair with review participation.

### 3.2 File Hotspots (Churn × Complexity)

- **Definition.** Adam Tornhill / CodeScene "behavioral code analysis": a **hotspot** is code that is
  both *complex* and *frequently changed* — the intersection is where refactoring pays off most.
  CodeScene's **Code Health** metric (10 = healthy → 1 = severe issues) is an automated complexity/
  quality score layered on top of hotspots.
- **How to compute.** Change frequency from git (`git log --format= --name-only | sort | uniq -c`,
  i.e., code-maat `revisions`); complexity from a static proxy (lines of code, or indentation-based
  complexity, or a real complexity metric). Hotspot = high revisions × high complexity. The
  git-derivable half (change frequency) is stack-agnostic; the complexity half needs a language-aware
  or proxy measure.
- **What it indicates.** Prioritized technical-debt targets — where quality problems actually cost
  the team, because those files are touched constantly.
- **Gaming risk.** Low (change frequency is a natural signal).
- **Counter-signal.** A hotspot may be a legitimately central file; combine with coupling and defect
  (bug-fix) density before acting.

### 3.3 Knowledge Map / Distribution

- **Definition.** CodeScene "knowledge maps": visualize which author dominates which part of the
  codebase; the social/organizational view of ownership.
- **How to compute.** Same `git blame`/`git log` per-author attribution as bus factor, projected onto
  the directory tree (code-maat `entity-ownership` + `entity-effort`).
- **What it indicates.** Team-scale knowledge silos, off-boarding risk, and mismatch between team
  boundaries and module boundaries (Conway's law).
- **Gaming risk.** Low.
- **Counter-signal.** Pair with change coupling (silos + coupling across silos = coordination pain).

---

## 4. Honest caveats — what git CANNOT tell you

- **No business value / correctness of intent.** Git shows *what changed and how fast*, never whether
  the change was worth building or solved the user's problem. All these metrics can look great while
  the team builds the wrong thing.
- **No real defect rate without incident data.** Bug-fix ratio, revert rate, and SZZ are *proxies*
  built on commit-message heuristics; **tangled commits** and **ghost commits** make them noisy, and
  relabeling defeats them. Real change-failure/defect rates need incident/issue data.
- **Squash-merge distortion.** Squash merge discards the original commits and creates one new commit,
  so coding-time and commit-to-merge lead time collapse, and `git blame` points at the squash commit
  instead of the real author/lines. To compute lead time correctly you must fetch the PR's original
  first-commit timestamp from the API *before* the squash.
- **Rebase distortion.** Rebase rewrites commit IDs (and can rewrite dates), breaking commit-hash
  identity and skewing time-ordered analyses; it preserves authorship but not original ordering/IDs.
- **Raw LOC / commit count are vanity.** Lines of code, commits per day, and "coding days" reward
  volume and are trivially gamed (verbose code, commit-splitting). Treat them as descriptive context,
  never as targets — this is the core Goodhart's-law trap.
- **Merge strategy & monorepo effects.** File-touch counts, coupling, and ownership shift with repo
  structure (monorepo vs. many repos), generated/vendored directories, and lockfiles — normalize
  these out or they dominate the numbers.

### AI-code-specific distortions of git metrics

AI assistants change the *shape* of git history, so several metrics mislead in new ways
(GitClear's longitudinal research on 150–200M+ lines, 2020–2024):

- **Churn inflation.** Code churn (lines revised within ~2 weeks) rose from ~4.5% (2023) to ~5.7%
  (2024); more AI code is written and quickly rewritten. Rework-rate baselines set pre-AI now
  under-flag instability.
- **Refactoring collapse, duplication surge.** Refactoring ("moved" lines) dropped ~40%, while
  copy/pasted (cloned) lines rose past moved lines for the first time; GitClear saw an ~8× jump in
  duplicated 5+-line blocks. Cloned code correlates with **15–50% more defects**, so volume goes up
  while maintainability goes down — LOC/Diff-Delta throughput looks *better* as health gets *worse*.
- **Review can't keep up.** AI writes faster than humans review, inflating PR size and pickup/review
  time and pushing teams toward rubber-stamp reviews (which then hides quality loss).
- **Volume/velocity metrics become actively misleading.** Any metric rewarding lines, commits, or
  Diff-Delta volume is inflated by AI generation; **stability and rework metrics (churn, duplication,
  revert rate, post-merge fix rate) become the *more* trustworthy signals** precisely because they
  penalize the low-quality-high-volume pattern AI encourages.

---

## Quick reference table

| Git-derivable metric | Speed or Quality? | Gaming risk | Pairs-with (counter-signal) |
|---|---|---|---|
| PR cycle time (4 phases) | Speed | Medium | Revert rate; PR rework |
| Commit-to-merge lead time | Speed | Medium | PR size dist.; revert rate |
| Review latency (pickup time) | Speed | Low–Med | Review depth; post-merge rework |
| PR size distribution | Speed (quality-adjacent) | Medium | Rework rate; exclude generated lines |
| PR rework / iterations | Quality | Low | Cycle time; revert rate |
| Code churn / rework rate | Quality | Low–Med | DORA lead time; refactor vs. churn split |
| Revert frequency | Quality (stability) | Medium | Bug-fix ratio; post-merge churn |
| Hotfix/emergency frequency | Quality (stability) | Medium | Revert rate; incident data |
| Bug-fix ratio (SZZ) | Quality | Med–High | Revert rate; real issue data |
| Code age / stability | Quality | Low | Hotspots; coupling |
| Change/temporal coupling | Quality (architecture) | Very Low | Filter test+file pairs |
| Bus factor / ownership concentration | Maintainability | Low–Med | Weight by Diff Delta; review participation |
| Hotspots (churn × complexity) | Maintainability | Low | Coupling; bug-fix density |
| Knowledge map / distribution | Maintainability | Low | Change coupling |
| Raw LOC / commits/day / coding days | (Volume — vanity) | **Very High** | Never a target; context only |

---

## Tools & their signature metrics

- **GitClear** — *Diff Delta* (formerly *Line Impact*): an effort metric that discounts whitespace,
  churn, moved code, and generated files, weighting surviving change by what/how-durable/where
  (β·τ·σ). Also *Code Provenance* (time between writing code and changing it), and operation
  taxonomy (additions=features, moves=refactor, deletions=cleanup, duplicates=debt). Publishes the
  leading AI-code-quality longitudinal research. Free/commercial. https://www.gitclear.com/
- **CodeScene** (Adam Tornhill) — *Code Health* (10→1 automated quality score), *Hotspots*
  (complexity × change frequency), *Knowledge maps*, *Change/temporal coupling*. Grounded in
  behavioral code analysis (*Your Code as a Crime Scene*, *Software Design X-Rays*).
  https://codescene.com/
- **Pluralsight Flow (formerly GitPrime)** — *Coding Days*, *Commits per day*, *Impact*, *Efficiency*
  (= 1 − churn), *Code Churn/Rework* (own code rewritten within ~21 days). Note: several are
  volume/vanity metrics — use with care. https://www.pluralsight.com/product/flow
- **LinearB** — cycle time and its four phases (coding/pickup/review/deploy — the "golden metrics"),
  DORA metrics, plus GitStream for policy automation. https://linearb.io/
- **code-maat** (open source, Tornhill) — CLI over `git log --numstat`; analyses: `revisions`, `age`,
  `coupling`, `soc`, `authors`, `entity-ownership`, `entity-effort`, `entity-churn`, `abs-churn`,
  `author-churn`, `main-dev`, `fragmentation`, `communication`, `summary`. Requires
  `git log --all --numstat --date=short --pretty=format:'--%h--%ad--%aN' --no-renames`.
  https://github.com/adamtornhill/code-maat
- **Hercules** (src-d, Go/go-git) — runs a DAG of analyses over full history: line burndown, churn,
  developer/file co-occurrence (coupling) matrices, ownership over time. https://github.com/src-d/hercules
- **git-quick-stats** — lightweight shell tool for per-author commit/line stats, commits by
  hour/day/month, contributor summaries. Good zero-setup baseline. https://github.com/arzzen/git-quick-stats

---

## Relevance to our goal

For a stack-agnostic, gaming-resistant, git-only dashboard, favor **flow-distribution and
stability** metrics over volume metrics. Most robust and hardest to game (recommended shortlist):

1. **PR cycle time, decomposed (esp. pickup/review-wait time).** Hard to fake meaningfully, exposes
   the real bottleneck, directly actionable. (Guard the "late clock start" by also tracking coding
   time.)
2. **Rework rate / short-lived churn (own code rewritten within ~21 days).** Strong instability/
   quality proxy, penalizes exactly the low-quality-high-volume pattern AI encourages; near-impossible
   to game *upward* in your favor.
3. **Revert + hotfix frequency** as a git-only **change-failure proxy.** Cheap, stack-agnostic,
   stability-focused.
4. **Temporal/change coupling** and **hotspots (change frequency × complexity).** Architectural-health
   signals that are essentially impossible to game without genuinely improving the code.
5. **Bus factor / single-author-file concentration.** Robust maintainability/risk signal from
   `git blame`; weight by Diff Delta to resist ownership-padding.

Explicitly **demote to context-only** (never targets): LOC, commits/day, coding days, PR count — all
Very-High gaming risk and inflated by AI generation. Every headline number should ship with a
counter-signal from the table above (speed always paired with a stability metric), and every
message-derived metric (bug-fix ratio, reverts) should be labeled a *proxy*, not ground truth.

---

## Sources

- LinearB — Cycle Time & four phases: https://linearb.io/blog/cycle-time ,
  https://linearb.helpdocs.io/article/1s38stsugw-glossary-metrics ,
  https://linearb.io/blog/why-estimated-review-time-improves-pull-requests-and-reduces-cycle-time
- DORA — metrics history & lead-time decomposition: https://dora.dev/insights/dora-metrics-history/ ,
  Octopus 2024/25 DORA summary: https://octopus.com/devops/metrics/dora-metrics/ ,
  GitDailies DORA-for-GitHub (squash/lead-time handling): https://gitdailies.com/articles/dora-metrics/
- GitClear — Diff Delta / Line Impact / provenance: https://www.gitclear.com/blog/line_impact_is_now_diff_delta ,
  https://www.gitclear.com/diff_delta_factors ,
  AI code quality 2025 research: https://www.gitclear.com/ai_assistant_code_quality_2025_research ,
  2026 maintainability gap: https://www.gitclear.com/the_ai_code_quality_maintainability_gap
- Pluralsight Flow / GitPrime — churn/rework: https://www.pluralsight.com/resources/blog/software-development/code-churn ,
  New/Churn/Refactor (Software.com): https://docs.software.com/metrics/new-churn-and-refactor
- CodeScene / Adam Tornhill — Code Health, hotspots, coupling:
  https://codescene.com/blog/measure-code-health-of-your-codebase ,
  https://codescene.com/blog/change-coupling-visualize-the-cost-of-change ,
  https://www.adamtornhill.com/
- code-maat: https://github.com/adamtornhill/code-maat — Hercules: https://github.com/src-d/hercules —
  git-quick-stats: https://github.com/arzzen/git-quick-stats
- PR size / review latency: https://docs.software.com/metrics/pull-request-size ,
  https://www.minware.com/guide/metrics/review-latency ,
  PR cycle-time benchmarks: https://gitkraken.com/blog/healthy-pr-lifecycle-time-benchmarks-targets-2026
- Bug-fix heuristics / SZZ: https://link.springer.com/article/10.1007/s10664-024-10511-2 ,
  Evaluating SZZ on Linux kernel: https://arxiv.org/pdf/2308.05060
- Bus factor / ownership: https://repowise.dev/blog/guides/code-ownership-bus-factor-git ,
  "Assessing the Bus Factor of Git Repositories": https://www.researchgate.net/publication/272824568_Assessing_the_Bus_Factor_of_Git_Repositories
- AI code & technical debt: https://leaddev.com/technical-direction/how-ai-generated-code-accelerates-technical-debt ,
  AI-writes-faster-than-review study: https://arxiv.org/pdf/2607.01904
- Squash/rebase distortion: https://blog.dnsimple.com/2019/01/two-years-of-squash-merge/ ,
  https://docs.github.com/en/pull-requests/reference/pull-request-merges
