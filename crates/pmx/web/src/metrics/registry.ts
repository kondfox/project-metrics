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

/** Inputs this version can't measure yet, with what they need. */
export const NOT_MEASURED = {
  velocity: { label: "Velocity", needs: "the LLM classifier (M5)" },
  security: { label: "Security", needs: "osv-scanner and gitleaks snapshots (M3)" },
  peer_review: { label: "Peer review", needs: "code-host data (M4)" },
  duplication: { label: "Duplication", needs: "jscpd snapshots (M3)" },
} as const;

export type NotMeasuredId = keyof typeof NOT_MEASURED;
