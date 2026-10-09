# Forging the Velocity Metric — Design

**Goal:** fleet headline metric #2 ([fleet-vision.md](fleet-vision.md)) — a **productivity** read that
goes beyond commit- and line-counts by weighting *how hard* the delivered changes were, and that pairs
against Quality so speed can't be bought with slop.

## 1. The new signal — six-axis work-item complexity

Every work item (a ticket, or a logical work package of commits) is scored by **reading its diff** on six
independent 0–3 axes — **R1 layers touched, R2 data model, R3 contract, R4 algorithmic content, R5 blast
radius, R6 verification need** — summed (0–18) and mapped to Fibonacci **points**:

| sum | size | points |
|---|---|---|
| 0–2 | XS | 1 |
| 3–5 | S | 2 |
| 6–8 | M | 3 |
| 9–11 | L | 5 |
| 12–14 | XL | 8 |
| 15–18 | XXL | 13 |

Full rubric + hard rules: [`rubrics/six-axis@1.md`](../rubrics/six-axis@1.md). Why it's worth the trouble:
- **Measures hardness, not volume** — "diff size never raises a score"; a 40-line change to a core
  ordering rule outranks a 900-line CRUD endpoint.
- **Cross-project comparable by design** — the axes are stack-abstract, so points compare a Kotlin app to
  a .NET service honestly (unlike LOC).
- **Gaming-resistant** — needs an LLM judging diffs against fixed anchors; you can't inflate it by writing
  more. Fibonacci weighting encodes non-linearity (an XXL = 13 XS).

## 2. The equation (locked v1)

**Do NOT** compute `w₁·commits + w₂·lines + w₃·points` — the three overlap on "how much happened", so
summing them triple-counts volume and re-imports the gameable signals (LOC & commit counts are
*context, never a headline target* — [synthesis.md](synthesis.md) §8). Points is the signal that lets us
**retire** raw volume from the headline. So:

```
Velocity(project, window) = total_points / engineer_days      # points per FTE-day
```
Capacity denominator = **FTE-days** once the utilization integration lands (ties Velocity to the **FTE**
metric #1); interim = **active engineer-days** (distinct person-days with commits). Units: *story points of
delivered complexity per full-time engineer-day* — capacity-normalized, cross-project comparable, hard to
game.

Commits and lines are **not addends** — they become the interpretive **ratios** (drill-down):
- **points / commit** — is each unit of activity getting harder, or just more frequent?
- **points / KLOC added** — *complexity density*; **falling density while lines rise = volume-without-value /
  the AI-slop-velocity alarm** (the thing this system exists to catch).

Points sets the level; commits and lines explain *why* it moved.

## 3. Rules that keep it honest

- **Pair with Quality.** Velocity ↑ with Quality holding = real win; Velocity ↑ with Quality falling =
  faster slop. Same speed↔quality pairing as everywhere.
- **Project-level headline only.** The rubric emits per-developer points; those stay in the private lead
  drill-down (never a per-person score), like all individual data ([synthesis.md](synthesis.md) §7a.3).
- **Verification debt is a Quality signal, not Velocity.** The rubric's *points where R6≥2 but no test was
  written* is captured separately and feeds the tests/Quality picture.

## 4. Data source & tools — the one real cost

This is the fleet's **first non-git, non-deterministic input**: scoring needs an LLM reading every work
package's diff (cost, latency, judgment variance — mitigated by fixed anchors + the "take the lower one"
rule). Automatable as a batch pass, but a genuine step up from the pure-git collector.

- `tools/six_axis_score.py` — `extract` groups a window's commits into work packages (by ticket id, else
  by author-week) and dumps each package's diff (generated artifacts excluded) for scoring; `ingest` maps
  the LLM's R1–R6 back to points and aggregates (incl. verification-debt points).
- `tools/velocity_score.py` — computes `points / engineer-day` + the two ratios from the scored points and
  git counts.

## 5. Dry-run results (2026-09-14)

