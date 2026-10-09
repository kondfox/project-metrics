# Portable metrics tool — implementation plan

**Status:** PLAN, nothing built yet. Drafted 2026-10-08, revised 2026-10-09 with owner decisions.
**What** to measure is defined by [metrics-spec.md](metrics-spec.md) (source of truth). This page covers
**how** to build the tool, which runs anywhere and shows its results in a web dashboard. Where this page
changes the spec, it says so with **⟂ spec change**. The spec must be updated to match before M1.

Working name: **`pmx`** (placeholder).

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
| `pm-config` | Load and validate `pmx.toml`; `[people]` identity, bots, externals (spec §1.3); FTE provider (spec §10.2); JSON-config importer |
| `pm-classify` | File class, test detection, stack role, technology (spec §1.4–1.5). Pure functions, table-driven tests |
| `pm-git` | Streaming parsers for `git log --numstat` and the rework walk (`log -p -w`); snapshot trees via `git worktree`/`archive`; fetch |
| `pm-snapshot` | Runs and parses the external tools; results cached by `(tool, tool version, sha)` |
| `pm-codehost` | GitHub and GitLab adapters behind `merged_prs()` (spec §10.4) |
| `pm-llm` | The classifier: the shared schema (§6.1), providers, data-policy guard, cache, eval harness |
| `pm-metrics` | Roll-ups, ratios, scores (Quality, Security, Velocity, multi-stack). No I/O. **Also compiled to WASM** for the dashboard |
| `pm-progress` | Work planning, ETA model, renderers (TTY / plain / JSON) (§5) |
| `pmx` (bin) | CLI (`clap`); `axum` for `serve`; dashboard bundled with `rust-embed` |
| `web/` | TypeScript + ECharts dashboard (the prototype already uses ECharts); loads `pm-metrics` WASM |

### 2.2 Technical choices

- **Shell out to `git`, not `gix`, in v1.** The spec defines its metrics by `git log --numstat` and
  `log -p -w` semantics (whitespace-ignoring diffs), so shelling out gives exact parity with the
  prototype. Revisit `gix` only if speed becomes a problem.
- **One metrics implementation for CLI and browser.** The dashboard recomputes custom ranges from
  `days` (spec §10.6). Running the same Rust code via WASM rules out the drift the prototype already
  has: the published fleet page is ahead of `collector/c.js`.
- **SQLite cache (`.pmx/cache.sqlite`).** Stores the last processed SHA per repo, per-day components,
  snapshot results by SHA, LLM answers by `(package id, diff hash, rubric version, provider, model
  version)`, PR pages, and per-unit timings for the ETA. Every finished unit is committed at once, so
  runs are resumable. The rework walk restarts from the last run's date minus 21 days.
- **External tools are optional.** `pmx doctor` reports what is installed. `pmx tools install` downloads
  pinned versions of the Go binaries (scc, osv-scanner, gitleaks) into the user cache. jscpd (Node) and
  semgrep (Python) are the setup friction. A missing tool shows as "not measured" (spec §10.5).

---

## 3. Developer setup for a multi-repo project

### 3.1 Workspace layout

One project is one **workspace folder**. The repos it lists can live anywhere.

```
~/metrics/opus-leo/
  pmx.toml        ← project definition (safe to commit; tokens only as env-var names)
  .pmx/           ← cache + bare clones of URL repos (gitignored)
  out/            ← project.json; out/private/ is never shared
```

### 3.2 Commands

| Command | What it does |
|---|---|
| `pmx init [dirs…]` | Scans for git clones. Suggests a branch per repo (from `origin/HEAD`) and a role from the repo's contents: `*.csproj` → backend, React/Vite → frontend, `android/`/`ios/` → mobile, Terraform → infra, Cypress/Playwright → qa, mixed → `per-file`. Reads the code-host type and project id from the remote URL. Lockfiles are auto-discovered. You confirm the suggestions; it writes `pmx.toml` |
| `pmx repo add <path\|url> [--role] [--branch]` · `list` · `remove` | Edit the repo list. For a URL, pmx keeps a bare clone in `.pmx/repos/` and only reads from it, so working copies are never touched |
| `pmx people` | Lists every author identity across **all** repos plus code-host logins, and proposes merges (same name with different emails, noreply addresses, matching logins) and bots/externals. You accept or edit; it writes `[people]`. This step is what makes cross-repo per-person metrics (multi-stack, active devs, mentoring) correct |
| `pmx check` | Every path exists, every branch resolves, tokens are present, no unmapped identities above a commit threshold |
| `pmx import-config config_opus.json` | Converts a prototype config (plus `externals.json`, `fte.json`, `secrets_triage.json`) |

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

[fte]
source = "file"                   # spec §10.2
file = "fte.toml"

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

`pmx fleet serve|export` reads each project's `out/project.json`, **never** `out/private/`. It pools
numerators and denominators across projects (never averaging ratios) and builds the fleet page. Every
headline links to that project's dashboard. Remote publishing (shared git repo, bucket, or server) is
deferred, so the manifest entries are paths for now. The entry type should be extensible to URLs later.

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
6. **Pull requests:** paginated, cached per page and by merge date.
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
| Git ingest | commit | `git rev-list --count` per repo, from the last processed SHA |
| Rework walk | commit | Commits since the last run's date minus 21 days |
| Snapshots | (SHA, tool) | Uncached week-end/month-end SHAs × installed tools, weighted by tree size (KLOC from a quick `scc`) |
| Pull requests | PR | API total (GitHub `total_count`, GitLab `X-Total`) minus cached |
| Velocity | work package | Unscored packages. Known only after ingest; estimated from commits until then |

### 5.2 ETA model

