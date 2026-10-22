# Security index — point-in-time score for dependencies and our own code

**Status:** v1 shipped 2026-09-15 (dependency advisories); own-code SAST findings joined it on
2026-10-22 (plan D63). Turns the fleet's raw "Vulns (crit / high)" column into a 0–100 **Security
score + A–E band**, consistent with [Quality](quality-index.md) and [Velocity](velocity-index.md). Kept
a **separate column** (not folded into the Quality gate) — it becomes the `gate_vuln` term in
`quality-index.md` §4.1 only once remediation-age data exists. The normative formula is
[spec §7](metrics-spec.md#7-security-fleet-headline--security-indexmd-internal-notes-not-published).

## 1. Why a score, not a raw count

Raw transitive-CVE counts are not a headline anyone can act on, and they are not comparable
across projects with very different dependency surfaces. But we must not "zero everyone" — a naive
gate (`0.5^n_critical`) drives every project to 0 because current critical counts are large
(Project B 14, Project A 45). The design goal: a **fixed-target** score (never fleet-relative), where an open
Critical is the enforceable red line, and severity/exposure grades the rest.

## 2. Formula

```
sev   = 10·critical + 3·high + 1·moderate + 0.2·low      # dependency advisories
      + 10·sast_high + 3·sast_medium + 1·sast_low        # own-code SAST findings (since D63)
base  = 100 · 0.5^(sev / H)                              # fixed-scale exponential decay
score = min(base, 40)  if red line  else base            # red line ⇒ at best band D
score = max(1, score)
band  = A≥90 · B≥75 · C≥55 · D≥30 · E<30                 # same fixed cut-points as Quality
```
Red line: an open dependency Critical, a live high-confidence secret, or an own-code high.

**Fixed constants (this is the "documented once" yardstick, not fleet-derived):**

| Constant | Value | Meaning |
|---|---|---|
| dependency weights | 10 / 3 / 1 / 0.2 | critical / high / moderate / low — CVSS-shaped, criticals dominate |
| own-code weights | 10 / 3 / 1 | high / medium / low — one level above dependencies (below) |
| `H` (half-load) | **250** | every 250 weighted-severity points halves the score (§3) |
| red-line cap | **40** | trips the gate → score capped at the top of band D |

### 2.1 Why one load for both sources

- **Same kind of thing:** an advisory and a SAST finding are both known, open, exploitable defects;
  severity-weighted counting answers "how much risk is open", wherever the door is.
- **Better than combining two sub-scores:** an average lets a clean side hide a dirty one and needs
  an arbitrary split between sources; the worse-of-two ignores fixes on the other side. One load keeps
  every finding's cost proportional to its severity, and needs no new constant.
- **Own code weighs one level higher:** a SAST finding is in code we run (many advisories sit in
  paths never called) and we can fix it ourselves.
- **The gate on top** keeps a red-line finding from hiding in a low load; below 40 the load still
  grades projects.
- **Limits:** a false-positive own-code high caps a project at 40, and reviewed findings can't be
  dismissed yet (a SAST triage list, like secrets', is the follow-up); own-code counts are small, so
  SAST acts mostly through the cap; languages the rules barely cover look clean (a lower bound).

Counts are **absolute**, not per-dependency: an open Critical is an exploitable door regardless of
how large the dependency tree is, and this matches the absolute `gate_vuln` intent. Per-package rate
is kept only as a **trend-only** secondary read (a big monorepo will always carry more transitive
CVEs than a 24-package app), never as the cross-project score.

## 3. Calibration pass (2026-09-15, against the three pilot projects)

`H` was chosen so every project sits **mid-band** (robust to small count changes) rather than on a
band edge:

| H | Project C (0 crit) | Project B (14 crit) | Project A (45 crit) |
|---|---|---|---|
| 250 **(chosen)** | 69 · **C** | 19 · **E** | 2 · **E** |
| 350 (rejected) | 77 · B | 31 · D (1 pt above floor) | 6 · E |

Chosen **H = 250**. Reads honestly: Project C (no criticals) lands **C** on its remaining high/moderate
load; Project B (14 crit) and Project A (45 crit) both land **E**, distinguished by the raw score (19 vs 2). The
critical-cap guarantees no project with an open Critical can ever read better than **D**.

**What 250 means** (no red-line finding): 250 points ≈ 25 criticals, 83 highs or 250 moderates.

| Band | Load allowed | Roughly |
|---|---|---|
| A (≥ 90) | ≤ 38 | up to ~12 highs |
| B (≥ 75) | ≤ 104 | up to ~35 highs |
| C (≥ 55) | ≤ 216 | up to ~72 highs |
| D (≥ 30) | ≤ 434 | up to ~145 highs |

It is a judgement fitted to three projects, on dependency counts only (own code joined later and adds
little load). It matters less than the cap: it grades projects without red-line findings, and how
far below 40 the gated ones fall. **Re-check when the fleet grows:** score every project at H = 200,
250, 300 and 350, keep the value that leaves the most projects mid-band, and record it as a decision.

## 4. Point-in-time caveat (marked `~`)

Without remediation-age / SLA data the score cannot tell a Critical opened **today** from one ignored
for a year. It is therefore a **snapshot**, flagged `~` on the fleet (same convention as approximate
Velocity). **Migration path:** when age data lands, replace the raw counts in `sev` with **overdue**
counts (past-SLA) and the identical formula *becomes* the `gate_vuln` term — no rework, and it can then
be folded into the Quality gate.

## 5. Data & tooling

- **Trend data:** `tools/sec_trend_all.py <config> <out.json>` — for every declared lockfile in every
  repo (`config.security_lockfiles`), extracts the lockfile as it existed at each month-end on the
  repo's branch and runs OSV-Scanner, pooling distinct-advisory severity counts across lockfiles.
  Generalizes the Project B-only `collector/sec_trend.py`. Verified: Project B' final month reproduces its
  `security{}` snapshot exactly (14/118/99/20).
- **Score:** `tools/security_score.py` — `score()/band()` for a snapshot, `score_series()` for a monthly
  trend. Constants above live here.
- **Surfacing:** the fleet view's `security` column shows score + band (`tools/patch_fleet.py`), with a
  monthly-score sparkline. Each project dashboard's "Open dependency vulnerabilities over time" panel is
  fed by injecting the four `sec_*` monthly arrays into `DATA.series` (`tools/patch_sec_panel.py`).
- **Coverage gap:** ecosystems without a scannable lockfile (Android/Gradle, .NET/NuGet) are uncovered,
  so a project's real exposure is a **lower bound**. Same caveat as the snapshot.
