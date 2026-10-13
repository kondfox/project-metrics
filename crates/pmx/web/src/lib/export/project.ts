import type { Buckets } from "../../data/project";
import type { Leader, Metrics, Package, ProjectFile } from "../../data/types";
import { METRICS, metricDef, roleLabel, type MetricId } from "../../metrics/registry";
import { fteOver } from "../../metrics/velocity";
import { formatDate, type DateRange } from "../dates";
import { fileSlug, type Cell, type PageExport, type Sheet } from "./sheet";

/** The page's areas, by spec section, for the Summary's "Section" column. */
const AREA: Record<string, string> = {
  "2": "Activity",
  "3": "Quality",
  "4": "Velocity",
  "5": "Multi-stack",
  "6": "AI-assisted",
  "7": "Security",
  "8": "Peer review",
};

const areaOf = (id: MetricId) => AREA[metricDef(id).spec.replace("§", "").split(".")[0]!] ?? "";

/** The metric ids a series or range carries, in the registry's order, stack mix last. */
function knownIds(keys: Iterable<string>): MetricId[] {
  const have = new Set(keys);
  const fixed = (Object.keys(METRICS) as MetricId[]).filter((id) => have.has(id));
  const mix = [...have].filter((k) => k.startsWith("mix_")).sort() as MetricId[];
  return [...fixed, ...mix];
}

export interface ProjectView {
  project: ProjectFile;
  range: DateRange;
  cadence: "week" | "month";
  /** The selected range; null when it needs the engine. */
  metrics: Metrics | null;
  /** People: only in the lead's own copy. */
  leaders: Leader[] | null;
  points: Record<string, number> | null;
  /** The charts' weeks or months. */
  buckets: Buckets;
  indices: number[];
  inRange: boolean[];
}

function summary(v: ProjectView): Sheet {
  const [from, to] = [formatDate(v.range.start), formatDate(v.range.end)];
  const name = v.project.project.name;
  const rows: Cell[][] = [
    ["Project", "From", "To", "Section", "Metric", "Value", "Unit", "Events (n)", "Metric id"],
  ];
  if (v.metrics) {
    for (const id of knownIds(Object.keys(v.metrics.values))) {
      const def = metricDef(id);
      rows.push([
        name,
        from,
        to,
        areaOf(id),
        def.label,
        v.metrics.values[id] ?? null,
        def.unit,
        v.metrics.n[id] ?? null,
        id,
      ]);
    }
  }
  const options = v.project.velocity?.options;
  const fte = options ? fteOver(options, v.range) : null;
  if (fte) {
    rows.push([name, from, to, "Velocity", "FTE (mean over business days)", fte.mean, "FTE", null, "fte"]);
    rows.push([
      name,
      from,
      to,
      "Velocity",
      "FTE-days",
      fte.fteDays,
      "FTE-days",
      fte.businessDays,
      "fte_days",
    ]);
  }
  return { name: "Summary", rows };
}

function trend(v: ProjectView): Sheet {
  const ids = knownIds(Object.keys(v.buckets.series));
  const rows: Cell[][] = [["Period", "From", "To", "In range", ...ids.map((id) => metricDef(id).label)]];
  v.indices.forEach((i, k) => {
    const r = v.buckets.ranges[i]!;
    rows.push([
      v.buckets.labels[i]!,
      formatDate(r.start),
      formatDate(r.end),
      v.inRange[k]!,
      ...ids.map((id) => v.buckets.series[id]?.[i] ?? null),
    ]);
  });
  return { name: v.cadence === "month" ? "Trend (monthly)" : "Trend (weekly)", rows };
}

function packages(list: Package[]): Sheet {
  const rows: Cell[][] = [
    [
      "Date",
      "Package",
      "Id",
      "Repos",
      "Commits",
      "Lines added",
      "R1",
      "R2",
      "R3",
      "R4",
      "R5",
      "R6",
      "Size",
      "Points",
      "Tests",
      "Classifier",
    ],
  ];
  for (const p of list)
    rows.push([
      p.date,
      p.description || p.id,
      p.id,
      p.repos.join(", "),
      p.commits,
      p.added,
      ...(p.axes ?? [null, null, null, null, null, null]),
      p.size,
      p.points,
      p.tests,
      p.classifier,
    ]);
  return { name: "Work packages", rows };
}

/** Everything the project page shows for its range: CSV is the Summary, XLSX adds the rest. */
export function projectExport(v: ProjectView): PageExport {
  const p = v.project;
  const [from, to] = [formatDate(v.range.start), formatDate(v.range.end)];
  const sheets: Sheet[] = [summary(v), trend(v)];
  if (p.velocity) {
    sheets.push(packages(p.velocity.packages.filter((x) => x.date >= from && x.date <= to)));
  }
  if (v.points && Object.keys(v.points).length) {
    const rows = Object.entries(v.points).sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
    sheets.push({ name: "Points by person", rows: [["Person", "Points"], ...rows] });
  }
  if (v.leaders?.length) {
    sheets.push({
      name: "Multi-stack leaders",
      rows: [
        ["Person", "Stacks", "Technologies", "Breadth"],
        ...v.leaders.map((l) => [
          l.name,
          l.stacks
            .map(roleLabel)
            .sort((a, b) => a.localeCompare(b))
            .join(", "),
          l.techs.join(", "),
          Math.round(l.breadth * 100),
        ]),
      ],
    });
  }
  if (p.security?.top_packages.length) {
    sheets.push({
      name: "Vulnerable packages",
      rows: [
        ["Package", "Version", "Advisories", "Worst severity"],
        ...p.security.top_packages.map((x) => [x.name, x.version, x.count, x.max_severity]),
      ],
    });
  }
  const hotspots = p.hotspots.flatMap((r) =>
    r.files.slice(0, 10).map((f) => [r.repo, f.file, f.score, f.revisions, f.complexity, f.churn]),
  );
  if (hotspots.length) {
    sheets.push({
      name: "Hotspots",
      rows: [["Repo", "File", "Score", "Revisions", "Complexity", "Churn"], ...hotspots],
    });
  }
  if (p.ai_compare) {
    const { ai, human } = p.ai_compare;
    sheets.push({
      name: "AI-assisted vs human",
      rows: [
        [`${p.ai_compare.from} – ${p.ai_compare.to}`, "AI-assisted", "Human"],
        ["Commits", ai.commits, human.commits],
        ["Lines added", ai.added, human.added],
        ["Median commit size", ai.med_size, human.med_size],
        ["Tests with code %", ai.test_pct, human.test_pct],
        ["Docs with code %", ai.doc_pct, human.doc_pct],
      ],
    });
  }
  sheets.push({
    name: "About",
    rows: [
      ["Field", "Value"],
      ["Project", p.project.name],
      ["From", from],
      ["To", to],
      ["Cadence", v.cadence === "month" ? "monthly" : "weekly"],
      ["As of", p.meta.as_of],
      ["Measured since", p.project.range_start],
      ["Repos", p.project.repos.map((r) => r.name).join(", ")],
      ["Includes people", v.leaders != null || v.points != null],
      ["Generated by", `pmx ${p.meta.tool_versions.pmx ?? ""} at ${p.meta.generated_at}`.trim()],
    ],
  });
  return { file: `${fileSlug(p.project.name)}_${from}_${to}`, csv: sheets[0]!, sheets };
}
