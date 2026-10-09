# Metrics Spec v1 — implementation reference

**Purpose.** The single, implementation-ready definition of every v1 metric, written so a portable tool
("run it in any project repo → web dashboard") can be built from it without reading the prototype code.
Summary for humans: [metrics-overview.md](metrics-overview.md). Rationale and research live in the linked
topic pages; **this page wins** where they disagree with it.

**Scope (decided 2026-10-08):** every metric currently live on the fleet view or a project dashboard is
v1. Designed-but-unimplemented signals are listed in §9 and are *not* v1.

Reference implementation (prototype, single-machine): `collector/fleet.py` + `tools/*.py`. Where this spec
deviates from the prototype it says so with **⟂ change vs prototype**.

---

## 1. Global conventions (apply to every metric)

### 1.1 Time model — weekly cadence, monthly roll-up (decided 2026-10-08)
- **Cadence is weekly.** The dashboard shows **weekly** stats by default and can switch to **monthly**;
  its time filter accepts any range. **⟂ change vs prototype** (monthly only).
- **Week** = ISO week, Monday 00:00 – Sunday 24:00, bucketed by the event's date in its own recorded
  timezone (commit author date, PR merge date). Label `2026-W41`.
- **Storage granularity is the day.** ISO weeks straddle month boundaries, so weekly buckets cannot be
  summed into exact calendar months. The tool therefore stores the **additive components per day**
  (numerators, denominators, per-person-per-role line counts, raw value lists for medians). Week, month
  and any custom range are all exact roll-ups of days. Days without activity are not stored.
- A metric for any bucket or range = recompute from its summed components. Never average weekly ratios
  into a monthly ratio (same rule as §1.2).
- **Flow metrics** (commits, rework, tests/docs-with-code, AI-assisted, multi-stack, velocity, PR flow)
  aggregate over the bucket/range.
- **Snapshot metrics** (duplication, complexity, dependency vulns, own-code SAST, secrets) are measured on
  the tree at every **week-end** (Sunday 24:00) **and every month-end**, deduplicated by commit SHA (an
  unchanged SHA reuses the previous result, so quiet weeks cost nothing). The weekly view uses
  week-end values, the monthly view month-end values; a range shows the value at its end plus the trend.
- **The current week and month are dynamic** (run date), flagged *partial*. **⟂ change vs prototype**
  (`fleet.py` hard-codes `'2026-09'`).
- **Reproducible runs (decided 2026-10-09, M0).** `--as-of <date>` replaces the run date everywhere
  (window end, partial-period flags, "last complete month"), and a repo may pin `rev = "<sha>"` instead of
  following `branch`. `meta` always records the measured tip SHA of every repo. A run with the same pinned
  SHAs, `--as-of`, config and tool versions must produce identical non-LLM output. This is what the parity
  harness ([parity.md](parity.md)) relies on. **⟂ new vs prototype.** Transcendental functions use the
  portable `libm` implementation, so the same input gives bit-identical numbers on every OS and in the
  dashboard's WASM engine (plan D16).
- **Commit selection is by author date, in code** (decided 2026-10-09, M1a): a commit belongs to the
  project if its author date is on or after `range_start` and on or before `as_of`. pmx does not use
  `git log --since`: that filters on the committer date, a bare date takes the *current time of day*
  (so results depend on when you run), and the walk stops at the first older commit. On the golden
  repos the two selections are identical. **⟂ change vs prototype.** For speed, git is asked only for
  commits whose committer date is at most 7 days before the bound (`--since-as-filter`, git ≥ 2.38);
  a committer date earlier than the author date by more than that does not occur in practice (none in
  the golden repos).
- **Default window** for headline numbers: the last **complete** month. Scores (Quality, Velocity,
  Security) are also computed per week for the trend lines.
- **Small samples.** A week often has few events (a 3-person team may merge 5 PRs). Every ratio carries
  its denominator, and a bucket with fewer than **10** denominator events (commits, PRs) is flagged
  *low-n* and drawn hollow or faded rather than hidden. Weekly sparklines may add a trailing
  4-week line to smooth noise. The threshold is a calibration knob.
- **No data ≠ zero.** A bucket with no denominator (no commits, no added lines, no PRs) yields `null`,
  never `0`. This matters for scores: e.g. 0% rework would score badly under the band rule (§3.1).
  **⟂ change vs prototype** (writes `0`).

