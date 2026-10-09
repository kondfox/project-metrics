# Security index — point-in-time dependency-vulnerability score

**Status:** v1 shipped 2026-09-15. Turns the fleet's raw "Vulns (crit / high)" column into a
0–100 **Security score + A–E band**, consistent with [Quality](quality-index.md) and
[Velocity](velocity-index.md). Kept a **separate column** (not folded into the Quality gate) —
it becomes the `gate_vuln` term in `quality-index.md` §4.1 only once remediation-age data exists.

## 1. Why a score, not a raw count

Raw transitive-CVE counts are not a headline anyone can act on, and they are not comparable
across projects with very different dependency surfaces. But we must not "zero everyone" — a naive
gate (`0.5^n_critical`) drives every project to 0 because current critical counts are large
(Project B 14, Project A 45). The design goal: a **fixed-target** score (never fleet-relative), where an open
Critical is the enforceable red line, and severity/exposure grades the rest.

## 2. Formula

```
sev   = 10·critical + 3·high + 1·moderate + 0.2·low      # severity-weighted exposure load
base  = 100 · 0.5^(sev / H)                              # fixed-scale exponential decay
score = min(base, 40)  if critical > 0  else base        # any open Critical ⇒ at best band D
score = max(1, score)
band  = A≥90 · B≥75 · C≥55 · D≥30 · E<30                 # same fixed cut-points as Quality
```

**Fixed constants (this is the "documented once" yardstick, not fleet-derived):**

| Constant | Value | Meaning |
|---|---|---|
| severity weights | 10 / 3 / 1 / 0.2 | critical / high / moderate / low — CVSS-shaped, criticals dominate |
| `H` (half-load) | **250** | every 250 weighted-severity points halves the score |
| critical cap | **40** | an open Critical trips the gate → score capped at the top of band D |

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
