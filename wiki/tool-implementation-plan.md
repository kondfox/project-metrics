# Portable metrics tool — implementation plan

**Status:** M1–M4 built (2026-10-11): the git-derived core reproduces the prototype (M1a, see
[parity.md](parity.md) §6), a workspace can be set up, collected incrementally and resumed from the CLI
(M1b), one project is viewable end to end in the dashboard (M2), the snapshot metrics (duplication,
complexity, dependency vulnerabilities, secrets, SAST, hotspots, the Security score) reproduce the
prototype (M3), and the PR flow from GitHub and GitLab (peer review, cycle time, review wait, PR size,
mentoring) reproduces it too (M4). M5 (LLM classifier, Velocity) is **built** (2026-10-11): every
provider, the confidence gate, `classifier eval`, Velocity end to end; its exit criterion (a remote and
a local classifier passing the eval against the private reference set) is still to run, see §8. M6 (local
fleet) is **done** (2026-10-11): `fleet.toml`, pooling, `pmx fleet serve|export`, the fleet page with
drill-down, rebuilt from the three pilot projects' workspaces, see §8. M7 (packaging, guided `init`,
docs) is **built** (2026-10-12); its exit criterion (someone new onboards in 15 minutes) is still to run,
see §8. Drafted
2026-10-08, revised 2026-10-09 with owner decisions.
**What** to measure is defined by [metrics-spec.md](metrics-spec.md) (source of truth). This page covers
**how** to build the tool, which runs anywhere and shows its results in a web dashboard. Where this page
changes the spec, it says so with **⟂ spec change**. The spec must be updated to match before M1.

Name: **`pmx`** (D41).

---

## 1. Goal and decisions

A developer downloads one binary, points it at their project's repos, runs it, and opens a dashboard
in the browser. A second command combines several projects into the **fleet view**.

