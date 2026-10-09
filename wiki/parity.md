# Parity with the prototype (M0)

**Status:** M0, started 2026-10-09. Defines how `pmx` is checked against the private Python prototype
(`collector/fleet.py` + `tools/*.py`) on real projects, and which differences are **intentional**.

The prototype's outputs on real projects are private (client data), so they never enter this repo. This page
holds only the **method** and the **deviation register**. The golden data and the harness run stay in the
private workspace. Public CI tests against synthetic fixture repos instead (M1).

---

## 1. Golden set

One folder per freeze, e.g. `golden/2026-10-09/` in the private workspace:

| File | Content |
|---|---|
| `manifest.json` | `as_of`; every repo's name, branch, **pinned SHA** and tip date; SHA-256 of the prototype scripts and of the externals list; tool versions (git, osv-scanner, …) |
| `configs/` | The prototype configs with `branch` replaced by the pinned SHA |
| `prototype_out/` | The prototype's raw output per project |
| `metrics.csv` | The **long-format** golden table (§1.1), the only thing the harness reads |
| `inputs/` | Frozen external inputs that change over time (§1.2) |

### 1.1 Long format

```
project,metric,period,value,n
project-a,commits,2025-03,212,
project-a,rework_pct,2025-03,18.42,5310
```

- `metric` uses the spec's metric ids (§3). `period` is `YYYY-MM`; snapshot metrics use the period of the
  snapshot. `n` is the denominator where the spec defines one.
- `pmx export --format long --monthly` must emit the same table, so the harness is a plain table diff and
  does not depend on either tool's JSON shape.

### 1.2 Inputs that drift and must be frozen

Pinning the git SHAs is not enough. These inputs change after the freeze:

| Input | Freeze by |
|---|---|
| OSV vulnerability database | Store the **raw `osv-scanner` JSON** per (repo, lockfile) at the pinned SHA. `pmx` replays it through its parser, so parity tests our counting, not the database. (The first freeze showed why: one project's open criticals went 45 → 79 in four weeks with no code change.) |
| Code-host PR/MR data | Store the raw API responses (GitLab: every page and per-MR notes, keyed by request path; GitHub: the `gh pr list --json` export, fetched month by month to avoid GraphQL timeouts). `pmx` replays them through a fixture code-host adapter |
| FTE values | Copy the FTE file into the golden set |
| Velocity scores | Not part of parity. They go through `pmx classifier eval` (plan §6.5) against the reference set (§5) |

---

## 2. Comparison rules

- **Run:** `pmx collect --as-of <manifest.as_of>` with each repo pinned to `rev = <sha>`, then export the
  long format (spec §1.1, reproducible runs).
- **Periods:** compare the prototype's months only (its window ends at a hard-coded month, which becomes
  `as_of`).
- **Null vs zero:** prototype `0` with a zero denominator equals `pmx` `null` (spec §1.1, "No data ≠ zero").
- **Rounding:** the prototype rounds before storing (percentages to 0.1 or 0.01; commit sizes truncated to
  integers). Compare after applying the **prototype's** rounding to the `pmx` value. Any other difference
  fails.
- **Snapshot SHAs:** a month-end snapshot is the last commit with committer date before the 1st of the next
  month, reachable from the pinned SHA (`git rev-list -1 --before=…`).
- **KLOC:** scc on an exported tree is not bit-stable; allow ±0.1 KLOC (and the ratios derived from it).
- **Percentiles:** the prototype uses linear interpolation (numpy default) and then truncates to an integer.
  The spec adopts linear interpolation; truncation is a prototype display artifact.
- **Each failure is either a bug or a new entry in §3.** Never widen a tolerance to make a run pass.

---

## 3. Deviation register

**Expect** = what the harness checks. *Exact* means identical after §2's rules. *Recompute* means the
harness recomputes the expected value from the golden constituents with the spec's formula.