### 1.2 Pooling
Across a project's repos, **sum numerators and denominators**, then divide. Never average per-repo ratios.

### 1.3 Author filtering (who counts)
- **Author-attributable metrics** (commits, lines, rework*, tests/docs-with-code, AI-assisted,
  multi-stack, velocity, PR flow) exclude **bots** and **external developers**.
- **Codebase-snapshot metrics** (duplication, complexity, dependency vulns, SAST, secrets) do **not**
  filter authors — they measure the code that exists.
- Bots: config list + pattern `\[bot\]|dependabot|renovate|github-action|semantic-release|gitlab[ -]?ci|gitlab[ -]?runner|jenkins|buildkite|^claude$|anthropic`.
- Bot rule detail: the pattern is matched case-insensitively against the trimmed author name and,
  separately, the email; the `bots` list matches either exactly (case-insensitive).
- Externals: per-project list of names and emails (`[people] externals`), case-insensitive exact match
  on the author name, the email, or the canonical person name (§10.1).
- Identity unification: an alias map `email → canonical person` merges a person's multiple identities
  before any per-person counting.
- *Rework is computed codebase-wide on line content (not attributed to authors) in the prototype — keep
  that for v1.

> Caveat: author `claude` is treated as a bot, so commits *authored* (not co-authored) by Claude are
> excluded everywhere, including AI-assisted. Commits with a `Co-Authored-By: Claude` trailer and a human
> author count normally.

### 1.4 File classification (stack-agnostic, path-based)
Evaluated on the lower-cased path, in this order:

| Class | Rule |
|---|---|
| **generated** (ignored everywhere) | filename in `package-lock.json, yarn.lock, pnpm-lock.yaml, gradle.lockfile, composer.lock`; or extension in images/fonts/binaries/certs (`.png .jpg .jpeg .gif .svg .ico .webp .pdf .woff .woff2 .ttf .eot .otf .map .snap .lock .class .jar .keystore .pem .crt .key .pfx .p12 .xlsx .zip .mp4 .mp3`); or path contains `/node_modules/ /dist/ /build/ /.gradle/ /generated/ /vendor/ /.next/ /coverage/ /__snapshots__/ /.idea/ /out/` |
| **doc** | extension `.md`/`.mdx`, or path under `docs/`, or filename starts with `readme` |
| **source** | not generated, not doc, extension in `.ts .tsx .js .jsx .mts .cts .cjs .mjs .kt .kts .java .swift .dart .py .go .rb .cs .php .rs .scss .css .less .vue .svelte .sql .tf .tfvars .sh .gradle .hbs .c .cpp .h .m .mm` |
| **test** | source file matching the test rules below |
| **prod** | source and not test |

**Test detection — ⟂ fixed vs prototype** (prototype missed Python/Go/.NET/Swift/Dart/Ruby conventions).
A path is a test if **any** holds:
- directory segment: `/test/ /tests/ /__tests__/ /spec/ /e2e/ /cypress/ /playwright/ /androidtest/ /integration_test/ /test_driver/ /uitests/` or a .NET test project dir matching `/[^/]*\.(unit|integration|e2e)?tests?/`
- filename: `*.test.*`, `*.spec.*`, `*.e2e-spec.*`, `*.cy.*`, `*.feature`, `test_*.py`, `*_test.py`, `conftest.py`, `*_test.go`, `*_test.dart`, `*_spec.rb`, `*Test.{java,kt,cs,swift}`, `*Tests.{java,kt,cs,swift}`, `*IT.java`

