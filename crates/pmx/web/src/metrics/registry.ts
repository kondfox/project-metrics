import type { Role } from "../data/types";

/**
 * How the dashboard presents each metric id of `project.json` (spec §10.7). Adding a metric to the
 * dashboard starts here; the numbers themselves come from Rust (never computed in TypeScript).
 */
export type Unit = "percent" | "count" | "lines" | "score" | "index" | "ratio";

export interface MetricDef {
  label: string;
  unit: Unit;
  decimals?: number;
  /** One line under the value. */
  description?: string;
  /** What the denominator `n` counts, for the low-n note. */
  nCounts?: string;
  /** Spec section. */
  spec: string;
}

export const METRICS = {
  quality: {
    label: "Quality",
    unit: "score",
    description: "geometric mean of the sub-scores present",
    spec: "§3",
  },
  quality_constituents: { label: "Quality constituents", unit: "count", spec: "§3" },
  s_rework: { label: "Rework score", unit: "score", spec: "§3.1" },
  s_tests: { label: "Tests score", unit: "score", spec: "§3.2" },
  s_docs: { label: "Docs score", unit: "score", spec: "§3.3" },
  rework_pct: {
    label: "Rework",
    unit: "percent",
    description: "lines deleted within 21 days of being added; target band 15–25%",
    nCounts: "added lines",
    spec: "§3.1",
  },
  test_discipline_pct: {
    label: "Tests with code",
    unit: "percent",
    description: "commits touching prod that also touch tests",
    nCounts: "commits touching prod",
    spec: "§3.2",
  },
  doc_discipline_pct: {
    label: "Docs with code",
    unit: "percent",
    description: "commits touching prod that also touch docs",
    nCounts: "commits touching prod",
    spec: "§3.3",
  },
  commits: { label: "Commits", unit: "count", spec: "§2" },
  added: { label: "Lines added", unit: "lines", description: "source files", spec: "§2" },
  commit_med: {
    label: "Commit size",
    unit: "lines",
    description: "median added + deleted lines",
    nCounts: "commits",
    spec: "§2",
  },
  commit_p90: { label: "Commit size p90", unit: "lines", spec: "§2" },
  commits_per_dev_med: {
    label: "Commits per dev",
    unit: "ratio",
    decimals: 1,
    description: "median over people",
    nCounts: "people",
    spec: "§2",
  },
  active_devs: {
    label: "Active devs",
    unit: "count",
    description: "added lines in a breadth role",
    spec: "§2",
  },
  multi_stack_pct: {
    label: "Multi-stack devs",
    unit: "percent",
    description: "≥ 40 lines in 2+ roles",
    nCounts: "active devs",
    spec: "§5",
  },
  breadth_index: {
    label: "Breadth index",
    unit: "index",
    decimals: 1,
    description: "mean normalized entropy × 100",
    nCounts: "active devs",
    spec: "§5",
  },
  techs_per_dev: {
    label: "Techs per dev",
    unit: "ratio",
    decimals: 2,
    description: "technologies with ≥ 40 lines",
    nCounts: "active devs",
    spec: "§5",
  },
  ai_assist_commit_pct: {
    label: "AI-assisted commits",
    unit: "percent",
    description: "Co-Authored-By: Claude trailer",
    nCounts: "commits",
    spec: "§6",
  },
  ai_assist_line_pct: { label: "AI-assisted lines", unit: "percent", nCounts: "added lines", spec: "§6" },
  dup_pct: {
    label: "Duplication",
    unit: "percent",
    decimals: 2,
    description: "duplicated lines (jscpd, ≥ 5 lines / 50 tokens); 3% → score 100, 15% → 0",
    spec: "§3.4",
  },
  s_dup: { label: "Duplication score", unit: "score", spec: "§3.4" },
  cx_per_kloc: {
    label: "Complexity / KLOC",
    unit: "ratio",
    decimals: 1,
    description: "cyclomatic complexity per 1,000 code lines",
    spec: "§3.5",
  },
  kloc: {
    label: "Code size",
    unit: "ratio",
    decimals: 1,
    description: "KLOC (scc, generated folders excluded)",
    spec: "§3.5",
  },
  security_score: {
    label: "Security",
    unit: "score",
    description: "dependency advisories by severity; an open critical or a live secret caps it at 40",
    spec: "§7.3",
  },
  vuln_critical: { label: "Critical", unit: "count", spec: "§7.1" },
  vuln_high: { label: "High", unit: "count", spec: "§7.1" },
  vuln_moderate: { label: "Moderate", unit: "count", spec: "§7.1" },
  vuln_low: { label: "Low", unit: "count", spec: "§7.1" },
  vuln_advisories: { label: "Advisories", unit: "count", description: "distinct per repo", spec: "§7.1" },
  vuln_packages: { label: "Vulnerable packages", unit: "count", spec: "§7.1" },
  vuln_total_packages: { label: "Packages scanned", unit: "count", spec: "§7.1" },
  secrets_high: {
    label: "Live secrets",
    unit: "count",
    description: "key material or provider tokens in the tree, outside tests, docs and triage",
    spec: "§7.2",
  },
  secrets_low: { label: "Low-confidence secret hits", unit: "count", spec: "§7.2" },
  sast_high: {
    label: "SAST high",
    unit: "count",
    description: "own-code findings outside tests",
    spec: "§7.4",
  },
  sast_medium: { label: "SAST medium", unit: "count", spec: "§7.4" },
  sast_low: { label: "SAST low", unit: "count", spec: "§7.4" },
  sast_new: { label: "New findings", unit: "count", spec: "§7.4" },
  sast_fixed: { label: "Fixed findings", unit: "count", spec: "§7.4" },
  sast_per_kloc: { label: "SAST per KLOC", unit: "ratio", decimals: 3, spec: "§7.4" },
  sast_kloc: { label: "KLOC scanned", unit: "ratio", decimals: 1, spec: "§7.4" },
  suppressions: {
    label: "Suppression markers",
    unit: "count",
    description: "nosemgrep, nosec, NOSONAR, …",
    spec: "§7.4",
  },
} as const satisfies Record<string, MetricDef>;

