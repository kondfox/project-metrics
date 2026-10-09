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
- `pmx export --format long --monthly` emits the same table (metric ids: spec §10.7). The harness
  (`pmx-parity`) builds it through the same library code, adding the prototype's windows where the
  register says so, and diffs it against `metrics.csv`.

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

- **Harness:** `PMX_GOLDEN_DIR=<golden folder> cargo run -p pmx-parity --release`. It converts each golden
  config with the `import-config` converter, checks every pinned `rev` against the manifest, checks the
  externals list against its manifest hash (`PMX_PROTOTYPE_TOOLS`, default `<golden>/../../tools`), runs
  `collect` in a temporary workspace without fetch or cache, and prints the report to the terminal only.
  Nothing from the golden set is written anywhere.
- **Two dialects** (plan D10). The **prototype dialect** swaps in the prototype's test rules and its way
  of reading git output (verbatim rename notation and quoted paths; diff lines starting with `--- `/`+++ `
  taken as headers). It isolates everything else, so **every M1 series must match exactly** there; this is
  the gate. The **spec dialect** is the real tool: only pure commit counts must still match, and the
  deviations below are reported as deltas for review.

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
| `commits`, `commits_per_dev_med`, `ai_assist_commit_pct` | §2, §6 | Exact (both dialects) | Commit counts don't depend on paths |
| `added`, `commit_med`, `commit_p90`, `ai_assist_line_pct`, `rework_pct`, stack mix, multi-stack | §1.6, §3.1 | Exact in the prototype dialect; **differs slightly** in the spec dialect | Git-reading fixes (spec §1.6, §3.1): renames resolve to the new path, non-ASCII paths are unquoted, diff content lines starting with `-- ` are lines. Files the prototype misread as "not source" now count. Commit sizes are truncated to integer as in the prototype |
| `mix_*` (stack mix) | §1.5, §2 | Exact for single-role repos. May differ for `per-file` repos | Test-detection fix moves some files into `qa` |
| `rework_pct` | §3.1 | Exact | — |
| `s_rework` / Quality | §3.1, §3 | Recompute | Rework score is now two-sided (15–25% band) |
| `test_discipline_pct` | §3.2 | **Differs, usually upward** | Test-detection fix (Python/Go/.NET/Swift/Dart/Ruby conventions). Harness reports the delta per repo; a drop needs investigation. The drops seen in M1a came from the rename fix (renamed source files now count as prod, which grows the denominator), not from the test rules |
| `doc_discipline_pct` | §3.3 | Near-exact | Doc rules unchanged, but the denominator (commits touching prod) shrinks when files become tests |
| `multi_stack_pct`, `breadth_index`, `techs_per_dev` | §5 | Exact **when `pmx` is queried for the prototype's window** | Prototype uses a trailing 90-day window ending at the first day of the next month (exclusive); the spec uses the selected range. The harness queries `pmx` with that 90-day range per month |
| `active_devs` | §2 | Exact with the same 90-day query | Prototype counts people in the multi-stack leaders list of that window |
| `ai_assist_commit_pct`, `ai_assist_line_pct` | §6 | Exact | Not emitted when `ai_attribution = false` (both sides) |
| `ai_compare` | §6 | Exact **if** the latest commit ≤ `as_of` | Prototype's 12-month window ends at the latest commit in any repo, even past its last month. The spec ends it at the latest commit on or before `as_of` |
| Security counts (critical/high/moderate/low, top packages) | §7.1 | Exact with the frozen OSV database | Prototype scans branch tips at run time; the golden run uses the pinned SHAs |
| Security score | §7.3 | Recompute | Recomputed from the golden counts and secrets gate; compared to one decimal (the prototype rounded) |
| `pr_count`, `pr_cycle_median_h`, `review_wait_median_h`, `review_coverage_pct` | §8 | Exact for repos without external PR authors | Spec excludes externals in the PR layer; the prototype did not |
| `mentoring_pairs` | §8 | **Differs** | Expert rule unified across GitHub and GitLab |
| `fix_pct`, `cfr_pct`, `single_author_file_pct`, `new_stack_entries` | §9 | Not compared | Placeholder zeros in the prototype; dropped from the tool |
| Velocity (monthly points) | §4 | Not compared | Package date rule changed (last commit, not modal month) and classifier changed. Covered by `classifier eval` against the reference set (§5) |
| Duplication, complexity, hotspots, SAST, secrets | §3.4, §3.5, §7 | Exact at the same SHA and tool versions | Snapshot cadence differs (week-end + month-end vs month-end), so only month-end snapshots are compared. Checked two ways: a **replay** of the golden set's recorded scans through pmx's summarize/pool code (fast, every run), and **live** tool runs at the golden SHAs (`--live-snapshots`, slow) |
| `code`, `kloc` (snapshot sizes) | §3.5, §7.4 | Exact | Two KLOCs: `code` = scc with the duplication excludes (complexity), `kloc` = whole-tree scc rounded per repo (SAST density, pmx id `sast_kloc`) |
| Month-end snapshot commits | §3.4 | **Differs in rare months** | The prototype's `--before=<1st>` used the run's time of day, so it took commits from the morning of the 1st, and its quality and security runs (made at different times) even disagreed for two repos in one month. pmx uses 00:00 UTC. The replay uses the commit the prototype's security run actually took |
| SAST and secrets counts | §7.2, §7.4 | Exact in the prototype dialect; **differ** in the spec dialect | The spec's security test path adds the §1.4 test rules (e.g. Go `_test.go`) to the prototype's folder rule |
| `sast_new`, `sast_fixed` first month | §7.4 | null = 0 | No previous snapshot: pmx writes null, the prototype 0 |
| Hotspots | §3.5 | Report only (not in `metrics.csv`) | pmx counts revisions by author date in the 365 days ending on `as_of`; the prototype used the committer date (and the run's time of day). Rebased commits can shift a file's revision count by one. Ties break by the most recently changed file, as in the prototype |
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

## 6. Freeze and run log

| Date | Result |
|---|---|
| 2026-10-09 | **M3 parity run** (snapshot metrics, all three projects). **Replay** of the golden scans through pmx's summarize/pool code: every series exact in the prototype dialect (duplication, complexity, code size, SAST high/medium/low/new/fixed/per-KLOC, secrets, suppressions, the 7 dependency counts and the recomputed Security score). **Live** tool runs at the golden SHAs (scc 4.1.0, jscpd 4.3.0, gitleaks 8.30.1, semgrep 1.179.0 with the prototype's rules, OSV replayed): exact for every month whose month-end commit matches the golden one; the 1–5 months per project where the prototype's time-of-day `--before` took a commit from the 1st are reported as deltas. **Hotspots:** top-15 identical in 12 of 13 repos; the rebase-heavy monorepo differs by a revision or two on a few files because pmx windows by author date and the prototype by committer date. Found and fixed on the way: series misaligned when snapshots start later than `range_start`, and hotspots measured at the tip instead of the as-of commit. |
| 2026-10-09 | **M1a parity run** against the 2026-10-09 golden set, all three projects, every M1 series (activity, commit size, commits per dev, stack mix, rework, tests/docs-with-code, multi-stack and active devs over the prototype's 90-day window, AI-assisted, AI vs human). **Prototype dialect: exact everywhere**, except the AI comparison of the one project whose tip is after `as_of` (expected, §3). Spec dialect: commit counts exact; every other delta traced to a listed deviation. Two findings became spec fixes: the prototype's rename and quoted-path reading (spec §1.6) and its rework diff parser (spec §3.1). Each project runs in under 10 s. |
| 2026-10-09 | **Dependency scan for the third project:** its prototype config listed no lockfiles (an older result had been carried over to save a rescan), so the golden config adds its one lockfile. The carried-over figure was stale: a dependency-upgrade sweep two days before the freeze had removed every critical. Lesson for the tool: never carry a security snapshot over from an older run; auto-discover lockfiles (spec §10.1). |
| 2026-10-09 | **Snapshots:** duplication, complexity, SAST, secrets and hotspots frozen for 13 month-ends per project. Three projects scanned in parallel caused semgrep rule timeouts in two scans; re-run serially, every SAST count reproduced on a repeat run. An earlier unpinned run disagreed for three months of one project, which is why runs must be pinned. **Velocity reference:** 240 packages (80 per project, stratified by month × ticketed) scored by Claude Opus against `six-axis@1`. One `author~week` id occurred in two projects, hence project-scoped ids (spec §10.3). |
| 2026-10-09 | Re-running the prototype at pinned SHAs reproduced every git-derived series of the previous run (2026-09-13) exactly for all months before the last two. Security re-frozen from raw scanner output. PR/MR flow frozen for all three projects; every PR series matched the previous run except mentoring pairs. Snapshot metrics (duplication, complexity, SAST, secrets) and the Velocity golden set are still pending |
