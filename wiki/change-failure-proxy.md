# Change-Failure Proxy — Commit Convention & Detection Spec

**Purpose.** Make the change-failure guardrail (Quality-index constituent, [quality-index.md](quality-index.md)
§4 & rollout step 1) actually produce signal. Today the collector's `cfr_pct` / `fix_pct` read ≈0
because these teams rarely write "revert"/"hotfix" in commit subjects (see
internal notes (not published) §48–49), and because the current classifier can't
separate a **change failure** (fixing code that already shipped) from **ordinary in-development bug-fixing**
(which is normal iteration, already captured by Rework Rate).

This spec fixes both: a low-friction **commit convention** (the human/agent side) plus a **broadened,
linkage-aware detector** (the collector side). It stays pure-git, stack-agnostic, and $0 — consistent
with the DIY decision ([synthesis.md](synthesis.md) §7b #6).

---

## 1. What we actually want to count

**Change failure = a change that repairs previously-shipped code.** "Shipped" = merged to the main
branch or released (tagged). A fix to code still inside its own unmerged PR/branch is *iteration*, not a
change failure — don't count it here (Rework Rate already sees it).

Two derived metrics come out of the same data:
- **Change-Failure Rate (CFR proxy)** = change-failure commits ÷ total commits (or ÷ merges/releases) per window.
- **Fix latency** = median days from the *referenced broken commit* to the *fix* — needs the linkage the trailer below provides.

---

## 2. The commit convention (draft)

Built on [Conventional Commits](https://www.conventionalcommits.org/) (already a low-friction standard)
plus **git trailers** — the same mechanism the fleet already uses for AI attribution
(`Co-Authored-By: Claude`), so it's consistent with existing tooling.

**a) Reverts — no new convention needed.** Use native `git revert`. It auto-writes:
```
Revert "<original subject>"

This reverts commit 1a2b3c4d....
```
Both lines are machine-detectable, and the body gives the reverted SHA for free (→ linkage + latency).

**b) Production / post-release fixes — add a trailer.** When a commit fixes a defect in code that was
**already merged to main or released**, add a `Fixes:` trailer naming what broke:
```
fix: reject expired refresh tokens

Fixes: 9f8e7d6              # SHA of the breaking commit, or a release tag (v2.3.1), or #1423
Hotfix: true               # OPTIONAL — only for emergency/out-of-band production fixes
```

**c) In-development fixes stay plain.** A `fix:` correcting code that hasn't shipped yet (same PR, not
merged) gets **no** `Fixes:` trailer. That's the line between change-failure and normal iteration.

**Reference formats accepted in `Fixes:`** — a commit SHA (`[0-9a-f]{7,40}`), a release tag
(`v1.2.3`), or a tracker issue (`#1423`, `PROJ-88`). Multiple allowed, comma-separated.

---

## 3. Detection — patterns to add to the collector

Case-insensitive, multiline, evaluated over the **full commit message** (subject + body + trailers):

```python
import re

REVERT_SUBJECT = re.compile(r'^Revert ".*?"', re.I | re.M)            # git native
REVERT_BODY    = re.compile(r'^This reverts commit ([0-9a-f]{7,40})', re.I | re.M)  # → reverted SHA
REVERT_TYPE    = re.compile(r'^revert(\([^)]*\))?!?:\s', re.I | re.M) # conventional-commits type
HOTFIX_WORD    = re.compile(r'\b(?:hotfix|rollback|emergency)\b', re.I)
HOTFIX_TRAILER = re.compile(r'^Hotfix:\s*(?:true|yes|1)\s*$', re.I | re.M)
FIXES_TRAILER  = re.compile(r'^(?:Fixes|Fix|Closes|Resolves):\s*(.+)$', re.I | re.M)  # → ref(s)

SHA_REF   = re.compile(r'^[0-9a-f]{7,40}$', re.I)
TAG_REF   = re.compile(r'^v?\d+\.\d+', re.I)
ISSUE_REF = re.compile(r'^(?:#\d+|[A-Z][A-Z0-9]+-\d+)$')
```

**Classification — a commit is a `change_failure` if ANY of:**
1. `REVERT_SUBJECT` or `REVERT_TYPE` matches (a revert), **or**
2. `HOTFIX_TRAILER` matches, or `HOTFIX_WORD` matches in the **subject** (not deep in the body — limits false positives), **or**
3. it carries a `FIXES_TRAILER` whose reference is **"shipped"** (see linkage below).

**Linkage test (this is what excludes in-dev fixes and enables latency):** for each `Fixes:` reference,
- **SHA** → shipped iff that commit is an **ancestor of the fix commit's first parent**
  (`git merge-base --is-ancestor <ref> <fix>^1`). If it's only reachable inside the same unmerged
  branch, it's iteration → **don't count**.
- **release tag** → always "shipped".
- **tracker issue** → "shipped" iff the issue is labeled a bug in the tracker (or, tracker-less, count it
  but flag as lower-confidence).

**Latency:** for reverts, `fix.date − commit(reverted_SHA).date`; for `Fixes:` SHA refs,
`fix.date − commit(ref).date`. Report the **median** per window; skip unlinkable ones.

**Aggregation** (matches the rest of the pipeline): pool across a project's repos —
`CFR = Σ change_failure_commits / Σ total_commits`, never averaging per-repo rates.

---

## 4. Gaming resistance & counter-signals

- **Under-reporting** (fix-forward by hand, omit the trailer, avoid `git revert`) is the main risk —
  and the reason we *also* keep the native-revert and inverse-diff detection, not just the trailer.
- **Cross-check, don't trust in isolation** (per [git-derived-metrics.md](git-derived-metrics.md) §2.2):
  a real change-failure fix almost always **re-touches recently-changed lines**, so a genuine spike
  should correlate with **Rework Rate** and hotspot churn. A CFR that stays ~0 while rework and revert
  counts rise means the *convention* is being skipped, not that quality is perfect — surface that as a
  data-quality flag, not a green light.
- **Never per-individual** — CFR is a property of the delivery system, not a person
  ([synthesis.md](synthesis.md) §7a.3). Team/project only.
- Label it a **proxy**, not ground truth — true CFR needs deploy↔incident linkage
  ([dora-metrics.md](dora-metrics.md)), a per-project upgrade for later.

---

## 5. Rollout (seeding the convention so the signal appears)

1. **Commit template** — ship a repo `.gitmessage` with the `Fixes:` / `Hotfix:` trailers commented in,
   wired via `git config commit.template .gitmessage`.
2. **CLAUDE.md / agent instructions** — because a large and growing share of commits are AI-authored,
   put the convention in each repo's agent instructions so **agents emit the `Fixes:` trailer
   automatically** when fixing shipped code. This is the highest-leverage step — it makes the dominant
   commit source compliant by default.
3. **CONTRIBUTING.md** — one short paragraph: use `git revert` for reverts; add `Fixes: <ref>` when you
   repair already-merged/released code; plain `fix:` otherwise.
4. **Optional soft enforcement** — a `commit-msg` hook / `commitlint` rule that *reminds* (doesn't block)
   when a `fix:` commit touches files whose changed lines trace to already-merged commits but carries no
   `Fixes:` trailer. Keep it advisory — a hard gate would just push people to drop `fix:` entirely.
5. **Baseline note** — CFR will legitimately read low for ~1–2 months until the convention lands; treat
   the pre-convention period as "not measured," not as "0% failures," on the dashboard.

---

*Status: draft spec 2026-09-14 — rollout step 1 of [quality-index.md](quality-index.md) §5. Pure-git,
stack-agnostic, $0. Next after adoption: fold the change-failure score into Quality v1.*