2-week pilot (**2026-09-01 → 09-15**). Commits grouped into work packages (ticket id, else author-week),
each package's diff LLM-scored against the rubric (81 packages total, 4 scorer passes). **Live on the
fleet** (`tools/patch_fleet.py`).

| Project | **Velocity** (pts/eng-day) | pts/commit | pts/KLOC | points | eng-days | verif-debt |
|---|---|---|---|---|---|---|
| **Project A** | 3.72 | **0.99** | 3.1 | 171 | 46 | 18 pts |
| **Project C** | 3.46 | 0.28 | 1.56 | 45 | 13 | 0 |
| **Project B** | 1.37 | 0.29 | 1.02 | 37 | 27 | 0 |

**What points reveals that commits/lines hide:**
- **Project A does far harder work per commit (0.99 vs ~0.28)** — its window was real backend/permission-
  model/photo-pipeline work (one XXL: app-wide retirement of the legacy role model). High points/KLOC (3.1)
  too — solving complexity compactly.
- **Project C's window was mostly XS** — docs, security investigations, CI tag-bumps, tiny UI tweaks
  (≈15 of 28 packages scored all-zero). High commit count, low delivered complexity — the exact illusion
  raw commit counts create.
- **Project B** lowest velocity here; a quiet fortnight of medium fixes.

**Caveats (why this is a pilot, not a verdict):**
- The **engineer-day denominator is rough** — distinct (author, author-date) pairs; rebased/cherry-picked
  author dates scatter it, so cross-project `pts/eng-day` is shakier than the within-project ratios. The
  denominator becomes solid when real **FTE-days** integrate. `pts/commit` and `pts/KLOC` are the more
  robust cross-project reads for now.
- **Bot leakage**: `GitLab CI` / `Gitlab Runner` / boilerplate-init packages slipped the bot filter and
  each floored to XS=1 pt (the rubric has no zero band). Minor here; tighten the bot/docs-only exclusion
  and consider a genuine-zero → 0-points rule before production.
- Diffs were capped at 24 KB/package for scoring; a few large packages were truncated.
- LLM scoring has judgment variance (mitigated by fixed anchors + take-the-lower rule); different runs
  will vary a little.

## 5b. FTE-normalized velocity (2026-09-14) — the denominator that matters

The author-day denominator misleads whenever a project's committers aren't all full-time. **Manual
per-project FTE** (`tools/fte.json`, a stopgap until the utilization platform) fixes it:
`Velocity = points / (FTE × business-days-in-window)`. Where FTE is unset it falls back to
active-engineer-days and is flagged `~ eng-day approx` on the fleet (not comparable to FTE-normalized rows).

