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
    hotspots: [],
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
      not_measured: [
        "duplication",
        "complexity",
        "hotspots",
        "dependency_vulnerabilities",
        "secrets",
        "sast",
        "peer_review",
        "velocity",
      ],
      pull_requests: null,
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

/** The fixture with snapshot metrics measured: an open critical caps Security at 40. */
export function fixtureWithSnapshots(): ProjectFile {
  const p = fixtureProject();
  const m = (a: number | null, b: number | null) => [a, b];
  Object.assign(p.series_monthly, {
    security_score: m(52.1, 40),
    vuln_critical: m(0, 2),
    vuln_high: m(10, 12),
    vuln_moderate: m(20, 18),
    vuln_advisories: m(30, 32),
    vuln_packages: m(9, 10),
    secrets_high: m(0, 0),
    dup_pct: m(4.2, 4.0),
    s_dup: m(90, 91.7),
    quality_constituents: m(4, 4),
  });
  p.meta.not_measured = ["sast"];
  p.security = {
    date: "2026-09-30",
    top_packages: [{ name: "left-pad", version: "1.0.0", count: 3, max_severity: "critical" }],
    lockfiles: { "shop-api": ["package-lock.json"] },
  };
  return p;
}

/** The fixture with PR flow from GitLab (no PR sizes). */
export function fixtureWithPulls(): ProjectFile {
  const p = fixtureProject();
  const m = (a: number | null, b: number | null) => [a, b];
  Object.assign(p.series_monthly, {
    pr_count: m(14, 20),
    pr_cycle_median_h: m(30.5, 22.25),
    review_wait_median_h: m(4, null),
    review_coverage_pct: m(85.7, 0),
    mentoring_pairs: m(3, 0),
  });
  Object.assign(p.n_monthly, {
    pr_cycle_median_h: m(14, 20),
    review_wait_median_h: m(12, 0),
    review_coverage_pct: m(14, 20),
    mentoring_pairs: m(12, 0),
  });
  p.meta.not_measured = p.meta.not_measured.filter((x) => x !== "peer_review").concat("pr_size");
  p.meta.pull_requests = { host: "gitlab", repos: ["shop-api", "shop-web"], sizes: false };
  return p;
}

/** The fixture with scored work packages; FTE unknown in September's second package's month. */
export function fixtureWithVelocity(): ProjectFile {
  const p = fixtureProject();
  const m = (a: number | null, b: number | null) => [a, b];
  Object.assign(p.series_monthly, {
    velocity: m(0.31, 0.2),
    velocity_approx: m(0, 0),
    velocity_points: m(13, 9),
    velocity_packages: m(6, 2),
    points_per_commit: m(0.33, 0.17),
    points_per_kloc: m(4.3, 2.2),
    verification_debt_pts: m(5, 0),
  });
  Object.assign(p.n_monthly, { velocity: m(6, 2), velocity_points: m(6, 2) });
  p.meta.not_measured = p.meta.not_measured.filter((x) => x !== "velocity");
  const pkg = (id: string, date: string, points: number | null, description: string) => ({
    id,
    date,
    repos: ["shop-api"],
    commits: 3,
    added: 120,
    description,
    axes: points == null ? null : ([2, 1, 2, 1, 1, 2] as [number, number, number, number, number, number]),
    points,
    size: points == null ? null : "M",
    tests: true,
    truncated: false,
    classifier: "claude-code:claude-sonnet-5-5:pmx-prompt@1",
    model: "claude-sonnet-5-5",
  });
  p.velocity = {
    rubric: "six-axis@2",
    classifiers: ["claude-code:claude-sonnet-5-5:pmx-prompt@1"],
    options: { since: "2026-08-01", as_of: "2026-09-30", fte: [{ from: null, to: null, fte: 2 }] },
    packages: [
      pkg("SHOP-42", "2026-09-24", 3, "SHOP-42 Order totals honour regional tax rules"),
      pkg("p1~2026-W37", "2026-09-10", 3, "tidy the checkout form"),
      pkg("SHOP-40", "2026-08-20", 5, "SHOP-40 Customer nickname"),
    ],
  };
  return p;
}