| # | Decision | Date |
|---|---|---|
| D1 | **Rust**, single static binary; dashboard embedded in it | 2026-10-09 |
| D2 | **Fleet data stays local** for now: a `fleet.toml` lists local project folders. Remote shared storage is decided later | 2026-10-09 |
| D3 | **Configurable LLM classifier** (Jev, a local model for sensitive repos, other LLMs). **All providers use Jev's question/answer schema** (§6) | 2026-10-09 |
| D4 | `pmx collect` shows **progress and an ETA** (§5) | 2026-10-09 |
| D5 | jscpd and semgrep stay **optional external tools** in v1, not rewritten in Rust (the numbers would change) | 2026-10-09 |
| D6 | Config format is **TOML**, with an importer for the prototype's `config_*.json` | 2026-10-09 |
| D7 | **Open source**, dual-licensed **MIT OR Apache-2.0**, at `github.com/kondfox/project-metrics`. The repo holds the tool and this wiki. Company-specific pilot data and the Python prototype stay private | 2026-10-09 |
| D8 | **M1 is split.** M1a = the numbers (config, classification, git ingest, per-day store, M1 metrics, `project.json`, parity). M1b = setup commands and progress/ETA. Trust the numbers before polishing usability | 2026-10-09 |
| D9 | `project.json` keeps per-person day components (needed to recompute multi-stack, active devs and commits per dev for any range) under **pseudonymous ids** (`p1`, `p2`, …). The id → name map lives only in `private/leads.json` | 2026-10-09 |
| D11 | **Resume unit = one repo's stage** (ingest, rework walk). Each finished stage is written to the cache at once, so Ctrl-C loses only the stages in flight. A stage continues from its cached SHA when the new tip descends from it; otherwise (force-push, `--full`) the repo is recomputed | 2026-10-09 |
| D12 | Git reads are prefiltered on the **committer date** (`--since-as-filter`, git ≥ 2.38, 7 days before the author-date bound) so incremental runs don't re-diff old history; the exact author-date filter stays in code (spec §1.1). Older git works, just slower | 2026-10-09 |
| D13 | `[fte]` is `source = "static"` with `value` or dated `periods`, or `source = "file"` with the spec §10.2 JSON (replaces the `fte.toml` placeholder) | 2026-10-09 |
| D14 | ~~Dashboard without a JS build step (plain JS + vendored ECharts)~~ **Superseded by D18** the same day: the dashboard will grow, so it needs a component base | 2026-10-09 |
| D18 | **Dashboard in React 19 + TypeScript** (strict), built by Vite into one HTML file (a small in-repo plugin inlines JS and CSS, so D17 holds). ECharts 6 from npm, tree-shaken. Reusable components (tiles with their three states, a pure chart-option builder, a typed table), a metric registry for presentation, a provider + hooks for state. ESLint, Prettier and Vitest; `npm run check` in CI. `pmx`'s build script runs the npm build; without Node it embeds a placeholder page (CI forbids that with `PMX_REQUIRE_WEB`). Architecture: `crates/pmx/web/README.md` | 2026-10-09 |
| D19 | **TypeScript types are generated from the Rust model** (`ts-rs`, `pm-metrics --features ts` → `crates/pmx/web/src/generated`); CI fails when they are stale. Metric math stays in Rust (WASM engine), never in TypeScript | 2026-10-09 |
| D20 | **`pmx demo <dir>`** writes a fictional four-repo project (git fast-import, deterministic) and collects it: for trying pmx, for the dashboard's dev server (`npm run dev-data`) and for screenshots, with no real data anywhere | 2026-10-09 |
| D15 | **WASM engine:** `pm-wasm` exposes pm-metrics over a raw C ABI with JSON in and out (no wasm-bindgen). `pmx`'s build script compiles it for `wasm32-unknown-unknown`; without that target pmx still builds, and the dashboard falls back to whole weeks and months | 2026-10-09 |
| D16 | **Bit-identical floats everywhere:** pm-metrics uses the pure-Rust `libm` for `ln`/`exp` (entropy, Quality geomean), so native builds on any OS and the WASM engine agree to the last bit. A test checks every week and month of a fixture through the WASM module | 2026-10-09 |
| D17 | **One self-contained page:** data, ECharts and the WASM engine are inlined into one HTML file, so it works from `pmx serve`, as a shared file, and opened from disk. `pmx serve` binds to 127.0.0.1 only and includes the private lead view; `pmx export --format html` leaves it out unless `--with-private` (written under `out/private/`) | 2026-10-09 |
| D21 | **Snapshot stage** (`pm-snapshot` crate): trees via `git archive`; raw tool results cached per (repo, commit, tool version + options), so installing a tool or changing rules only runs that tool; classification (test paths, triage) applied after the cache. osv-scanner results are keyed by lockfile content and **day**. Scans run on a bounded pool (`[snapshots] jobs`, default 2) | 2026-10-09 |
| D22 | **`pmx tools install`** downloads the pinned scc, osv-scanner and gitleaks from their GitHub releases into the user cache (`PMX_TOOLS_DIR` overrides) and refuses any download whose SHA-256 differs from the digest compiled into pmx. semgrep and jscpd stay user-installed (`pmx doctor` says how) | 2026-10-09 |
| D23 | ~~No bundled SAST rules yet~~ **Superseded by D24** | 2026-10-09 |
| D24 | **Our own SAST rules, `pmx-sast@1`** (owner decision): 34 rules written from scratch under MIT OR Apache-2.0, bundled in the binary and used unless a project configures others. Precision over coverage (SAST is a trend); semgrep tests and a duplicate check in CI; versioned (any change is `@2`). Checked against the three pilot projects: 7 findings, all plausible, after excluding migrations from the SQL rules | 2026-10-11 |
| D25 | **Code-host adapters** (`pm-codehost`): GitHub over **GraphQL** (one request per 25 PRs with reviews, and files for per-file repos; no `gh` needed), GitLab over REST (MR list, then notes four at a time). HTTP sits behind a small `Http` trait, so tests use fakes and the parity harness replays the golden set's recorded responses by request path | 2026-10-11 |
| D26 | **PRs are fetched incrementally and stored one by one** in the cache: ask for PRs *updated* since the last complete fetch minus 24 h, skip those stored at the same `updated_at`. A fetch that fails falls back to the stored PRs with a warning; a repo with nothing stored makes the whole PR layer "not measured" rather than undercounting. A run `--as-of` a past date skips later merges and is not recorded as a fetch | 2026-10-11 |
| D27 | **Expert status over a trailing year** (spec §8): ≥ 3 PRs and ≥ 25% of the stack's top author, counted over PRs merged in the 365 days before each PR's merge date, so a month's mentoring pairs never change when newer PRs arrive. Logins resolve through `[people]`, so a reviewer is "another person", not just another login | 2026-10-11 |
| D28 | **`claude-code` provider** (owner decision): runs the Claude Code CLI (`claude -p`) under the user's own login, so a Claude subscription can classify without an API key. Output forced with `--json-schema` (the generated schema of §6.2), no tools, no settings, no session, from an empty temp folder. Not local (the guard refuses it for `local-only` repos). One answer per question, so `confidence_source = "none"`. The model that answered is read from the CLI's `modelUsage` and recorded per package; `pmx check` warns when `model` is an alias (`sonnet`) instead of a versioned id. `pmx classifier test <name>` scores one fictional package to check the login | 2026-10-11 |
| D29 | **Velocity stage rules** (M5): a package goes to the classifier of its `local-only` repo if it has one, else of its first repo; a package the guard refuses or a call that fails stays **unscored**, and any week or month holding an unscored package has no Velocity (null, never a partial sum). After 3 failures in a row a classifier gets no more packages in that run (an expired login fails every call). Answers are cached by content (§2.2) and kept across `--full`; `--no-cache` re-pays. `[velocity] since` limits how much history is scored; ranges starting earlier are null. The state carries no person names (the ticket id only); author-week ids are pseudonymized in `project.json` | 2026-10-11 |
| D30 | **API providers** (M5): `jev` passes Jev's request through (`POST /v1/systemone`, Bearer key from `TYPESAFE_API_KEY`, `confidence_source = "model"`). `anthropic` calls the Messages API over raw HTTP (Rust has no official SDK) with `output_config.format` = the generated schema minus numeric bounds (the API rejects `minimum`/`maximum`), an optional `effort`, and `fallbacks: "default"` on models that take it, so a declined request runs on another model server-side; the answering model is recorded and `collect` warns when more than one answered. `openai-compatible` uses `response_format` json_schema (strict), temperature 0 unless sampling, `effort` as `reasoning_effort` (`none` turns a thinking model's reasoning off), and asks for logprobs: the top alternatives at the **last** occurrence of each level digit become that question's probabilities (`confidence_source = "logprobs"`; a thinking model's logprobs start with its reasoning); a server that rejects logprobs is asked again without. `samples = k` (frequencies as probabilities) and `isolate_questions` wrap any generative provider. Retries: 429, 529, 5xx and dropped connections with backoff | 2026-10-11 |
| D31 | **Confidence gate in `collect`** (plan §6.4): only providers with a real confidence (Jev, logprobs, sampled) are gated. Below `confidence_gate`: re-ask once with every Score question's levels listed top-down, mapped back; still below: the `fallback` classifier if the package's data policy allows it (`escalated`), else the surer answer is kept (`low_confidence`). The gate and fallback are part of the answer cache key. `velocity.health` counts packages, unscored, escalated and low-confidence; the dashboard shows them | 2026-10-11 |
| D32 | **`pmx classifier eval`** (plan §6.5): the reference set is ids and scores only (`project,id,R1..R6`, CSV or JSON); pmx rebuilds each package from the workspace's repos as `collect` does, so the candidate sees exactly what it would in production. Reports quadratic-weighted κ per axis, points, Velocity drift, test–retest (a second run that never reads the cache), unsure and escalated answers, and ids not found. **Pass:** ≥ 20 packages, κ ≥ 0.6 on every axis, drift within ±10%, test–retest ≥ 80% same points. Terminal only | 2026-10-11 |
| D33 | **Fleet page = the dashboard app with a drill-down** (owner decision, M6): the same React bundle renders the fleet when its data is a fleet. `fleet.html` carries `fleet.json` and every project's `project.json` (never `leads.json`), so it stays one self-contained, shareable file (D17); a project row opens that project's dashboard in the same page (`#p=<key>&monthly&…`, cadence and range kept both ways). One WASM instance holds every project by key and pools any range (`fleet` op); a node check proves it reproduces every bucket of `fleet.json` | 2026-10-11 |
| D34 | **Fleet row pooling** (owner decision, M6; `pm-metrics` `fleet`): flow ratios pool Σ numerators ÷ Σ denominators over every project; people stay per project (pseudonymous ids can't be matched across projects, so one person in two projects counts twice). AI-assisted pools only projects that attribute AI; peer review only projects with PR data; duplication joins the fleet's Quality only when every project measured it. Security shows the **worst** project (a gate, [quality-index.md](quality-index.md) §2). Velocity pools Σ points ÷ Σ FTE-days only when every scored project has the same rubric and classifiers and the same denominator (all FTE or all engineer-days), else "not comparable". Each pooled value records which projects fed it (shown as "2/3"); the table also counts projects per band, since a fleet scalar can hide one project falling | 2026-10-11 |
| D35 | **Projects collected on different dates** (owner decision, M6): the fleet's as-of is the latest; an earlier project is marked "as of <date>" with a notice and a CLI warning, and its missing days count as missing, not zero | 2026-10-11 |
| D36 | **`six-axis@3`** (owner decision, approved as written; implemented 2026-10-11, the default rubric): the `@1` anchors asked as one yes/no per anchor with a probability from every provider (`stated` for `claude-code` / `anthropic`), level = highest clear yes (P ≥ 0.65), R1 from a per-file layer choice, R5 without folder counting, fixed worked examples; packages unchanged; validated Opus-`@3` vs Sonnet-`@3`. See §6.4.1 | 2026-10-11 |
| D37 | **PR / MR packages** (owner decision): work packages take the first matching rule: the ticket id in the subject, else the merged PR/MR the commit belongs to, else author-week (spec §4). Untagged commits of a PR whose ticketed commits name one ticket join that ticket. The code-host fetch gains each PR's commits (GitHub: in the same GraphQL query; GitLab: `/merge_requests/:iid/commits`, only when a classifier is configured) and its merge/squash commit; commits match by SHA, else (rebased PRs) by author email + subject within a day, else (GitHub) a squash subject ending in `(#n)`, which works offline too. Stored PRs without commits are fetched again once. A repo without PR data falls back with a warning and a `velocity.health` mark. Velocity now runs after the pull stage | 2026-10-11 |
| D38 | **PR membership from git where the host is silent** (owner decision): a GitLab instance returned empty commit lists for most merged MRs (`X-Total` > 0, empty body), and a moved repo's older MRs live in another project. So, after the host's own commit list, a merge commit decides membership: a merge whose SHA is a PR's landed SHA, or whose message names a PR, owns `merge^1..merge^2`'s non-merge commits (earliest merge wins; merges from long-lived branches skipped; MRs of another project path get `<repo>:<path>!<n>`). Then the rebase match and `(#n)` (spec §4 rule 2) | 2026-10-11 |
| D39 | **Migrations stay in the Velocity diff** (owner decision): the rule "never in the diff: `migrations/` (generated)" is dropped. Hand-written migrations (e.g. Kysely, Knex, SQL files) are the main evidence for R2 and for the `database` layer; dropping them hid schema work from the classifier, and the case-sensitive match left framework-generated `Migrations/` folders in anyway. Packages with migrations get a new state, so their cached answers are not reused | 2026-10-12 |
| D40 | **People in the fleet drill-down, lead's copy only** (owner decision; amends D33): a project opened from the fleet shows the per-person lead view (Multi-stack leaders, points by person) when the page carries that project's `out/private/leads.json`. `pmx fleet serve` includes them (only this machine can connect, like `pmx serve`); `pmx fleet export --with-private` writes `out/private/fleet.html` with them and nothing else. The default `fleet export` (`fleet.json`, `fleet.html`) stays shareable, with no people | 2026-10-12 |
| D41 | **The tool is `pmx`** (owner decision, M7): the working name becomes the name; "working name" is dropped from the CLI, the crate and the docs | 2026-10-12 |
| D42 | **Distribution** (owner decision, M7): GitHub Releases built by `cargo-dist` for macOS arm64/x64, Linux x64/arm64 and Windows x64, each with the dashboard and the WASM engine built in (CI fails rather than ship the placeholder page), shell and PowerShell installers, and a **Homebrew tap** (`kondfox/homebrew-tap`, `brew install kondfox/tap/pmx`). Releases also serve `cargo binstall`. **Not** on crates.io in v1: the build needs Node and every `pm-*` crate would have to be published | 2026-10-12 |
| D43 | **`pmx init` is a guided setup** (owner decision, M7): in a terminal it lets you edit each suggested repo (keep, role, branch, drop), asks for the name and start date, checks the code-host token, offers a Velocity classifier (only what is found on the machine, default none, since diffs then leave it), writes `pmx.toml`, then offers the `[people]` merges, `tools install`, runs `check`, and offers `collect` and `serve --open`. Every step can be skipped. `--yes`, or stdin that is not a terminal, keeps today's behaviour (write the suggestions, nothing else); `--interactive` asks even without a terminal (for tests and scripts). The separate commands stay | 2026-10-12 |
| D44 | **Fleet cells show a trend line and a change arrow** (owner decision; amends the M6 fleet page): under each value a sparkline of its weeks or months over the chart window (at least a year back to the range end, as the dashboard's charts), scaled to its own min and max, gaps kept, the latest point dotted. Next to it ▲/▼ and how much it moved since the period just before (the same number of whole months when the range is whole months, else the same number of days; none when that period starts before the first day measured). Percentages move in whole points. Green up / red down only where a rise is good news (Quality, Security, multi-stack, peer review); Velocity, AI-assisted and active devs stay grey, since a rising Velocity is easy to game. The table shows no low-n badge (owner decision); a project's own dashboard keeps it (spec §1.1) | 2026-10-12 |
| D45 | **No "Over time" section on the fleet page** (owner decision; amends D33): a chart with a line per project stops reading at ~20 projects, and each table cell now carries its own trend line (D44). Each project's charts stay on its own dashboard, a click away. Velocity is the first column, then Quality | 2026-10-12 |
| D46 | **The project dashboard follows the fleet's column order** (owner decision): headline tiles Velocity, Quality, Security, Multi-stack, AI-assisted, Peer review, and the sections below in the same order. Then Code health, the one area without a fleet column (the Activity section was folded in, D54). Each headline tile scrolls to its section on click (no `#anchor`: the hash holds the view state) | 2026-10-13 |
| D47 | **FTE box in the Velocity section** (owner decision): next to Velocity, the mean FTE over the range's business days and the FTE-days Velocity is divided by (spec §10.2, counted up to as-of like `fte_days`). With no `[fte]` it reads "not measured", with a business day lacking FTE "no data"; both say Velocity then uses active engineer-days. The dashboard computes it from `velocity.options`, and a chart beside Velocity plots it per week or month (shown when `[fte]` is set), ready for weekly FTE from the provider | 2026-10-13 |
| D48 | **Sticky one-line header** (owner decision): on the project and fleet pages one line (← Fleet, the name, the cadence/range controls, as-of) stays at the top while scrolling, so the page always says what is shown and takes little room; it wraps only when narrow. Notices sit below and scroll away; sections a headline tile scrolls to land below it. The repo count, "measured since" and the chart key (* running, hollow = low n, ~ point in time) live in the footer; the fleet table's key explains ▲▼ | 2026-10-13 |
| D49 | **"Last 30 days" range preset** (owner decision): the 30 days ending on as-of (clamped to the first day measured), between "Last complete week" and "Last 3 months". Not a whole week or month, so it needs the metrics engine like the multi-month presets; its ▲▼ compares with the 30 days before | 2026-10-13 |
| D50 | **Dark / light switch** (owner decision): a ☀/☾ button at the end of the header. The page follows the system until the viewer chooses; the choice sits in the browser's storage (per viewer, never in the data) and charts recolor with it | 2026-10-13 |
| D51 | **One color per stack role** (owner decision): each role keeps one palette color (frontend, backend, mobile, QA, infra, data, docs = `--series-1`…`7`), used by the Stack mix chart and by the role badges in the Multi-stack leaders table | 2026-10-13 |
| D52 | **People counts are never low-n** (owner decision; amends spec §1.1 for these metrics): Multi-stack devs, Breadth index and Techs per dev count people, and a project rarely has 10 developers, so the flag would always be on. They show no low-n badge, no faded value and no hollow chart points (`noLowN` in the metric registry). Breadth (leaders table) and the Breadth index carry a "?" with what they mean | 2026-10-13 |
| D53 | **Work packages by size chart** (owner decision): in the Velocity section, above the packages table, stacked bars of how many packages of each rubric size (XS…XXL, plus "not scored" when any) each week or month delivered, counted by package date from `velocity.packages` | 2026-10-13 |
| D54 | **No Activity section** (owner decision): its git throughput moves to Velocity (Lines added, Commit size and Commits per dev boxes; Commits and lines added, Commit size charts), shown even when Velocity is not measured; Stack mix % moves to Multi-stack. Its Commits and Active devs boxes go (commits stay in the chart, active devs in the Multi-stack chart and headline note). Commits per dev counts people, so it is never low-n (D52) | 2026-10-13 |
| D55 | **Every metric box links to its definition** (owner decision): a ↗ in the box's corner opens the metric's section of `wiki/metrics-spec.md` on GitHub (`main`), from the registry's `spec` field (FTE: §10.2). The anchors live in `lib/docs.ts`; a test checks each against the spec's headings, so renaming a heading means updating the anchor. Not-measured boxes have no link | 2026-10-13 |
| D10 | The parity harness is a workspace crate (`pmx-parity`, not published) that reads the golden set from `PMX_GOLDEN_DIR` and prints to the terminal only. It runs twice: in the **prototype dialect** (the prototype's test rules and git reading) every M1 series must match exactly; in the spec dialect the intentional differences are reported | 2026-10-09 |

---

## 2. Architecture

```
pmx init / repo / people ──► pmx.toml (project definition)
pmx collect ─┬─ git ingest ──────────► per-day components ─┐
             ├─ snapshot tools (scc · jscpd · osv-scanner · gitleaks · semgrep)
             │    per week-end/month-end SHA               ├─► .pmx/cache.sqlite (incremental, resumable)
             ├─ code-host adapter (GitHub / GitLab PRs)    │
             └─ classifier (Jev / local / Claude …) ───────┘
                                   └─► out/project.json (spec §10.6) + out/private/leads.json
pmx serve | pmx export ──► project dashboard (static, embedded)
pmx fleet serve|export --manifest fleet.toml ──► fleet.json + fleet dashboard
```

### 2.1 Cargo workspace

| Crate | Responsibility |
|---|---|
| `pm-config` | Load and validate `pmx.toml`; `[people]` identity, bots, externals (spec §1.3); FTE provider (spec §10.2); JSON-config importer; `fleet.toml` (§3.4) |
| `pm-classify` | File class, test detection, stack role, technology (spec §1.4–1.5). Pure functions, table-driven tests |
| `pm-git` | Streaming parsers for `git log --numstat` and the rework walk (`log -p -w`); snapshot trees via `git worktree`/`archive`; fetch |
| `pm-snapshot` | Finds, runs and parses the external tools (pure parsers, tested on sample output); turns raw results into per-repo summaries (D21) |
| `pm-codehost` | GitHub (GraphQL) and GitLab (REST) adapters behind `CodeHost::merged_prs()` (spec §10.4), an `Http` seam for fakes and replays, the `gh pr list` export reader (D25) |
| `pm-llm` | The classifier: the shared schema (§6.1), providers, data-policy guard, cache, eval harness |
| `pm-metrics` | Roll-ups, ratios, scores (Quality, Security, Velocity, multi-stack); fleet pooling and `fleet.json` (D34). No I/O. **Also compiled to WASM** for the dashboard |
| `pm-progress` | Work planning, ETA model, renderers (TTY / plain / JSON) (§5) |
| `pmx` (lib + bin) | Collect pipeline, `.pmx/cache.sqlite`, outputs, export; CLI (`clap`); `serve` (a small std-only loopback server); `demo` (D20); the built dashboard embedded as one page (D17, D18) |
| `pm-wasm` (cdylib, unpublished) | pm-metrics for the browser: custom-range metrics over `days` (D15), several projects by key and the pooled fleet row (D33) |
| `pmx-parity` (bin, unpublished) | Parity harness against the private golden set (D10, [parity.md](parity.md)) |
| `crates/pmx/web/` | React + TypeScript dashboard with ECharts (D18); loads the `pm-wasm` engine; types generated from `pm-metrics` (D19). Inside the `pmx` crate so the crate can be packaged |

### 2.2 Technical choices

- **Shell out to `git`, not `gix`, in v1.** The spec defines its metrics by `git log --numstat` and
  `log -p -w` semantics (whitespace-ignoring diffs), so shelling out gives exact parity with the
  prototype. Revisit `gix` only if speed becomes a problem.
- **One metrics implementation for CLI and browser.** The dashboard recomputes custom ranges from
  `days` (spec §10.6). Running the same Rust code via WASM rules out the drift the prototype already
  has: the published fleet page is ahead of `collector/c.js`.
- **SQLite cache (`.pmx/cache.sqlite`).** Stores each repo's stage results keyed by `(repo, stage,
  settings hash)` with the SHA they were computed at (M1: the ingest's per-day components and the rework
  counts), later snapshot results by SHA, LLM answers by `(package id, diff hash, rubric version,
  provider, model version)` and PR pages, plus per-stage timings for the ETA. Every finished unit is
  committed at once, so runs are resumable (D11). The settings hash covers what changes a result besides
  the SHA (pmx version, `range_start`, role, role rules and `[people]` for the ingest), so editing
  `[people]` redoes the ingest but not the rework walk.
- **Incremental runs.** Ingest reads only `cached SHA..tip` and adds it: exact, since every component is
  a per-commit sum. The rework walk restarts 21 days before the earliest new author date and replaces
  every day from that date on. Tests check both against full runs, including a back-dated commit. An
  incremental rework can differ from a full one only when author dates run backwards across that 21-day
  boundary; `pmx collect --full` recomputes everything.
- **External tools are optional.** `pmx doctor` reports what is installed. `pmx tools install` downloads
  pinned versions of the Go binaries (scc, osv-scanner, gitleaks) into the user cache. jscpd (Node) and
  semgrep (Python) are the setup friction. A missing tool shows as "not measured" (spec §10.5).

---

## 3. Developer setup for a multi-repo project

### 3.1 Workspace layout

One project is one **workspace folder**. The repos it lists can live anywhere.

```
~/metrics/acme-shop/
  pmx.toml        ← project definition (safe to commit; tokens only as env-var names)
  .pmx/           ← cache + bare clones of URL repos (gitignored)
  out/            ← project.json; out/private/ is never shared
```

### 3.2 Commands

| Command | What it does |
|---|---|
| `pmx init [dirs…]` | Guided in a terminal (D43). Scans for git clones. Suggests a branch per repo (from `origin/HEAD`) and a role from the repo's contents: `*.csproj` → backend, React/Vite → frontend, `android/`/`ios/` → mobile, Terraform → infra, Cypress/Playwright → qa, mixed → `per-file`. Reads the code-host type and project id from the remote URL. Lockfiles are auto-discovered. You confirm the suggestions; it writes `pmx.toml` |
| `pmx repo add <path\|url> [--role] [--branch] [--name]` · `list` · `set <name> [--role] [--branch] [--rev] [--rename] [--code-host-id]` · `remove` | Edit the repo list. For a URL, pmx keeps a bare clone in `.pmx/repos/` and only reads from it, so working copies are never touched. `add` suggests role and branch like `init` |
| `pmx people` · `--apply` · `merge "<Person>" <id>…` · `bot <id>…` · `external <id>…` | Lists every author identity across **all** repos since `range_start`, plus the code-host logins (authors and reviewers) of the PRs the last `collect` stored, and proposes merges: same normalized name, `first.last` email local part or login, a login's display name, GitHub noreply login. Flags automated-looking identities as bot candidates. `--apply` (or answering yes) writes `[people]`; `merge`/`bot`/`external` fix the rest. This step is what makes cross-repo per-person metrics (multi-stack, active devs, mentoring) correct |
| `pmx check [--threshold 10]` | Every path exists (URL repos: reachable), every branch resolves, lockfile overrides exist, tokens are present, repos lack no `code_host_id` when `[code_host]` is set, the `local-only` data policy holds for the classifier and its fallbacks, the FTE file has the project, and no identity with ≥ threshold commits is missing from `[people]` (an unmapped login with ≥ threshold PRs and reviews is a warning). Exit code 1 on any error |
| `pmx import-config config_acme.json` | Converts a prototype config (plus `externals.json`, `fte.json`, `secrets_triage.json`, found next to it or in `../tools/`, or passed with flags) |
| `pmx plan` (= `collect --dry-run`) | Prints the work per stage and repo (cached, incremental, full) and the ETA, without running anything |

`pmx collect` runs `git fetch` by default. This only updates remote-tracking refs. `--no-fetch`
measures what is already there.

### 3.3 Config example

```toml
[project]
name = "Project A"
range_start = "2024-09-01"
breadth_roles = ["frontend", "backend", "mobile", "qa", "infra"]
ai_attribution = true

[[repo]]
path = "~/src/acme/shop-api"          # local clone…
branch = "origin/development2"
role = "backend"
code_host_id = 785

[[repo]]
url = "git@gitlab.example.com:acme/shop-web.git"   # …or a URL pmx clones itself
role = "frontend"

[[repo]]
path = "~/src/acme/shop-legacy"
role = "per-file"                 # monorepo role rules for this repo only (spec §1.5)

[code_host]
type = "gitlab"
base = "https://gitlab.example.com"
token_env = "GL_TOKEN"

[people]
"Jane Doe" = ["jane.doe@example.com", "jdoe"]   # emails and logins together
bots = ["gitlab-runner@example.com"]
externals = ["Sam Contractor"]

[fte]                             # spec §10.2
source = "static"
value = 4.5

[[secrets_triage]]
repo = "shop-api"
file = "certs/dev.pem"
verdict = "rotated"
by = "…"
date = "2026-10-08"
```

**⟂ spec change (§1.5, §10.1):**
- The project-level `monorepo: true` becomes **`role = "per-file"` on a single repo**, so a project can
  mix a monorepo with single-role repos.
- `identity` (email → person) and `usernames` (login → person) merge into one **`[people]`** table:
  canonical name → list of emails and logins.

**Combining the data:** every repo writes into the same per-day store under canonical people. Ratios
pool numerators and denominators across repos (spec §1.2), so the output is one project.

### 3.4 Local fleet (D2)

```toml
# ~/metrics/fleet.toml
name = "Acme Engineering"
projects = ["shop", "crm", "mobile-app"]        # workspace folders, relative to this file
```

`pmx fleet export` reads each project's `out/project.json`, **never** `out/private/`, for the shareable page.
Only the lead's own copy (`fleet serve`, `fleet export --with-private`) adds the projects' `leads.json` (D40). It pools
numerators and denominators across projects (never averaging ratios) and builds the fleet page. Every
headline links to that project's dashboard. Remote publishing (shared git repo, bucket, or server) is
deferred, so the manifest entries are paths for now. The entry type should be extensible to URLs later.

As built (M6): `pmx fleet serve|export [--manifest fleet.toml]` (the manifest defaults to `fleet.toml` in
the `-C` folder). `export` writes `out/fleet.json` (spec §10.8) and `out/fleet.html` next to the manifest;
`serve` re-reads every `project.json` on reload. The page lists one row per project (Velocity, Quality,
Security, multi-stack, AI-assisted, peer review, and active devs as context), the pooled fleet row (D34),
and the projects per band; every value carries a trend line and a change arrow against the period
before (D44), in place of charts (D45); each project's Velocity cell
names the classifier that scored it. Rows link into the project dashboards (D33). Folder names are the
project keys (`-2`, `-3` for repeats). `pmx demo <dir> --fleet` makes a fictional fleet of three
projects that differ in what they measure (one without AI attribution, one without PR data).

### 3.5 Privacy split

Per-person drill-downs (multi-stack leaders, per-person Velocity points) are private to the lead
([metrics-overview.md](metrics-overview.md)). `collect` writes them to `out/private/leads.json`, which
only the project dashboard loads, and only when that file is present locally. `project.json` holds
team-level data only.

---

## 4. Collection pipeline

1. **Plan:** count the remaining work units per stage (§5.1).
2. **Fetch** the repos.
3. **Git ingest:** commits → per-day components (activity, tests/docs-with-code, AI, per-person-role lines).
4. **Rework walk** (spec §3.1).
5. **Snapshots:** for each week-end and month-end SHA not yet cached, run scc, jscpd, osv-scanner,
   gitleaks and semgrep. These run in a bounded worker pool.
6. **Pull requests:** paginated, each PR stored as it arrives; later runs ask only for PRs updated
   since the last fetch (D26).
7. **Velocity:** build work packages → classifier (§6) → points.
8. **Roll up:** `pm-metrics` produces `weeks[]`, `months[]`, scores and `meta`, then writes `project.json`
   and `private/leads.json`.

Stages 5 and 6 run concurrently with each other. Stage 7 needs stage 3.

---

## 5. Progress and ETA (D4)

### 5.1 Planning the work

Before any heavy work, `collect` counts what's left with cheap queries, minus what is already cached:

| Stage | Unit | How it's counted |
|---|---|---|
| Fetch | repo | Repos that follow a branch (or URL repos not cloned yet) |
| Git ingest | commit | `git rev-list --count` per repo, from the last processed SHA |
| Rework walk | commit | Commits since the cached tip's date minus 21 days (an estimate) |
| Snapshots | (SHA, tool) | Uncached week-end/month-end SHAs × installed tools, weighted by tree size (KLOC from a quick `scc`) |
| Pull requests | PR | Unknown until listed: the total grows as each repo's list arrives (as built in M4; a merged PR's details cost one request on GitLab, none on GitHub). Default 0.3 s per PR |
| Velocity | work package | Unscored packages. Known only after ingest; estimated from commits until then |

### 5.2 ETA model

`ETA = Σ remaining units × cost per unit`. Cost per unit is learned per stage and per tool (and per KLOC
for snapshots) from earlier runs stored in the cache. A first run uses built-in defaults. The estimate is
corrected live as each stage runs, and it accounts for stages running in parallel.

As built (M1b): timings are recorded per repo and stage (`timing` table; the last 50 per stage). Before a
stage runs its time is `units × learned cost ÷ workers`; while it runs, the observed wall-clock rate
takes over in proportion to the work done. Repos run on a worker pool (one per CPU core, at most one per
repo). Defaults: 2 s per fetch, 1 ms per ingested commit, 1.5 ms per walked commit.

### 5.3 Rendering

TTY (a block redrawn in place; no `indicatif` needed):

```
pmx collect · Project A                         overall ▕████████░░░░░░░░▏ 52%  ETA ~6m
 ✓ fetch          5 repos                                    4s
 ✓ git ingest     12,418 commits (9,870 cached)              18s
 ● snapshots      ▕██████░░░░▏ 61/104  jscpd shop-web @ 2026-W33   ~4m
 ● pull requests  ▕████████░░▏ 812/1,003                                ~40s
 ○ velocity       ~140 packages · classifier: jev                       ~1m
 ⚠ semgrep not installed: SAST panels will show "not measured"
```

- `--progress=plain` prints a status line every 10 seconds (for CI and pipes).
- `--progress=json` emits newline-delimited events for other tools.
- `--dry-run` (alias `pmx plan`) prints the work table and the ETA without running anything.
- **Ctrl-C** loses at most the units in flight (D11). The next run resumes.
- `--full` recomputes every repo; `--no-cache` neither reads nor writes the cache.
- **Final summary:** computed vs cached counts, skipped tools, warnings, classifier escalations and
  the share of low-confidence answers.

---

## 6. LLM classifier (D3)

### 6.1 One schema for every provider: Jev's

Jev's API format ([docs.typesafe.ai](https://docs.typesafe.ai/)) is the tool's internal contract.

**Request:**

```jsonc
{
  "state": "…",                         // text or JSON: the work package
  "questions": {
    "r2_data_model": { "type": "score",  "instructions": "…", "criteria": ["…", "…", "…", "…"] },
    "layer_persistence": { "type": "noul", "instructions": "…" },
    "kind": { "type": "choice", "instructions": "…", "criteria": { "fix": "…", "feature": "…" } }
  }
}
```

**Response:**

```jsonc
{
  "model": "jev-1.13.0",
  "answers": {
    "r2_data_model": { "type": "score", "score": 1.2, "probabilities": {"0": 0.1, "1": 0.6, "2": 0.3, "3": 0},
                       "confidence": 0.55 },
    "layer_persistence": { "type": "noul", "noul": 0.91 },
    "kind": { "type": "choice", "choice": "feature", "probabilities": {…}, "confidence": 0.8 }
  },
  "usage": { "input_tokens": 7412 },
  "confidence_source": "model"           // pmx extension: model | logprobs | sampled | none
}
```

Rust types mirror this exactly (`serde`). The scoring code consumes `answers` and never knows which
provider produced them.

### 6.2 Providers

| Provider | How it fills the schema |
|---|---|
| `jev` | Passes the request through to `POST https://api.typesafe.ai/v1/systemone` (raw HTTP via `reqwest`; there is no Rust SDK). Retries 429/529 with backoff |
| `openai-compatible` (Ollama, vLLM, llama.cpp, LM Studio, OpenAI) | Renders the questions into a fixed prompt template. Forces the output with a **JSON schema generated from the question set** (an enum of levels or options per question; a 0–1 number for Noul). Probabilities come from token logprobs where available |
| `anthropic` | Same template and generated schema, enforced through structured output |
| `command` | Any executable that reads the request JSON on stdin (plus a `context` field: the rubric's shared instructions) and writes the response JSON on stdout. This is the escape hatch for custom local classifiers |
| `claude-code` | The Claude Code CLI under the user's login (D28): same template and generated schema, forced with `--json-schema`. Built in M5 |

**Probabilities and confidence.** Only Jev returns calibrated probabilities. Other providers fill them
from logprobs, from sampling the question k times (`samples = k`), or from a single answer
(`confidence_source = "none"`, one-hot probabilities). Rules that depend on confidence (§6.4) fall back
to the plain answer when the source is `none`.

**Question isolation.** Jev judges each question separately against the same state. For generative
providers, `isolate_questions = true` makes one call per question with a shared prompt prefix (cache
friendly), which matches Jev. The cheaper default is one call per package.

The prompt template and schema generator are versioned with the tool. Their version is part of the
cache key for non-Jev providers.

### 6.3 Configuration and the data-policy guard

```toml
[classifiers.jev]
provider = "jev"
model = "jev-1.13.0"            # pin a versioned id, never jev-latest
api_key_env = "TYPESAFE_API_KEY"
confidence_gate = 0.5
fallback = "claude"

[classifiers.claude]
provider = "anthropic"
model = "claude-sonnet-5-5"
api_key_env = "ANTHROPIC_API_KEY"

[classifiers.sub]
provider = "claude-code"        # Claude Code CLI, your own login (D28)
model = "claude-sonnet-5-5"
jobs = 4                        # packages classified at once

[classifiers.local]
provider = "openai-compatible"
base_url = "http://localhost:11434/v1"
model = "qwen3-coder:30b"
samples = 3

[velocity]
classifier = "jev"              # project default

[[repo]]
path = "~/clients/secret-bank-api"
classifier = "local"            # per-repo override
data_policy = "local-only"      # hard guard
```

- **`data_policy = "local-only"`** (per repo or per project) makes pmx refuse any provider, or any
  fallback, whose endpoint is not loopback or a `command`. A misconfiguration fails at `pmx check`,
  before any code is sent.
- If one project uses more than one classifier, `collect` warns. `meta` records the classifier per
  package. The fleet view marks which classifier fed each project's Velocity, because scores from
  different models are not strictly comparable.
- **No classifier configured** → Velocity is `null` (spec §10.3), not 0.

### 6.4 Velocity scoring with the shared schema

The six-axis rubric ([`rubrics/six-axis@1.md`](../rubrics/six-axis@1.md)) becomes a
versioned **question set**, [`six-axis@2.toml`](../crates/pm-llm/rubrics/six-axis@2.toml) (bundled in
the `pm-llm` crate so the binary always scores with the version it reports). Jev's documented weak spots
([jev-1.13 jaggedness](https://docs.typesafe.ai/model-jaggedness/jev-1.13.md)) are unreliable counting,
poorly calibrated in-between scores, dates and arithmetic, and lower accuracy when the input contains
irrelevant text. The design therefore keeps counting and arithmetic in code:

| Axis | Question(s) | Decided in code |
|---|---|---|
| R1 layers | One **Noul** per named layer: transport/DTO, validation, service, domain, persistence, database object, UI, navigation | Count layers with p ≥ 0.5 → 1 = 0, 2 = 1, 3–4 = 2, 5+ = 3 |
| R2 data model · R3 contract · R4 algorithmic · R6 verification | One **Score** each, 4 levels from the rubric anchors, plus examples from the golden set | Most likely level, with the rubric's "when two anchors fit, take the lower" rule: choose the lower level if `P(lower) ≥ P(top) − δ` |
| R5 blast radius | Files and modules touched, counted in code, plus a **Noul**: "touches auth, sync, schema, ordering/filtering applied everywhere, or build/release?" | Yes → 3; else one file → 0, one module → 1, several → 2. A module is the file's repo plus its first two directories; files excluded from the diff don't count |
| Sum, size, points, verification debt | none | In code (spec §4, unchanged) |

**State:** the package's subjects, the files changed with their class and role, and the diff filtered
in code (generated paths removed and whitespace-only hunks dropped, capped at 24 KB ≈ 8k tokens). This
fits Jev's 32k-token limit for the state plus the longest question.

**Description:** Jev can't generate text, so the description is the ticket id plus the first commit
subject.

**Confidence gate:** if any answer's confidence is below the gate, re-ask once with the Score levels or
Choice options reordered (Jev tends to favour the first option). If it is still below the gate, send
the package to the `fallback` classifier (only if the data policy allows it) and mark it in the data.
The escalation rate is reported as a health metric.

**Cost:** at Jev's price of $0.042 per million input tokens, ~1,000 packages × ~8k tokens costs about
$0.35. One package is one call (70–500 ms).

⟂ **spec change (§10.3):** the output contract `{id, R1..R6, description}` stays as it is, but it is now
derived from schema answers. The rubric version bump (`six-axis@2`) invalidates the score cache.

### 6.4.1 `six-axis@3` (decided and implemented 2026-10-11)

**Why a new version.** The first evals on the pilot projects (private; method in "M5 progress") failed
for Sonnet *and* for Opus under `six-axis@2`: both scored a third to a half more points than the Opus
`six-axis@1` reference, with similar per-axis agreement. The same model scoring higher under `@2` puts
the cause in the protocol, not the model. Three defects of `@2`:
- **R5 counts folders.** A "module" is the repo plus the first two directories, so the same change gets
  a different blast radius in a deep monorepo, a flat repo or across repos. R5 had the lowest agreement
  in every run.
- **R1 claims are unchecked.** Eight independent "does it touch layer X?" questions; nothing ties a
  "yes" to a changed file, and the model never sees the total it is producing.
- **"Take the lower anchor" depends on the provider.** The rule needs a probability per level, but a
  provider without one (`claude-code`, `confidence_source = none`) returns one level as one-hot, so the
  rule never fires there and the gate is off. A scoring rule must not depend on the provider.

`@1` is not the alternative: it scores packages in batches (a package's score depends on its
neighbours, so it is neither fair nor cacheable), lets the model invent its own layers, and leaves
"take the lower" unchecked.

**What stays.** The anchor texts (from `six-axis@1.md`), the size and points table, the hard rules, one
package per call, the state (subjects, files with class and role, the filtered diff ≤ 24 KB), the cache
key (rubric version included), and **the work packages exactly as they are** (ticket id across repos,
else author-week).

**What changes** (exact question texts, prompt template and examples:
[velocity-metrics.md](velocity-metrics.md)).

| Axis | `six-axis@3` question(s) | Decided in code |
|---|---|---|
| R2 data model · R3 contract · R4 algorithmic · R6 verification | One **yes/no per anchor above 0** (three per axis), worded from the anchor, e.g. R2-2 "Does the change add a new table, entity, view, stored procedure or enum type?"; R6 as needs: "Is more than a glance needed…?", "Is a unit test required…?", "Is an integration, migration or end-to-end test required…?" | Level = the highest anchor answered a **clear yes**: P(yes) ≥ τ (τ = 0.65, in the question set). An anchor between 0.5 and τ is ambiguous and does not raise the level: "when two anchors fit, take the lower" as a rule, not a request. No clear yes → 0 |
| R5 blast radius | Yes/no per anchor: "Could a mistake break behaviour beyond the changed file, within one module?", "…across several modules?", and the `@2` app-wide question for 3. The file, repo and directory counts are given as context, not as the answer | Same highest-clear-yes rule. No folder counting |
| R1 layers | One **choice per changed file**: which layer it belongs to: the fixed eight (transport/DTO, validation, service, domain, persistence, database, UI, navigation) or "none". Files pm-classify marks test, docs or config are "none" without asking; at most the 40 files with the most changed lines are asked, the rest count as unknown (recorded) | Count distinct layers among the files → 1 = 0, 2 = 1, 3–4 = 2, 5+ = 3 (`@1`'s bands). Every layer is backed by a file, so each R1 can be audited |

**Probabilities from every provider.** Every yes/no answer is P(yes) in [0, 1], and every per-file
choice carries a probability for the chosen option:
- Jev and logprobs (`openai-compatible`) give them natively, as now.
- `claude-code` and `anthropic` are asked to **state** them in the forced JSON schema (a number 0–1 per
  question). New `confidence_source = "stated"`. A model that will only answer 0 or 1 still works: the
  rule degrades to plain yes/no.
- `samples = k` averages k stated answers (`sampled`), the most robust choice for `claude-code`.
- The confidence gate applies to every source. An answer's confidence is max(P, 1 − P); below the gate
  (per classifier, as now) it is re-asked once in a fresh call and the two are averaged, then it goes to
  the fallback as in D31.

**Fixed worked examples, never batches.** The question set carries three or four fictional, already
answered example packages (in the repo, versioned with the set) that go into every prompt. They give the
model the comparison points a batch gave `@1`, but the same ones for every package, so scores stay
independent and cacheable.

**Validation (decided for now: model against model).** Opus under `@3` scores the reference set's 240
packages one per call (no batches); that becomes the `@3` reference. Sonnet under `@3` is evaluated
against it with `pmx classifier eval` and the D32 thresholds, and Opus's own test–retest is reported.
The Opus `@1` set stays as a secondary comparison (drift only). Agreement between two models shows
consistency, not correctness; a human-labelled set remains the stronger check and is deferred.

**Cost.** About 15 yes/no questions plus up to 40 per-file choices, still one call per package: longer
calls, not more of them.

**Not comparable across versions.** `@2` and `@3` scores never pool (the fleet already requires one
basis, D34). Switching versions re-scores every package: the rubric version is part of the answer cache
key, so no `--full` is needed.

**How it is built (2026-10-11).**
- `six-axis@3.toml` sits next to `@2` (kept, so `@2` stays selectable and its cached answers readable).
  `[velocity] rubric = "six-axis@2" | "six-axis@3"` picks one; the default is `@3`; `pmx check` rejects
  an unknown id. A question set names its prompt template, and every generative provider's identity
  (so the cache key) carries that template: `@2` keys are unchanged.
- The per-file questions are built per package from the files `collect` already lists (class `prod` or
  `other`, most changed lines first, at most 40). The count of eligible files beyond 40 is recorded per
  package (`unknown_files` in `project.json`).
- Confidence of a yes/no answer is max(P, 1 − P) for every rubric (it was |P − 0.5| · 2; only a `jev`
  gate under `@2` sees the difference).
- The `@3` gate re-asks in a fresh call and averages: P(yes) and each option's probability are means;
  the choice is the option with the highest mean.
- `openai-compatible` under `@3` reports `stated`, not `logprobs`: reading probabilities for stated
  numbers and option names from token logprobs is not built yet.
- `pmx classifier score <name> --ids <file> --out <csv>` scores listed packages one per call (through
  the answer cache) and writes them as a `project,id,R1..R6` reference set with real ids, for the
  model-against-model validation above.

**First `@3` validation (2026-10-11, private pilots; method only).** A fresh sample per pilot project
from the current grouping (80 packages, stratified by month and package kind, fixed seed, ids only)
was scored by Opus under `@3`, one package per call, with `classifier score`; Sonnet under `@3` was
evaluated against it with `classifier eval` (D32 thresholds), and Opus against its own reference for
test–retest. Sonnet passed on most but not all projects; where it failed, the weak axis had almost no
spread in the reference (κ rests on a handful of packages) and Sonnet scored consistently below Opus.
Both models had similar test–retest. **Owner decision:** Sonnet stays the production classifier for
every pilot project (one basis, so the fleet keeps pooling, D34); its lower scores than Opus are a
known bias. A human-labelled set remains the stronger check.

### 6.5 Qualifying a classifier

`pmx classifier eval --classifier <name> --golden <set>`:
- The **golden set** is the work packages the prototype already scored (`tools/six_axis_score.py`
  output), frozen in M0.
- It reports per-axis agreement (quadratic-weighted κ), total-points agreement, project Velocity drift
  (target ±10%), test–retest stability (two runs), and the share of low-confidence answers.
- A classifier becomes a project's default only after it passes. Run this before trusting a local model
  on a sensitive repo.

### 6.6 Other Jev uses

| Candidate | Status |
|---|---|
| **Fix/revert classifier** (Choice on message + diff: "does this fix or revert recent work?") as a detector for the [change-failure proxy](change-failure-proxy.md), so teams don't have to adopt a commit convention first | After v1. Strong candidate |
| **Grouping commits without a ticket id into work packages** (Noul: "same logical change?") instead of `author~ISO-week` | After v1 |
| **AI-assisted detection** | **No.** Detection is deferred and needs AI-tool telemetry, not guesses from git ([fleet-vision.md](fleet-vision.md)) |
| **SAST, duplication, any trended snapshot count** | **No.** Must be deterministic (internal notes (not published) C7) |
| **Secrets triage** | **No.** Secret context never leaves the machine |

---

## 7. Distribution

- Release binaries built by `cargo-dist` (macOS arm64/x64, Linux x64/arm64, Windows x64), a Homebrew tap,
  and `cargo binstall pmx` (D42). No crates.io release in v1.
- `pmx doctor` and `pmx tools install` for external tools (§2.2).
- Onboarding target: a new developer goes from download to their first dashboard in under 15 minutes.
- CI templates (weekly GitLab job, GitHub Action) are **deferred** together with remote fleet storage (D2).

---

## 8. Milestones

| # | Deliverable | Done when |
|---|---|---|
| **M0** | Freeze the prototype outputs for Project A, Project B and Project C at pinned SHAs as golden files (private). Write the deviation register ([parity.md](parity.md)). Update metrics-spec.md with this page's ⟂ changes | Golden set + long-format table exist; the harness runs **locally** against them (golden data is private, so public CI uses synthetic fixtures instead) |
| **M1a** ✓ 2026-10-09 | `pm-config` (incl. the prototype-config conversion), `pm-classify`, `pm-git`, `pm-metrics`; `pmx collect` (fetch, git ingest, rework walk, per-repo cache) and `pmx export --format long`; activity, rework, tests/docs, multi-stack and AI metrics, weekly and monthly; `project.json` + `private/leads.json`; `pmx-parity` | Matches the golden files except the listed ⟂ differences. **Done:** exact in the prototype dialect for all three projects |
| **M1b** ✓ 2026-10-09 | `init`, `repo`, `people`, `check`, `import-config` (CLI over the M1a converter, plus `fte.json` and `secrets_triage.json`); the progress, resume and ETA framework (§5), incl. incremental ingest from the last processed SHA | A new workspace set up from scratch without hand-editing TOML; Ctrl-C and resume lose at most the units in flight. **Done:** `crates/pmx/tests/cli.rs` runs init → people → repo → check → collect → export; `tests/incremental.rs` checks incremental, resumed and rewritten-history runs against full runs |
| **M2** ✓ 2026-10-09 | Project dashboard: weekly/monthly, custom range via WASM, low-n and "not measured" states, `serve`/`export` | One project viewable end to end. **Done:** `pmx serve` / `pmx export --format html`; headline, Quality, activity, multi-stack (with the private leaders table) and AI sections; view state in the URL hash (`#monthly&2026-06-15..2026-09-30`); `tests/dashboard.rs` covers rendering, the privacy split, serving and WASM = CLI |
| **M3** ✓ 2026-10-09 | `pm-snapshot`: scc, jscpd, osv-scanner, gitleaks, semgrep; Quality with duplication; Security score; SAST trend; `doctor`, `tools install` | Quality and Security reproduced for all three projects. **Done:** every snapshot series exact in the parity replay for all three projects, and in the live tool runs ([parity.md](parity.md) §6); dashboard Security and Code health sections; `tests/snapshots.rs` covers the stage with fake tools |
| **M4** ✓ 2026-10-11 | `pm-codehost`: GitHub + GitLab; peer review, cycle time, review wait, mentoring (unified expert rule, spec §8) | Spec §8 complete. **Done:** every PR series exact against the recorded GitLab responses and the GitHub export for all three projects in the prototype dialect; in the spec dialect the only deltas are the months with PRs by bots or externals, and mentoring pairs (the unified rule) ([parity.md](parity.md) §6); `tests/pulls.rs` covers the stage against a fake GitLab (incremental fetch, fallback to stored PRs, not measured); dashboard Peer review section; `pmx demo` has PRs |
| **M5** | `pm-llm`: schema types, `jev`, `openai-compatible`, `anthropic` and `command` providers, data-policy guard, confidence gate, cache, `classifier eval`; Velocity on `six-axis@2` | At least one remote and one local classifier pass the eval |
| **M6** ✓ 2026-10-11 | Local fleet: `fleet.toml`, aggregation, fleet dashboard | Fleet rebuilt from the three local project folders. **Done:** see "M6 progress" |
| **M7** | Packaging, `init` wizard polish, docs | 15-minute onboarding verified by someone new. **Built:** see "M7 progress" |

**M5 progress (2026-10-11).** Built: the schema types, the `six-axis@2` question set and the scoring in
code (§6.4); all five providers: `claude-code` (D28), `command`, `jev`, `anthropic` and
`openai-compatible` (D30), with `samples`, `isolate_questions` and `effort`; the data-policy guard; the
confidence gate with its re-ask and fallback (D31); work packages and filtered diffs; the answer cache;
Velocity and its ratios in `pm-metrics` and the WASM engine (FTE-days from `[fte]`, engineer-days
fallback); per-person points in `leads.json` (each package split by its commits); the dashboard tile,
section, package table, health line and the lead's points table; `pmx classifier test`,
`pmx classifier eval` (D32), `collect --no-velocity`. Tested with fake `command` classifiers and fake
HTTP (`crates/pmx/tests/velocity.rs`, `crates/pmx/tests/classifier.rs`, `pm-llm` unit tests). Run live:
`claude-code` with Sonnet on the demo project (87 packages; `effort` and `samples` too), and
(2026-10-11) on the three pilot projects' private M6 workspaces: `claude-code` with `claude-sonnet-5-5`,
`[velocity] since` = the reference set's window, collected `--as-of 2026-09-30` at the golden pins. Every
package was scored by that one model (none unscored, escalated or low-confidence; no confidence gate, since
`claude-code` has no confidence); package counts and ticketed shares are close to the prototype's
extraction; the fleet pools Velocity per FTE-day (same classifier, FTE known everywhere), checked against
Σ points ÷ Σ FTE-days rebuilt from each project's FTE periods. Not an eval: no κ against the reference
scores yet. Nothing from the run is in this repo. **Left for the
exit criterion:** `classifier eval` of one remote and one local classifier against the private reference
set (needs `PMX_GOLDEN_DIR`'s reference as `project,id,R1..R6`, a local model server, and API keys for
`jev`/`anthropic`, none of which are on this machine); the fleet marking of each project's classifier
comes with M6 (done: the fleet page names each project's classifier, and pools Velocity only across
projects scored the same way, D34).

**M6 progress (2026-10-11).** Built: `fleet.toml` (`pm-config`), pooling and `fleet.json` (`pm-metrics`
`fleet`, D34), the keyed WASM engine with its `fleet` op, `pmx fleet serve|export`, the fleet page and
drill-down in the dashboard app (D33), stale-project handling (D35), `pmx demo --fleet`. Tested:
`pm-metrics` unit tests for every pooling rule, `crates/pmx/tests/fleet.rs` (CLI export, exact pooling of
two collected projects, missing project, serving, and the WASM engine against every bucket of
`fleet.json`), dashboard tests (`src/fleet/FleetApp.test.tsx`). Checked on the fictional demo fleet in a
browser. **Exit run (2026-10-11):** three private workspaces made with `pmx import-config` from the golden
set's pinned configs, each collected `--as-of 2026-09-30 --no-fetch` (month-end snapshots, all five tools,
PRs where a token was available), then `pmx fleet export`. A script independent of pm-metrics recomputed
every pooled monthly value from the three `project.json` files (sums, Σ value·n, worst Security, the
attributing / PR subsets): no differences; the WASM engine reproduced every week and month of the real
`fleet.json`, coverage included; the page shows each project's own numbers and links into its dashboard.
Nothing from the run is in this repo.

**M7 progress (2026-10-12).** Built: the name is `pmx` (D41). Release packaging with cargo-dist
(`dist-workspace.toml`, the generated `.github/workflows/release.yml`, `.github/build-setup.yml` that
builds the dashboard and the WASM engine into every target and fails instead of embedding the placeholder),
shell and PowerShell installers and the Homebrew formula for `kondfox/homebrew-tap` (D42). A local
`dist build` of the macOS arm64 archive gives a 15 MB binary that links only system libraries and
exports the full dashboard. CI gains a Windows build. The guided `pmx init` (D43,
`crates/pmx/src/wizard.rs`; `tests/cli.rs` `guided_init` drives it with scripted answers). User docs in
`docs/`: getting started, Velocity, fleet, troubleshooting, releasing, and the onboarding-test protocol;
a rewritten README. The tap repo `kondfox/homebrew-tap` and the `HOMEBREW_TAP_TOKEN` secret exist (2026-10-12).
**First release `v0.1.0` (2026-10-12):** all five targets built, the GitHub Release has the archives
and installers, the formula is in the tap, and `brew install kondfox/tap/pmx` installs a binary whose
dashboard is the full app. **Left:** the exit criterion: the protocol in `docs/onboarding-test.md` with someone new.

**Testing:**
- **Synthetic fixture repos**, built by the tests themselves with scripted authors, dates and file trees
  (`crates/pmx/tests/fixture.rs`, `crates/pm-git/src/tests.rs`), for classification, rework, pooling,
  identity merging, URL clones and the cache. Public CI runs these.
- **Golden parity files** (M0) for regression.
- **Recorded provider responses** for classifier tests, so CI makes no live LLM calls.

---

## 9. Open items

- **Remote fleet storage** and with it the CI templates (D2): shared git repo, bucket, or a small server.
- **Jev access:** early access or waitlist status, data-retention terms, and per-client contract
  checks before sending any diffs (same constraint as C8 in
  internal notes (not published)).
- **Local model choice** for sensitive repos. Pick it with `classifier eval`. **Candidate (2026-10-11):**
  `qwen3.5:9b` on Ollama 0.40 (fits an 18 GB Apple Silicon laptop, ~7 GB). Setup lessons: use the
  official Ollama app (`brew install --cask ollama-app`); the Homebrew formula ships without the xgrammar
  library, so every structured-output request fails with "structured output is unavailable". Start the
  server with `OLLAMA_CONTEXT_LENGTH=16384` (pmx's prompts reach ~10k tokens and the OpenAI endpoint can't
  set the context size) and classify with `effort = "none"`: with reasoning on, one trivial answer took
  2,500 tokens and two minutes; off, a package takes ~7 s and the logprobs land on the answer.
- ~~The **tool name**~~ — resolved 2026-10-12 (D41): `pmx`.
- **Cross-file SAST** stays an open decision, separate from this plan.
- ~~Bundled SAST rules licence~~ — resolved 2026-10-11 (D24): our own `pmx-sast@1` rules.