`ETA = Σ remaining units × cost per unit`. Cost per unit is learned per stage and per tool (and per KLOC
for snapshots) from earlier runs stored in the cache. A first run uses built-in defaults. The estimate is
corrected live as each stage runs, and it accounts for stages running in parallel.

### 5.3 Rendering

TTY (`indicatif`):

```
pmx collect · Project A                         overall ▕████████░░░░░░░░▏ 52%  ETA ~6m
 ✓ fetch          5 repos                                    4s
 ✓ git ingest     12,418 commits (9,870 cached)              18s
 ● snapshots      ▕██████░░░░▏ 61/104  jscpd opus-leo-web @ 2026-W33   ~4m
 ● pull requests  ▕████████░░▏ 812/1,003                                ~40s
 ○ velocity       ~140 packages · classifier: jev                       ~1m
 ⚠ semgrep not installed: SAST panels will show "not measured"
```

- `--progress=plain` prints a status line every 10 seconds (for CI and pipes).
- `--progress=json` emits newline-delimited events for other tools.
- `--dry-run` (alias `pmx plan`) prints the work table and the ETA without running anything.
- **Ctrl-C** loses at most the units in flight. The next run resumes.
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
| `command` | Any executable that reads the request JSON on stdin and writes the response JSON on stdout. This is the escape hatch for custom local classifiers |

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
versioned **question set**, `rubrics/six-axis@2.toml`. Jev's documented weak spots
([jev-1.13 jaggedness](https://docs.typesafe.ai/model-jaggedness/jev-1.13.md)) are unreliable counting,
poorly calibrated in-between scores, dates and arithmetic, and lower accuracy when the input contains
irrelevant text. The design therefore keeps counting and arithmetic in code:

| Axis | Question(s) | Decided in code |
|---|---|---|
| R1 layers | One **Noul** per named layer: transport/DTO, validation, service, domain, persistence, database object, UI, navigation | Count layers with p ≥ 0.5 → 1 = 0, 2 = 1, 3–4 = 2, 5+ = 3 |
| R2 data model · R3 contract · R4 algorithmic · R6 verification | One **Score** each, 4 levels from the rubric anchors, plus examples from the golden set | Most likely level, with the rubric's "when two anchors fit, take the lower" rule: choose the lower level if `P(lower) ≥ P(top) − δ` |
| R5 blast radius | Files and modules touched, counted in code, plus a **Noul**: "touches auth, sync, schema, ordering/filtering applied everywhere, or build/release?" | Yes → 3; else 0/1/2 from the file and module counts |
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

- Release binaries built by `cargo-dist` (macOS arm64/x64, Linux x64/arm64, Windows), a Homebrew tap,
  and `cargo binstall pmx`.
- `pmx doctor` and `pmx tools install` for external tools (§2.2).
- Onboarding target: a new developer goes from download to their first dashboard in under 15 minutes.
- CI templates (weekly GitLab job, GitHub Action) are **deferred** together with remote fleet storage (D2).

---

## 8. Milestones

| # | Deliverable | Done when |
|---|---|---|
| **M0** | Freeze the prototype outputs for Project A, Project B and Project C at pinned SHAs as golden files (private). Write the deviation register ([parity.md](parity.md)). Update metrics-spec.md with this page's ⟂ changes | Golden set + long-format table exist; the harness runs **locally** against them (golden data is private, so public CI uses synthetic fixtures instead) |
| **M1** | `pm-config`, `pm-classify`, `pm-git`; `init`, `repo`, `people`, `check`, `import-config`; the progress, resume and ETA framework; activity, rework, tests/docs, multi-stack and AI metrics; `project.json` + `private/leads.json` | Matches the golden files except the listed ⟂ differences |
| **M2** | Project dashboard: weekly/monthly, custom range via WASM, low-n and "not measured" states, `serve`/`export` | One project viewable end to end |
| **M3** | `pm-snapshot`: scc, jscpd, osv-scanner, gitleaks, semgrep; Quality with duplication; Security score; SAST trend; `doctor`, `tools install` | Quality and Security reproduced for all three projects |
| **M4** | `pm-codehost`: GitHub + GitLab; peer review, cycle time, review wait, mentoring (unified expert rule, spec §8) | Spec §8 complete |
| **M5** | `pm-llm`: schema types, `jev`, `openai-compatible`, `anthropic` and `command` providers, data-policy guard, confidence gate, cache, `classifier eval`; Velocity on `six-axis@2` | At least one remote and one local classifier pass the eval |
| **M6** | Local fleet: `fleet.toml`, aggregation, fleet dashboard | Fleet rebuilt from the three local project folders |
| **M7** | Packaging, `init` wizard polish, docs | 15-minute onboarding verified by someone new |

**Testing:**
- **Synthetic fixture repos**, generated by a script with scripted authors, dates and file trees, for
  unit and integration tests of classification, rework, pooling and identity merging.
- **Golden parity files** (M0) for regression.
- **Recorded provider responses** for classifier tests, so CI makes no live LLM calls.

---

## 9. Open items

- **Remote fleet storage** and with it the CI templates (D2): shared git repo, bucket, or a small server.
- **Jev access:** early access or waitlist status, data-retention terms, and per-client contract
  checks before sending any diffs (same constraint as C8 in
  internal notes (not published)).
- **Local model choice** for sensitive repos. Pick it with `classifier eval` once M5 lands.
- The **tool name**.
- **Cross-file SAST** stays an open decision, separate from this plan.
- **Bundled SAST rules licence.** Semgrep registry rules are under the Semgrep Rules License, which restricts
  redistribution, so they can't ship in this repo. Use the LGPL-licensed Opengrep rules fork (or rules we
  write) for the bundled snapshot in spec §7.4.