Matching details (decided 2026-10-09, M1a): directory rules match any path segment; filename rules match
the last segment only (the prototype matched `.test.`/`.spec.` anywhere in the path); the CamelCase rules
(`*Test`, `*Tests`, `*IT`) are **case-sensitive** on the original filename, so `Latest.kt` is not a test
(the prototype's lower-cased `endswith('test.kt')` made it one).

**Co-change touches (§3.2, §3.3):** a commit "touches a test file" if any changed, **non-generated** path
matches the test rules, whatever its extension (so `.feature` specs and fixtures under `tests/` count),
and "touches a doc file" if any changed path is class *doc*. Class *test* above (source files only) is
what decides prod vs test and the per-file `qa` role. **⟂ change vs prototype:** it ignored the generated
check, so snapshot-only changes (`__snapshots__/*.snap`) counted as tests.

Re-baseline tests-with-code history after this change (it will rise for repos using these conventions).

**Known v1 limits (accepted):** `.html` templates, `.json/.yaml/.xml` config are not "source";
`.tf`/`.sh` count as source.

### 1.5 Stack role (per line of work)
- **Multi-repo project:** role = the repo's configured role (`frontend · backend · mobile · qa · infra ·
  data · docs`).
- **Per-file repos** (`role = "per-file"` on a repo; **⟂ spec change 2026-10-09:** was project-level
  `monorepo: true`, so a project can now mix a monorepo with single-role repos): role per **file**, first
  match wins (accepted as-is, validated on Project B):
  test → `qa`; `.sql` or `/hex_queries/ /dataflow/ /sql/` or `bigquery` → `data`; `.tf/.tfvars` or
  `/infrastructure/ /environment/ /terraform/ /scripts/` → `infra`; `.tsx .jsx .css .scss .less .hbs .vue
  .svelte` or `/ui-react/ /app-ui-react/ /delivery/` or `web-ssr` → `frontend`; else `backend`.
  (Several path hints are Project B-specific; harmless elsewhere, but make the list config-overridable.)
- **Technology** per source file = extension map: `.ts/.mts/.cts`→TypeScript, `.tsx/.jsx`→React,
  `.js/.mjs/.cjs`→JavaScript, `.kt/.kts`→Kotlin, `.gradle`→Gradle, `.java`→Java, `.swift`→Swift,
  `.dart`→Dart, `.py`→Python, `.go`→Go, `.rb`→Ruby, `.cs`→C#, `.php`→PHP, `.rs`→Rust,
  `.css/.scss/.less`→CSS, `.vue`→Vue, `.svelte`→Svelte, `.sql`→SQL, `.tf/.tfvars`→Terraform, `.sh`→Shell,
  `.hbs`→Handlebars, `.c/.h`→C, `.cpp`→C++, `.m/.mm`→Objective-C.

### 1.6 Commit source
`git log <branch> --no-merges --numstat` on the configured branch (or pinned `rev`) per repo. Binary
numstat (`-`) = 0. Reading the output (decided 2026-10-09, M1a; **⟂ fix vs prototype**):
- **Renames** resolve to the new path (`src/{a.js => b.ts}` → `src/b.ts`). The prototype kept the
  notation as the path, so a rename that changed or wrapped the extension made the file "not source".
- **Paths are unquoted** (`core.quotePath=false`, C-style escapes decoded), so non-ASCII file names
  classify normally. The prototype saw them quoted, which hid their extension.
- **Git settings that change the output are pinned** on every call, so a user's git config can't move
  the numbers: `core.quotePath=false diff.renames=true diff.algorithm=myers diff.indentHeuristic=true
  diff.noprefix=false diff.mnemonicPrefix=false diff.relative=false log.showRoot=true
  log.showSignature=false`, plus `--no-ext-diff --no-textconv` for diffs.
- Author date = `%ad` with `--date=short` (the author's own timezone, §1.1).

### 1.7 Scores and bands
All 0–100 scores use fixed targets (never fleet-relative) and the same bands:
**A ≥ 90 · B ≥ 75 · C ≥ 55 · D ≥ 30 · E < 30**.

---

## 2. Activity & context metrics (project dashboard)

| Metric | Definition | Components to store per day |
|---|---|---|
| **Commits** | count of non-merge, non-excluded commits | `commits` |
| **Lines added** | Σ added lines in **source** files | `added` |
| **Commit size** | per commit `Σ(added+deleted)` over source files (commits with 0 skipped); show **median** and **p90** | list of sizes |
| **Commits per dev** | median over people of commits in the period | per-person commit counts |
| **Active devs** | distinct people with ≥1 added source line in a role listed in `breadth_roles` within the window (same population as §5) | per-person-role lines (§5) |
| **Stack mix** | % of added source lines by stack role | added lines per role |

Context only — never headline targets ([synthesis.md](synthesis.md) §8).

---

## 3. Quality (fleet headline) — [quality-index.md](quality-index.md) §4.0

```
Quality = geometric_mean(s_rework, s_tests, s_docs, s_dup)    over the constituents present
```
Each sub-score is floored at 1 before the geomean; missing constituents are **dropped and the geomean
renormalized** (never imputed). Always display the constituents and how many fed the score ("3/4").

### 3.1 Rework rate → `s_rework`
- **Measure:** for each repo, walk `git log -p -w --no-merges --reverse` from `range_start − 21 days`.
  Over **source** files only, ignoring trivial lines (stripped length < 4, or one of `{ } }); ) ); }, };
  [ ] (( )) return; else try {`):
  - every added line records `(file, line text) → date`;
  - a deleted line whose `(file, text)` was added **≤ 21 days** earlier counts as reworked (on the date
    of the deletion).
- Deletion date − add date ≤ 21 days, both author dates. With out-of-order author dates the difference
  can be negative, which counts (as in the prototype).
- Lines are keyed by the **post-image** path: renames are not followed, and the lines of a file deleted
  as a whole are not seen (`+++ /dev/null`). Known v1 limits, both as in the prototype.
- The diff is parsed by its headers (`diff --git` … `@@`), so a content line that itself starts with
  `-- ` or `++ ` (SQL comments, decrement operators) is a line. **⟂ fix vs prototype:** it took such
  lines for file headers.
- `rework% = Σ reworked / Σ added` (non-trivial source lines), pooled.
- **Score — ⟂ change vs prototype (decided 2026-10-08): distance from the 15–25% band, both sides.**
  ```
  s_rework = 100                       if 15 ≤ x ≤ 25
           = max(0, 100 − 4·(15 − x))  if x < 15
           = max(0, 100 − 4·(x − 25))  if x > 25
  ```
  (prototype was one-sided: ≤25% → 100.) Slope 4 pts/pp on both sides is a calibration knob. Expect
  low-rework projects to drop (e.g. 10% → 80; 0% → 40); `null` when no lines were added (§1.1).

### 3.2 Tests-with-code → `s_tests`
`% of commits touching ≥1 prod file that also touch ≥1 test file`, pooled. `s_tests = 100·min(1, x/80)`.
Blind spot: tests in a separate repo (e.g. an e2e repo) never co-occur with prod changes.

### 3.3 Docs-with-code → `s_docs`
Same, with doc files. `s_docs = 100·min(1, x/40)`.

### 3.4 Duplication % → `s_dup`
`jscpd@4 --min-lines 5 --min-tokens 50` on the tree at each week-end and month-end; `Σ duplicated lines / Σ scanned lines`,
pooled. Ignore `node_modules dist build .next out coverage vendor Pods .gradle .git generated
__generated__ .venv venv __pycache__ .turbo bin obj`, `*.min.js *.map *.lock *-lock.json *.snap *.svg`
and the repo's `.gitignore`. `s_dup = 100·clamp((15 − x)/12, 0, 1)` (3% → 100, 15% → 0).
Snapshot trees are materialized via throwaway `git worktree`/`git archive` — never touch the working copy.

### 3.5 Trend-only companions (not in the score)
- **Complexity / KLOC** — `scc`: Σ cyclomatic complexity ÷ code lines × 1000, per snapshot (week-end/month-end), pooled. Within-project trend only (stack-dependent).
- **Hotspots** — per file `revisions × complexity` (revisions = commits touching the file in the last 12 months; complexity from `scc --by-file`); top-N per repo, ranked within repo only. Same excludes as duplication.

---

## 4. Velocity (fleet headline) — [velocity-index.md](velocity-index.md)

1. **Work packages.** Non-merge, non-excluded commits in the window are grouped by the first ticket id in
   the subject (`\b[A-Z][A-Z0-9]{1,9}-\d+\b`), else by `author~ISO-week`. Package date = its last commit date; it is counted in that week and that month. (⟂ change vs prototype: modal commit month.)
2. **Diff extraction.** Each package's diff, excluding generated paths (§1.4 + `migrations/`, `Pods/`,
   `__generated__/`, `*.min.js`), capped at **24,000 bytes** (flag `truncated`).
3. **LLM scoring** against the six-axis rubric → R1–R6 (0–3 each); sum → points
   `0–2 XS 1 · 3–5 S 2 · 6–8 M 3 · 9–11 L 5 · 12–14 XL 8 · 15–18 XXL 13`. Contract: §8.3.
4. **Velocity = Σ points ÷ (FTE × business days in window)** — points per FTE-day. Business days = Mon–Fri
   (holidays not yet subtracted). If FTE is unknown: fallback `Σ points ÷ active engineer-days`
   (distinct person-days with a commit), shown with a `~` marker.
5. **Drill-down ratios:** `points / commit`, `points / KLOC added`. Also report **verification debt** =
   points where R6 ≥ 2 and the package touched no test file (feeds Quality reading, not Velocity).

Per-person points exist only in the private lead drill-down — never a headline.

---

## 5. Multi-stack developers (fleet headline) — [fullstack-metrics.md](fullstack-metrics.md)

**As-built rule is the v1 spec (decided 2026-10-08)** — the DOA/truck-factor design in fullstack-metrics.md
is future work.
- Per person, per stack role: Σ added **source** lines in the **selected time window** (only roles listed
  in `breadth_roles`). **⟂ change vs prototype:** window = the dashboard's time filter, not a fixed
  90-day trailing window. Store per-day `(person, role, lines)` and `(person, tech, lines)` so any
  window is a sum.
- A role **counts** for a person if lines ≥ **40**.
- **Multi-stack devs %** = people with ≥ 2 counting roles ÷ people with any lines in breadth roles.
- **Breadth index** = mean over people of normalized entropy of their line distribution over their
  counting roles (`H / ln(#roles present)`), × 100.
- **Techs per dev** = mean count of technologies with ≥ 40 lines.
- Drill-down "leaders": per person roles, techs, breadth, lines (lead-only view).

Mentoring pairs (PR-based) are in §7.

---

## 6. AI-assisted (fleet headline)

- A commit is AI-assisted if its message matches `co-authored-by: claude` (case-insensitive, anywhere).
- **AI-assisted commits %** = AI commits ÷ commits; **AI-assisted lines %** = added source lines in AI
  commits ÷ added source lines.
- **AI vs human comparison** (the 365 days up to the latest commit on or before `as_of`, by any author,
  bots included; both ends inclusive): for each group — commits,
  added lines, median commit size, tests-with-code %, docs-with-code % (definitions as §2/§3).
  Project-level only; never per developer.
- Config `ai_attribution: false` (trailers stripped by policy) → hide all AI series (show "not tracked",
  not 0).
- Known limit: Claude-trailer only; other tools (Copilot, Cursor) are invisible. Detection hardening is
  deferred ([fleet-vision.md](fleet-vision.md)).

---

## 7. Security (fleet headline) — [security-index.md](security-index.md), internal notes (not published)

### 7.1 Dependency vulnerabilities
- For each configured lockfile, take its content **at the branch tip** (`git show <branch>:<lockfile>`;
  week-end/month-end SHA for the trend) and run `osv-scanner scan source --lockfile <f> --format json`.
- Severity per advisory: `database_specific.severity` if present, else from the group's CVSS
  `max_severity`: ≥9 critical, ≥7 high, ≥4 moderate, >0 low, missing → unknown. `MEDIUM` ≡ moderate.
- Count **distinct (repo, advisory id)** at its highest severity; pool across repos.
- Drill-down: top 8 vulnerable packages (name, version, advisory count, max severity).

### 7.2 Live secrets (gate)
- `gitleaks dir <tree> --redact` on the tree at each snapshot/tip (history scan `gitleaks git` is
  drill-down only).
- **High confidence** = rule id starts with one of `private-key pkcs12 aws- gitlab- github-
  azure-ad-client-secret gcp-service-account slack- stripe- sendgrid- twilio- openai- anthropic-
  kubernetes-secret npm-access-token digitalocean- heroku- hashicorp- jfrog- doppler- postman-` **and**
  the file is not a test path or noise path (`docs/`, `prd/`, `*.md`, `README`, `*.example`, `*.sample`,
  `sample*`/`example*`). Everything else is low (trend only).
- A hit matching a triage entry (`repo + file [+ rule]`) does not count.
- Secret values are never stored.

### 7.3 Security score
```
sev   = 10·critical + 3·high + 1·moderate + 0.2·low
score = 100 · 0.5^(sev / 250)
if critical > 0 or live_high_secrets > 0:  score = min(score, 40)    # capped at band D
score = max(1, score)
```
Flagged `~` (point-in-time; no remediation-age data). Coverage is a lower bound where an ecosystem has no
scannable lockfile.

### 7.4 Own-code SAST (project dashboard, trend only — never ranked)
- `semgrep scan --metrics=off --json` with a **pinned local ruleset snapshot** (licence-compatible rules only, e.g. the LGPL Opengrep fork, not the Semgrep registry; ship the rules with the
  tool; record the snapshot date) on each snapshot tree.
- Findings in non-test paths, bucketed `ERROR/HIGH/CRITICAL → high`, `WARNING/MEDIUM → medium`,
  `INFO/LOW → low`. Also: findings per KLOC; **new / fixed** vs the previous snapshot of the same cadence keyed by the multiset of
  `(repo, rule, file)`; **suppression markers** count (`nosemgrep`, `# nosec`, `NOSONAR`,
  `gitleaks:allow`, security `eslint-disable`, `@SuppressWarnings("squid…`).
- Reuse the previous snapshot's result when a repo's SHA is unchanged.
- **Rule timeouts make a scan incomplete.** semgrep drops rules that time out, so counts would depend on
  machine load. Retry the scan once; if timeouts persist, store the snapshot as *incomplete* and show the
  value as "not measured" rather than a lower count. Run snapshot scans with bounded parallelism.
  (Found in M0.)

---

## 8. Code-host (PR/MR) flow — "Peer review" fleet column + project panels

Per merged PR/MR (bucketed by merge date), excluding bot authors:
- **PR count**; **cycle time** = median hours `created → merged`;
- **review wait** = median hours `created → first review` (reviewed PRs only; `null` if none);
- **Peer review %** (fleet column) = PRs with ≥ 1 review by a non-author, non-bot human ÷ merged PRs.
  *Review* = GitHub: a submitted review; GitLab: a non-system note by a non-author **or** an approval
  system note ("approved this merge request").
- **PR size** (GitHub only today) = median `additions + deletions`.
- **Mentoring pairs** = reviewed PRs whose author is **not** an expert in the PR's stack while ≥ 1 reviewer
  **is**. **⟂ unify vs prototype** (the GitHub and GitLab adapters define "expert" differently — lines vs MR
  count). v1 rule: PR stack = repo role, or for a `per-file` repo the dominant role by added lines;
  expert in stack S = ≥ 3 merged PRs in S **and** ≥ 25% of the top author's count in S.
- **⟂ gap:** externals are not excluded in the prototype PR layer — apply §1.3 (requires the
  login→person mapping in `[people]`, §10.1).

---

## 9. Not in v1 (designed or placeholder)

| Signal | Status |
|---|---|
| Change-failure proxy | Fully specified in [change-failure-proxy.md](change-failure-proxy.md) §3; not implemented (prototype emits zeros). Joins Quality once teams adopt the convention. |
| `fix_pct`, `single_author_file_pct`, `new_stack_entries` | Placeholder zero series in the prototype — **drop** from the tool until implemented. |
| Vulnerability remediation age / SLA gate | Needs per-advisory first-seen tracking (§7.3 becomes `gate_vuln`). |
| DOA-based expertise, truck factor, durable lines | [fullstack-metrics.md](fullstack-metrics.md) §2–3, future. |
| Cross-file SAST | Open decision — internal notes (not published). |
| DevEx survey, lead time to release, deploy/incident data | [synthesis.md](synthesis.md) §3, not built. |

---

## 10. Inputs & contracts

### 10.1 Project config (one file per project)
A TOML file, `pmx.toml`, in the project's workspace folder (**⟂ spec change 2026-10-09:** was JSON, and
replaces `collector/config_*.json` + `tools/externals.json` + `tools/fte.json` + `tools/secrets_triage.json`;
`pmx import-config` converts them). Full commented example: [`examples/pmx.toml`](../examples/pmx.toml);
setup commands: [tool-implementation-plan.md §3](tool-implementation-plan.md#3-developer-setup-for-a-multi-repo-project).

| Key | Meaning |
|---|---|
| `[project]` `name`, `range_start`, `breadth_roles`, `ai_attribution` | As before. `ai_attribution = false` where Co-Authored-By is stripped by policy |
| `[[repo]]` `path` **or** `url` | Local clone, or a URL pmx clones read-only into `.pmx/repos/` |
| `[[repo]]` `branch`, `rev` | Integration branch to measure; optional `rev` pins a SHA (§1.1 reproducible runs) |
| `[[repo]]` `role` | `frontend · backend · mobile · qa · infra · data · docs`, or `per-file` (§1.5) |
| `[[repo]]` `code_host_id`, `classifier`, `data_policy` | GitLab project id / GitHub `owner/repo`; per-repo classifier override and `local-only` guard (§10.3) |
| `role_rules` | Optional override of the per-file path hints (§1.5) |
| `[people]` | **⟂ spec change 2026-10-09:** one table replaces `identity` + `usernames`. `"Canonical Name" = [emails…, code-host logins…]`, plus `bots = […]` and `externals = […]` (§1.3) |
| `security_lockfiles` | Optional override; default = auto-discover every lockfile OSV-Scanner supports |
| `[code_host]` `type`, `base`, `token_env` | Tokens only by env-var name, never inline |
| `[fte]` | FTE provider (§10.2): `source = "static"` with `value` or dated `periods = [{from, to?, fte}]`, or `source = "file"` with `file` (the JSON below) |
| `[[secrets_triage]]` | `repo, file, rule?, verdict (false-positive · rotated · accepted), by, date` |
| `[classifiers.<name>]`, `[velocity] classifier` | LLM classifiers (§10.3) |

### 10.2 FTE provider (external source, stub in v1)
- Interface: `fte(project, date) → float | null` (FTE on that day; a week/month uses the mean over its
  business days).
- **v1 provider:** static file `{ "<project>": <fte> }` — one current value applied to every date
  (accepted; the historical-FTE distortion on dormant periods is a known caveat).
- **Future provider:** utilization platform. The file shape must also accept dated ranges, so the static
  file and the integration share one shape:
  `{ "<project>": [ {"from": "2026-08-01", "to": "2026-08-31", "fte": 4.5}, {"from": "2026-09-01", "fte": 4.0} ] }`
  (open-ended `to` = until further notice). Units: full-time equivalents. `null` → Velocity falls back
  to engineer-days (§4).

### 10.3 LLM classifier (Velocity)
**⟂ spec change 2026-10-09:** the free-form scorer prompt is replaced by a configurable classifier with one
schema for every provider. Design: [tool-implementation-plan.md §6](tool-implementation-plan.md#6-llm-classifier-d3).
- **Schema:** Jev's question/answer format. Request `{state, questions{id: {type: noul|choice|score,
  instructions, criteria}}}`; response `{answers{id: {type, score|choice|noul, probabilities, confidence}}}`
  plus `confidence_source` (`model · logprobs · sampled · none`). Providers: `jev`, `openai-compatible`,
  `anthropic`, `command`. `data_policy = "local-only"` refuses any non-local provider or fallback.
- **Rubric:** a versioned question set, `six-axis@2`, derived from [`rubrics/six-axis@1.md`](../rubrics/six-axis@1.md)
  (anchors unchanged). Counting and arithmetic stay in code: R1 = count of per-layer Nouls; R5 = file/module
  counts plus one Noul; R2/R3/R4/R6 = Score questions with the "take the lower anchor" rule applied in code.
  Never edit anchors after scoring; any change bumps the version.
- **State per package:** `id`, `subjects[]`, `repos[]`, changed files with class and role, `n_commits`,
  `added/deleted`, `truncated`, and the filtered diff (≤ 24 KB).
- **Output per package (unchanged contract):** `{ "id": <verbatim>, "R1".."R6": 0–3, "description" }`,
  derived from the answers in code. `description` = ticket id + first commit subject. The tool computes
  sum/size/points itself.
- **Package ids are project-scoped.** An `author~ISO-week` id (and even a ticket id) can repeat across
  projects, so anything that crosses projects (fleet roll-ups, `classifier eval` sets) keys packages on
  `(project, id)`. Found in the M0 reference sample.
- **Cache key:** `(project, package id, diff hash, rubric version, provider, model version)`. Pin versioned model
  ids. Only new or changed packages are scored.
- **Low confidence:** re-ask once with reordered levels; then the configured fallback (if the data policy
  allows), flagged in the output.
- No classifier configured → Velocity is `null` (not 0).

### 10.4 Code-host adapter (PR/MR)
- Interface: `merged_prs(repo, since) → [{ id, author_login, created_at, merged_at, additions?,
  deletions?, files?[{path, additions}], reviews[{login, submitted_at}] }]`.
- **GitHub:** REST/GraphQL with a token; reviews = submitted reviews. (Prototype read a manual `gh`
  export, `pr.json`.)
- **GitLab:** `/projects/:id/merge_requests?state=merged&created_after=` + per-MR `/notes`; reviews = non-author
  non-system notes and approval system notes. Token from `code_host.token_env`.
- Missing token/host → all §8 metrics `null` with "no code-host data".

### 10.5 External binaries
| Tool | Used for | Version | If missing |
|---|---|---|---|
| git | everything | ≥ 2.30 | fatal |
| scc | complexity/KLOC, hotspots, KLOC for SAST density | any | §3.5 null |
| jscpd | duplication | `jscpd@4` via npx | `s_dup` dropped from Quality |
| osv-scanner | dependency vulns | v2 (`scan source`) | Security score without deps (flag) |
| gitleaks | secrets | v8 (`dir`, `git`) | secrets gate off (flag) |
| semgrep | own-code SAST | OSS CLI + bundled rules snapshot | SAST panels null |

Every metric with a missing input renders as **"not measured"** and Quality/Security show which
constituents fed them.

### 10.6 Output data model (per project, one JSON)
`days{date → components}` (the stored truth, sparse); `snapshots{sha-dated week-end/month-end →
snapshot metrics}`; `weeks[]`, `months[]` with precomputed `series_weekly` / `series_monthly
{metric → [value|null]}` plus `n` (denominators) for the low-n flag; `detail{week|month → stack_mix,
leaders}`; `ai_compare`; `security{snapshot}`; `hotspots[]`; `velocity{packages[] scored, with date}`;
`meta{generated_at, as_of, repo tip SHAs, partial_week, partial_month, tool versions, rules snapshot,
rubric version, classifier per package, constituents present}`. Per-person drill-downs (leaders, per-person
points) go to a separate `private/leads.json`, never into the shared file. The per-person day components
in `days` (commits and lines per person, needed for multi-stack, active devs and commits per dev over any
range) use **pseudonymous ids** (`p1`, `p2`, … by sorted name); the id → name map is only in
`private/leads.json` (decided 2026-10-09, plan D9). The dashboard recomputes custom ranges from `days` with the same Rust code compiled to WASM (plan D15). The fleet view is an
aggregate of these per-project files.

### 10.7 Metric ids (series and long format)
The keys of `series_weekly` / `series_monthly` and the `metric` column of `pmx export --format long`
([parity.md](parity.md) §1.1). `n` is the denominator stored alongside (for the low-n flag, §1.1).
Values are unrounded; `null` means no data.

| Id | Definition | `n` |
|---|---|---|
| `commits`, `added` | §2 | — |
| `commit_med`, `commit_p90` | §2, linear interpolation | commits with size > 0 |
| `commits_per_dev_med` | §2 | people with ≥ 1 commit |
| `active_devs` | §2 (same population as §5) | — |
| `mix_<role>` | §2 stack mix, % of added source lines; only roles with lines anywhere in the project | added source lines |
| `rework_pct`, `s_rework` | §3.1 | non-trivial added lines (rework walk) |
| `test_discipline_pct`, `s_tests` | §3.2 | commits touching prod |
| `doc_discipline_pct`, `s_docs` | §3.3 | commits touching prod |
| `quality`, `quality_constituents` | §3; the count of sub-scores that fed it (duplication arrives with snapshots) | — |
| `multi_stack_pct`, `breadth_index`, `techs_per_dev` | §5 over the bucket | people with lines in breadth roles |
| `ai_assist_commit_pct`, `ai_assist_line_pct` | §6; absent when `ai_attribution = false` | commits · added source lines |
| `ai_compare_<ai\|human>_<commits\|added\|med_size\|test_pct\|doc_pct>` | §6, period `last-12m` | — |