| Metric (prototype series) | Spec | Expect | Why |
|---|---|---|---|
| `commits`, `added` | §2 | Exact | — |
| `commit_med`, `commit_p90` | §2 | Exact | Truncated to integer as in the prototype |
| `commits_per_dev_med` | §2 | Exact | — |
| `mix_*` (stack mix) | §1.5, §2 | Exact for single-role repos. May differ for `per-file` repos | Test-detection fix moves some files into `qa` |
| `rework_pct` | §3.1 | Exact | — |
| `s_rework` / Quality | §3.1, §3 | Recompute | Rework score is now two-sided (15–25% band) |
| `test_discipline_pct` | §3.2 | **Differs, usually upward** | Test-detection fix (Python/Go/.NET/Swift/Dart/Ruby conventions). Harness reports the delta per repo; a drop needs investigation |
| `doc_discipline_pct` | §3.3 | Near-exact | Doc rules unchanged, but the denominator (commits touching prod) shrinks when files become tests |
| `multi_stack_pct`, `breadth_index`, `techs_per_dev` | §5 | Exact **when `pmx` is queried for the prototype's window** | Prototype uses a trailing 90-day window ending at the first day of the next month (exclusive); the spec uses the selected range. The harness queries `pmx` with that 90-day range per month |
| `active_devs` | §2 | Exact with the same 90-day query | Prototype counts people in the multi-stack leaders list of that window |
| `ai_assist_commit_pct`, `ai_assist_line_pct` | §6 | Exact | Not emitted when `ai_attribution = false` (both sides) |
| `ai_compare` | §6 | Exact **if** the latest commit ≤ `as_of` | Prototype's 12-month window ends at the latest commit in any repo, even past its last month. The spec ends it at `as_of` |
| Security counts (critical/high/moderate/low, top packages) | §7.1 | Exact with the frozen OSV database | Prototype scans branch tips at run time; the golden run uses the pinned SHAs |
| Security score | §7.3 | Recompute | — |
| `pr_count`, `pr_cycle_median_h`, `review_wait_median_h`, `review_coverage_pct` | §8 | Exact for repos without external PR authors | Spec excludes externals in the PR layer; the prototype did not |
| `mentoring_pairs` | §8 | **Differs** | Expert rule unified across GitHub and GitLab |
| `fix_pct`, `cfr_pct`, `single_author_file_pct`, `new_stack_entries` | §9 | Not compared | Placeholder zeros in the prototype; dropped from the tool |
| Velocity (monthly points) | §4 | Not compared | Package date rule changed (last commit, not modal month) and classifier changed. Covered by `classifier eval` against the reference set (§5) |
| Duplication, complexity, hotspots, SAST, secrets | §3.4, §3.5, §7 | Exact at the same SHA and tool versions | Snapshot cadence differs (week-end + month-end vs month-end), so only month-end snapshots are compared |
| Weekly series | §1.1 | No prototype counterpart | Validated with synthetic fixtures (M1) |

---

## 4. Known prototype facts relied on above

- Commits come from `git log <rev> --no-merges --numstat --since=<range_start>` and are bucketed by author
  date (`%ad`); months after the hard-coded end are dropped.
- Rework walks `git log -w --reverse -p` from `range_start − 21 days`, keyed on `(file path, line text)`;
  renames are not followed. Bots are not excluded (codebase-wide, as in the spec).
- Bots and externals are excluded from every author-attributable series; author `claude` counts as a bot.
- Multi-stack threshold is 40 lines; normalized entropy divides by `ln(#counting roles)`.
- GitHub "expert" status for mentoring is computed over **the whole PR export**, so it changes as the
  export grows. That alone moved earlier months' mentoring pairs between freezes, which is one more reason
  the metric is in the *differs* column.
- `gh pr list --json files` returns at most 100 files per PR; the prototype's GitHub mentoring stack uses
  that truncated list.

## 5. Velocity reference set

The input for `pmx classifier eval` (plan §6.5). Built once per freeze, privately:
- Extract work packages at the pinned SHAs for the last 12 months before `as_of`.
- Draw a **stratified sample**: 80 packages per project, proportional over (month × ticketed/author-week)
  strata, fixed seed.
- Score with a **strong reference model** against the frozen rubric version (`six-axis@1`), in batches, with
  ids echoed verbatim and validated. Record model, rubric version and date.
- Candidate classifiers (smaller models, local models, Jev where the code may leave the machine) are scored on
  the same packages and compared per axis (quadratic-weighted κ) and on points.
- Client code goes only to providers its data policy allows. The reference model is itself a provider.

## 6. Freeze log

| Freeze | Result |
|---|---|
| 2026-10-09 | **Dependency scan for the third project:** its prototype config listed no lockfiles (an older result had been carried over to save a rescan), so the golden config adds its one lockfile. The carried-over figure was stale: a dependency-upgrade sweep two days before the freeze had removed every critical. Lesson for the tool: never carry a security snapshot over from an older run; auto-discover lockfiles (spec §10.1). |
| 2026-10-09 | **Snapshots:** duplication, complexity, SAST, secrets and hotspots frozen for 13 month-ends per project. Three projects scanned in parallel caused semgrep rule timeouts in two scans; re-run serially, every SAST count reproduced on a repeat run. An earlier unpinned run disagreed for three months of one project, which is why runs must be pinned. **Velocity reference:** 240 packages (80 per project, stratified by month × ticketed) scored by Claude Opus against `six-axis@1`. One `author~week` id occurred in two projects, hence project-scoped ids (spec §10.3). |
| 2026-10-09 | Re-running the prototype at pinned SHAs reproduced every git-derived series of the previous run (2026-09-13) exactly for all months before the last two. Security re-frozen from raw scanner output. PR/MR flow frozen for all three projects; every PR series matched the previous run except mentoring pairs. Snapshot metrics (duplication, complexity, SAST, secrets) and the Velocity golden set are still pending |
