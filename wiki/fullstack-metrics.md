# Full-Stack Progression Metrics

**Status:** design (2026-09-13), grounded in `fullstack-and-expertise-research.md`.

> **v1 as-built rule (decided 2026-10-08):** the simple rule is the spec — a stack counts for a dev at ≥ 40 added source lines in the selected time window (stack = repo role, or per-file path guess in monorepos). The DOA / truck-factor / durable-line design below is future work. See [metrics-spec.md](metrics-spec.md) §5.

> **Prior-art note.** The literature measures *what a developer knows* from git well, but essentially
> nobody measures *breadth / becoming full-stack* — that framing is our own contribution. Three proven
> pieces are reused below: **Degree-of-Authorship (DOA)** as the validated per-file expertise score;
> **truck-factor** logic (restricted to one stack) as a hard-to-game per-stack *coverage* metric; and
> **review-as-knowledge-sharing** (Rigby & Bird: peer review raises the distinct files a developer
> knows by **66–150%**) as the evidence base for detecting mentoring. Key caveat carried throughout:
> authorship ≠ understanding, and Posnett/Devanbu's "dual focus" work links **high breadth to higher
> defect density** — so breadth MUST ship with a quality counter-signal (§5/§6).

**Goal.** Most developers currently work in a single stack (backend / frontend / mobile / QA /
devops / data). The company wants them to broaden — experimenting in unfamiliar stacks with teammates
who review and *explain* the new stack. We want to measure **progress toward full-stack**, at
company/team level for the dashboard, with per-developer growth profiles available to leads (framed as
development, never ranking).

This is a **capability-breadth** metric, which is different in kind from the delivery scorecard: it is
inherently about people and about *learning*, so it needs its own gaming-resistance and its own
interaction rules with the quality panel.

---

## 1. Building block: defining "stacks" objectively

Map each changed file to a **canonical stack role** so the method is uniform across projects even
though the tech differs:

`frontend · backend · mobile · infra/devops · test/QA · data/db` (extend as needed)

Mapping is **path + file-extension/language** based, configured per repo (a shared default heuristic +
per-repo overrides):
- frontend: `*.tsx|jsx|vue|svelte|css|scss`, `/web`, `/frontend`, `/components`
- backend: `*.go|java|rb|py|php|cs`, `/api`, `/server`, `/services`
- mobile: `*.swift|kt|dart`, `/ios`, `/android`
- infra/devops: `Dockerfile`, `*.tf|yaml` under `/.github|/k8s|/terraform|/deploy`
- test/QA: `*.spec.*|*.test.*`, `/tests`, `/e2e`
- data/db: `*.sql`, `/migrations`, `/db`

Exclude generated/vendored/lockfiles (as in the delivery scorecard).

---

## 2. Per-developer stack profile

Over a rolling window, compute each developer's **durable contribution** per stack (surviving/durable
lines — Diff-Delta-style, or blame-at-window-end approximation — NOT raw added lines, so trivial
padding doesn't count). This yields a distribution `p_s` = share of dev's durable work in stack `s`.