FTE: **Project B = 1, Project C = 3, Project A = 4.5** (Project A is the owner's estimate). 2-week pilot (10 business days):

| Project | FTE | author-day velocity (before) | **FTE-normalized** |
|---|---|---|---|
| **Project A** | 4.5 (est.) | 3.72 | **3.8 pts/FTE-day** |
| **Project B** | 1 | 1.46 | **3.5 pts/FTE-day** |
| **Project C** | 3 | 3.46 | **1.5 pts/FTE-day** |

**The ranking flips** — the sharpest demonstration of "headcount ≠ capacity" and why FTE is metric #1:
- **Project C looked fastest but is lowest per-FTE** — bursty committing gave it a tiny author-day
  denominator; 3 real FTE doing mostly docs/investigation/XS work = low delivered complexity per head.
- **Project B looked slowest but is high** — one focused FTE (contributors sharing 1 FTE) delivering steadily,
  now ~level with Project A.
- **Project A & Project B land together (~3.5–3.8)** once capacity-normalized; per delivered-complexity-per-FTE they're
  the productive pair, Project C the outlier to look into. Only honest **because all three now have an FTE**.

## 5c. Monthly velocity timeline (2026-09-14)

Scored **228 work packages** across the Jul/Aug/Sep 2026 quarter (6 scorer agents), tagged each package
to its **modal commit month**, aggregated points per month, and computed
`velocity[m] = monthly_points[m] / (FTE × business-days[m])`. **Live as a timeline panel on all three
project dashboards** (`tools/monthly_velocity.py` + `tools/patch_velocity_panel.py`).

| Project (FTE) | 2026-07 | **2026-08** (full month) | 2026-09 (partial) |
|---|---|---|---|
| Project A (4.5) | 0.86 | **3.4** | 1.35 |
| Project B (1) | 2.17 | **3.33** | 0.59 |
| Project C (3) | 0.57 | **1.3** | 0.85 |

- **August validates the pilot** — the clean full month reproduces the 2-week FTE-normalized figures
  (Project A 3.4≈3.8, Project B 3.33≈3.5, Project C 1.3≈1.5) and preserves the ordering (Project A≈Project B ≫ Project C).
- **Caveats:** September is a **partial month** (data through ~the 13th) so it reads low — not a real drop;
  and the modal-month assignment plus window edges make the first/last months of a scored window less
  reliable than the middle. The **timeline shows only the scored months** (nulls before).
- **Pipeline gotcha handled:** LLM scorers again mis-transcribed accented names ("Müller"→"Muller",
  "Szűcs"→"Szücs"); a **fuzzy resolver** (`difflib`, cutoff 0.8) matched every score back to its package
  (228/228, 0 unmatched) so month attribution held. Match externals/months via resolved package ids, never
  raw scorer keys.

**1-year extension (2026-09-14): Project A + Project B only.** Backfilled 9 earlier months. Package counts revealed
the cost asymmetry: **Project A 8, Project B 76, Project C 564** (Project C peaks at 230 in May). Owner chose to
extend **Project A + Project B now** (~3 agents) and defer Project C's 564-package backfill. Results:
- **Project B** — a real ~13-month line: steady **~1–2 pts/FTE-day** through the year, ramping to **3.33**
  in Aug (2025-09 0.36 · Oct 1.6 · Jan 1.7 · May 2.0 · Jul 2.2 · **Aug 3.3** · Sep 0.6 partial).
- **Project A** — **near-zero Jan–Jun** (0.05–0.11): it was genuinely dormant (~0 commits) until it ramped
  in July. Honest given the git, but see the FTE caveat.

**FTE-historical caveat:** FTE is a single *current* value applied across all months. Project A's 4.5 FTE against
its dormant early-2026 months makes those velocities read near-zero — the git reality, but Project A likely
wasn't staffed at 4.5 then. Per-month historical FTE (from the utilization platform) will fix this; until
then, read a project's velocity trend against its own activity, and don't over-read months where FTE and
real staffing diverge. (Also: AI-authored `Claude~` packages are now dropped; the bot filter was extended.)

To extend Project C later, or refresh: widen the extraction window and score — the pipeline handles months
end-to-end (`six_axis_score.py` → `monthly_velocity.py` → `patch_velocity_panel.py` / in-place series update).

**Exec-summary Velocity card (2026-09-21):** kept as a **single-month YoY** — latest *full* month's
pts/FTE-day vs ~the same month a year earlier (`vnear` finds the nearest non-null within ±2 months), with a
faster/steady/slower pill. A 6-month-window version (to mirror the Speed card) was tried and then **reverted
at the owner's request** back to this full-month comparison. Note the known limitation: with only ~13 months
of scored velocity, the year-ago baseline falls back to the earliest scored month (Sep 2025 on Project B), so the
ratio compares the latest full month against that early low point (a large ×). **Live artifacts are ahead of
the local copies — build any republish from the fetched live version.**

## 6. Rollout / open

1. Pilot the scoring pass (this dry run); validate the rubric holds across stacks.
2. Decide scoring cadence (monthly) and whether to score all work or a representative sample per window
   (full coverage is required for a valid `points/engineer-day`).
3. Swap the interim **active-engineer-day** denominator for real **FTE-days** when utilization integrates.
4. Add Velocity to the fleet, read beside Quality (done in v1 — see fleet-vision).
