import type { FleetFile, Metrics, Pooled } from "../data/types";
import { formatDate, type DateRange } from "../lib/dates";
import { fileSlug, type Cell, type PageExport, type Sheet } from "../lib/export/sheet";
import type { MetricId } from "../metrics/registry";
import { HEADLINES } from "./headlines";
import type { FleetRange } from "./hooks";

export interface FleetView extends FleetRange {
  fleet: FleetFile;
  range: DateRange;
  cadence: "week" | "month";
  trend(key: string | null, id: MetricId): Array<number | null>;
  periods: Array<{ label: string; range: DateRange }>;
}

const change = (now: number | null | undefined, was: number | null | undefined) =>
  now == null || was == null ? null : now - was;

/** The fleet table as on the page (fleet row first, projects by name); CSV is this, XLSX adds the trends. */
export function fleetExport(v: FleetView): PageExport {
  const [from, to] = [formatDate(v.range.start), formatDate(v.range.end)];
  const byName = [...v.fleet.projects].sort((a, b) => a.name.localeCompare(b.name));
  const header: Cell[] = ["Row", "From", "To", "As of"];
  for (const h of HEADLINES) header.push(h.label, `${h.label} change`);
  const row = (name: string, asOf: string, now: Metrics | Pooled | null, was: Metrics | Pooled | null) => {
    const r: Cell[] = [name, from, to, asOf];
    for (const h of HEADLINES)
      r.push(now?.values[h.id] ?? null, change(now?.values[h.id], was?.values[h.id]));
    return r;
  };
  const table: Sheet = {
    name: "Fleet",
    rows: [
      header,
      row("Fleet", v.fleet.as_of, v.pooled, v.previous?.pooled ?? null),
      ...byName.map((p) => row(p.name, p.as_of, v.rows[p.key] ?? null, v.previous?.rows[p.key] ?? null)),
    ],
  };
  const lines: Array<{ name: string; key: string | null }> = [
    { name: "Fleet", key: null },
    ...byName.map((p) => ({ name: p.name, key: p.key })),
  ];
  const trends: Sheet = {
    name: v.cadence === "month" ? "Trends (monthly)" : "Trends (weekly)",
    rows: [
      ["Period", "From", "To", ...HEADLINES.flatMap((h) => lines.map((l) => `${h.label} · ${l.name}`))],
      ...v.periods.map((p, i) => [
        p.label,
        formatDate(p.range.start),
        formatDate(p.range.end),
        ...HEADLINES.flatMap((h) => lines.map((l) => v.trend(l.key, h.id)[i] ?? null)),
      ]),
    ],
  };
  const about: Sheet = {
    name: "About",
    rows: [
      ["Field", "Value"],
      ["Fleet", v.fleet.name],
      ["From", from],
      ["To", to],
      ["Cadence", v.cadence === "month" ? "monthly" : "weekly"],
      ["As of", v.fleet.as_of],
      ["Change", "against the period just before, of the same length"],
      ["Generated at", v.fleet.generated_at],
    ],
  };
  return { file: `${fileSlug(v.fleet.name)}_${from}_${to}`, csv: table, sheets: [table, trends, about] };
}