export type MetricId = keyof typeof METRICS | `mix_${Role}`;

const ROLE_LABELS: Record<Role, string> = {
  frontend: "Frontend",
  backend: "Backend",
  mobile: "Mobile",
  qa: "QA",
  infra: "Infra",
  data: "Data",
  docs: "Docs",
};

export const roleLabel = (r: Role): string => ROLE_LABELS[r];

export function metricDef(id: MetricId): MetricDef {
  if (id.startsWith("mix_")) {
    const role = id.slice(4) as Role;
    return { label: `${roleLabel(role)} share`, unit: "percent", nCounts: "added lines", spec: "§2" };
  }
  return METRICS[id as keyof typeof METRICS];
}

/**
 * Inputs a run may not measure: milestones not built yet, or tools missing on the machine that
 * collected (`meta.not_measured`, written by pmx).
 */
export const NOT_MEASURED = {
  velocity: { label: "Velocity", needs: "the LLM classifier (M5)" },
  peer_review: { label: "Peer review", needs: "code-host data (M4)" },
  duplication: { label: "Duplication", needs: "jscpd (Node): see `pmx doctor`" },
  complexity: { label: "Complexity", needs: "scc: `pmx tools install`" },
  hotspots: { label: "Hotspots", needs: "scc: `pmx tools install`" },
  dependency_vulnerabilities: { label: "Security", needs: "osv-scanner: `pmx tools install`" },
  secrets: { label: "Live secrets", needs: "gitleaks: `pmx tools install`" },
  sast: { label: "SAST", needs: "semgrep and `[snapshots] sast_rules`: see `pmx doctor`" },
} as const;

export type NotMeasuredId = keyof typeof NOT_MEASURED;

/** Inputs that are always missing in this version (later milestones). */
const NOT_BUILT: readonly NotMeasuredId[] = ["velocity", "peer_review"];

export function isMeasured(notMeasured: readonly string[], id: NotMeasuredId): boolean {
  return !NOT_BUILT.includes(id) && !notMeasured.includes(id);
}
