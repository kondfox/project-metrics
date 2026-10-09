import type { LeadsFile, ProjectFile } from "../data/types";

/** A tiny fictional project: two months, nine weeks, one in progress. */
export function fixtureProject(over: Partial<ProjectFile["project"]> = {}): ProjectFile {
  const months = ["2026-08", "2026-09"];
  const weeks = [
    "2026-W31",
    "2026-W32",
    "2026-W33",
    "2026-W34",
    "2026-W35",
    "2026-W36",
    "2026-W37",
    "2026-W38",
    "2026-W39",
  ];
  const m = (a: number | null, b: number | null) => [a, b];
  const w = (v: number | null) => weeks.map(() => v);
  return {
    schema: "pmx.project/1",
    project: {
      name: "Acme Shop",
      range_start: "2026-08-01",
      breadth_roles: ["frontend", "backend"],
      ai_attribution: true,
      repos: [{ name: "shop-api", role: "backend", sha: "0123456789abcdef0123", tip_date: "2026-09-30" }],
      ...over,
    },
    days: {},
    snapshots: {},
    weeks,
    months,
    series_monthly: {
      quality: m(70, 82.4),
      quality_constituents: m(3, 3),
      s_rework: m(80, 100),
      s_tests: m(60, 70),
      s_docs: m(75, 80),
      rework_pct: m(10, 18.42),
      test_discipline_pct: m(48, 56),
      doc_discipline_pct: m(30, 32),
      commits: m(40, 52),
      added: m(3000, 4100),
      commit_med: m(80, 95),
      commit_p90: m(400, 480),
      commits_per_dev_med: m(10, 13),
      active_devs: m(4, 4),
      multi_stack_pct: m(25, 50),
      breadth_index: m(20, 31.5),
      techs_per_dev: m(1.5, 2),
      ai_assist_commit_pct: m(20, 41.2),
      ai_assist_line_pct: m(25, 47.9),
      mix_backend: m(60, 55),
      mix_frontend: m(40, 45),
    },
    n_monthly: {
      rework_pct: m(2900, 4000),
      test_discipline_pct: m(30, 41),
      doc_discipline_pct: m(30, 41),
      multi_stack_pct: m(4, 4),
      commit_med: m(40, 52),
    },
    series_weekly: { commits: w(10), rework_pct: w(null) },
    n_weekly: { commits: w(null) },
    detail: { "2026-09": { stack_mix: { backend: 55, frontend: 45 } } },
    ai_compare: {
      from: "2025-09-30",
      to: "2026-09-30",
      ai: { commits: 120, added: 9000, med_size: 110, test_pct: 60, doc_pct: 30 },
      human: { commits: 300, added: 14000, med_size: 60, test_pct: 40, doc_pct: 20 },
    },
    security: null,
    hotspots: null,
    velocity: null,
    meta: {
      generated_at: "2026-10-01T00:00:00Z",
      as_of: "2026-09-30",
      partial_week: false,
      partial_month: false,
      headline_month: "2026-09",
      latest_commit: "2026-09-29",
      low_n_threshold: 10,
      tool_versions: { pmx: "0.1.0", git: "git version 2.47.0" },
      dialect: "spec",
      quality_constituents: ["rework", "tests", "docs"],
      not_measured: ["duplication"],
    },
  };
}

export const fixtureLeads: LeadsFile = {
  schema: "pmx.leads/1",
  project: "Acme Shop",
  people: { p1: "Jane Doe" },
  leaders: {
    "2026-09": [
      {
        name: "Jane Doe",
        stacks: ["frontend", "backend"],
        techs: ["React", "Go"],
        breadth: 0.98,
        lines: 1200,
      },
    ],
  },
};