**Use the validated expertise score, not just line counts.** Per file, compute
**Degree-of-Authorship (Fritz/Murphy):**
`DOA = 3.293 + 1.098·FirstAuthorship + 0.164·Deliveries − 0.321·ln(1 + OthersChanges)`
(FirstAuthorship = 1 if the dev created the file; Deliveries = the dev's own changes to it;
OthersChanges = everyone else's). Normalize per file. Aggregate a dev's DOA over the files in a stack
to get a robust per-`(dev, stack)` **knowledge** score — this is the backbone of truck-factor tools
and is harder to game than raw lines.

A stack **"counts"** for a developer only past a **meaningfulness threshold** (e.g. ≥ X durable lines
across ≥ Y distinct PRs/weeks in that stack) — this kills the "one trivial cross-stack commit" game.

### Breadth score (two readings)

- **Stack count** (legible headline input): number of stacks that "count" for the dev. Drives
  "multi-stack developer %".
- **Breadth index** (nuanced, drill-down): normalized entropy of the distribution
  `H = -Σ p_s·log(p_s)`, scaled `breadth = H / log(S)` → 0 (all work in one stack) … 1 (evenly spread
  across all S stacks present). Entropy naturally discounts tiny slices, so it resists padding better
  than a raw count.

---

## 2b. Technology breadth (the second axis)

Stack-role breadth (§2) is *horizontal* — moving across layers (frontend ↔ backend ↔ mobile).
**Technology breadth** is a second, finer axis: distinct **languages/frameworks** a developer has
worked in, which can grow *within* a single stack. Someone who has shipped both **.NET** and **Java
Spring** backends is more versatile than a pure-.NET dev, even though both are "backend."

- **Detect technology per file** via language (extension) + the nearest enclosing **build/dependency
  manifest**: `*.csproj`/`Directory.Packages.props` → .NET; `pom.xml`/`build.gradle` (+ spring deps) →
  Java/Spring; `package.json` deps → React/Vue/Angular/Nest/Express; `pubspec.yaml` → Flutter;
  `Podfile`/`*.xcodeproj` → iOS/Swift; `go.mod` → Go; `requirements.txt`/`pyproject.toml` →
  Python/Django/FastAPI; etc. This is a per-repo **technology catalog** (default heuristics + repo
  overrides), the same pattern as the stack map.
- **Per-dev technology set** = technologies of the files they meaningfully authored (DOA/durable-line
  weighted, same thresholds as §2).
- **Technology-breadth score** = count of distinct technologies + normalized entropy across them.

The two axes combine into overall **versatility**: crossing *layers* (stack roles) and crossing
*technologies within a layer* both raise it. Report them separately on drill-down (so a lead sees
"broad across layers" vs "deep-stack but multi-framework"), and combined as one headline index.

## 3. The process signals (what we actually want to encourage)

Breadth is the *outcome*; these two are the **leading indicators** of the transition and directly
reward the behavior the company wants:

- **New-stack entry rate** — count of `(developer, stack)` pairs that are **new** this window vs the
  developer's trailing history (their first meaningful contribution to a stack). This is the moment a
  single-stack dev steps into a second stack. Trending up = the initiative is working.
- **Cross-stack mentoring pairs** — PRs where the **author is a learner** in stack X (little/no prior
  history in X) and a **reviewer is an expert** in X (top-quantile durable contribution to X). This
  captures exactly "teammates who review and explain the unfamiliar stack." Count + trend. Optionally
  weight by review depth (comments), since teaching reviews are substantive — but keep depth a
  descriptor, never a target.

Expert/learner level per `(dev, stack)` is derived from **DOA** over the stack's files: **expert** =
"knows" the stack (Avelino truck-factor thresholds: normalized DOA ≥ 0.75 and absolute DOA ≥ 3.293
across a meaningful share of the stack's files); **learner** = below threshold / newly entering.

> **Bonus per-stack robustness metric:** apply **truck-factor** logic *within a single stack* — greedily
> remove top DOA-knowers until >50% of that stack's files are "orphaned." The count is a hard-to-game
> **per-stack coverage / resilience** number that rises as more people genuinely learn the stack, and
> it doubles as a bus-factor reading per stack.

---

## 4. Front-face numbers (aggregate, dashboard)

| Metric | Definition | Reading |
|--------|------------|---------|
| **Multi-stack developer %** | share of active developers for whom ≥ 2 stack-roles "count" | headline, very legible; ↑ = progress |
| **Full-stack Breadth Index** | team avg of per-dev *versatility* (stack-role + technology breadth) × 100 (0–100) | headline; ↑ = broadening |
| **Cross-stack momentum** | new-stack/new-technology entries + mentoring-review pairs per quarter | leading/process indicator |

Drill-down separates the two axes: **stack-role breadth** (layers crossed) and **technology breadth**
(languages/frameworks crossed), so a lead can tell "broad across layers" from "deep-stack but
multi-framework."

Per-developer breadth profiles live one level down, **for the lead**, as a growth/skills view (who to
cross-train next, who's mentoring whom) — developmental framing, never a leaderboard, never tied to
comp/reviews.

---

## 5. Gaming resistance & the pairing rule

- **Trivial-touch padding** → durable-line weighting + meaningfulness threshold + entropy (a 1% slice
  barely moves the index).
- **Forced breadth producing bad code in the unfamiliar stack** → **pair breadth with the quality
  panel** (rework #4, change-failure #3), the same structural rule as everywhere else. This pairing is
  not optional here: Posnett/Devanbu's "dual focus" research empirically links **higher contribution
  breadth to higher defect density**, so breadth without a quality counter-signal would actively
  mislead. If breadth rises but quality in the expanded areas degrades, expansion is too fast /
  under-supported.
- **Support/morale** → cross-check against the **DevEx pulse**: are people supported, or stressed by
  being pushed into unfamiliar stacks? Breadth up + satisfaction down = pushing too hard.
- **Bus-factor synergy** → full-stack progression *improves* the knowledge-concentration / bus-factor
  gauge (more people across more areas). Breadth and bus factor reinforce, a useful cross-check.

---

## 6. The learning tax — critical interaction with the delivery scorecard

Working in a new stack is **slower and more error-prone at first** (the learning curve). If new-stack
work drags a team's headline lead-time and rework numbers, teams will **avoid cross-training to protect
their scorecard** — defeating the whole initiative. So:

- **Tag "learning / new-stack" PRs** (author is a learner in that stack).
- **Segment them out of, or report them separately from, the core delivery panel**, so the learning
  tax neither pollutes the delivery numbers nor penalizes anyone for growing.
- Expect and tolerate a **bounded, temporary** dip on new-stack work; the signal of health is that it
  *improves over time* as the developer climbs the curve.

---

## 7. Data feasibility

- Per-stack authorship, durable lines, new-stack entry, breadth → **pure git** (`git log --numstat` +
  blame + path→stack map + trailing-history comparison). ✓
- Mentoring review pairs, review depth → needs **PR review data** (reviewer identity) — same PR-API
  dependency as delivery metric #2 (review responsiveness). ✓

---

## 8. Open decisions

1. **Stack taxonomy** — adopt the canonical role set above (comparable across projects) vs let each
   project define its own stacks (more accurate, less comparable). Rec: canonical + per-repo path
   overrides.
2. **Per-developer breadth visibility** — this metric is developmental (a skills matrix), so
   per-developer profiles for leads are arguably appropriate here even though quality metrics are not.
   Confirm the framing (growth/planning, never ranking/comp).
3. **Learning-PR segmentation** — confirm we tag new-stack PRs and keep them out of the core delivery
   panel (rec: yes).
4. **Thresholds** — the "meaningful contribution" bar (lines / PRs / weeks) and the expert quantile.
