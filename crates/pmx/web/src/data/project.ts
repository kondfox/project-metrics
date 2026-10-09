import { bucketRange, sameRange, type Cadence, type DateRange } from "../lib/dates";
import type { MetricId } from "../metrics/registry";
import type { Metrics, ProjectFile } from "./types";

/** One cadence's precomputed series. */
export interface Buckets {
  cadence: Cadence;
  labels: string[];
  ranges: DateRange[];
  series: ProjectFile["series_weekly"];
  n: ProjectFile["n_weekly"];
  /** The last bucket is still running on as_of. */
  lastIsPartial: boolean;
}

export function buckets(p: ProjectFile, cadence: Cadence): Buckets {
  const labels = cadence === "month" ? p.months : p.weeks;
  return {
    cadence,
    labels,
    ranges: labels.map((l) => bucketRange(cadence, l)),
    series: cadence === "month" ? p.series_monthly : p.series_weekly,
    n: cadence === "month" ? p.n_monthly : p.n_weekly,
    lastIsPartial: cadence === "month" ? p.meta.partial_month : p.meta.partial_week,
  };
}

export const seriesOf = (b: Buckets, id: MetricId): Array<number | null> => b.series[id] ?? [];
export const nOf = (b: Buckets, id: MetricId): Array<number | null> => b.n[id] ?? [];

/** Mix roles present in the project (from the `mix_<role>` series). */
export const mixRoles = (p: ProjectFile) =>
  Object.keys(p.series_monthly)
    .filter((k) => k.startsWith("mix_"))
    .map((k) => k.slice(4) as `${string}`);

/**
 * Without the engine, a range can still be shown when it is exactly one precomputed week or month.
 */
export function bucketMetrics(p: ProjectFile, range: DateRange): { metrics: Metrics; label: string } | null {
  for (const cadence of ["month", "week"] as const) {
    const b = buckets(p, cadence);
    const i = b.ranges.findIndex((r) => sameRange(r, range));
    if (i < 0) continue;
    const label = b.labels[i]!;
    const values: Metrics["values"] = {};
    const n: Metrics["n"] = {};
    for (const [k, v] of Object.entries(b.series)) values[k] = v[i] ?? null;
    for (const [k, v] of Object.entries(b.n)) n[k] = v[i] ?? null;
    return { metrics: { values, n, stack_mix: p.detail[label]?.stack_mix ?? {}, leaders: [] }, label };
  }
  return null;
}
