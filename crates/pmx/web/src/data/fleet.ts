import { bucketRange, sameRange, type Cadence, type DateRange } from "../lib/dates";
import type { Coverage, FleetFile, Pooled } from "./types";

/** One cadence's precomputed fleet row. */
export interface FleetBuckets {
  cadence: Cadence;
  labels: string[];
  ranges: DateRange[];
  series: FleetFile["series_weekly"];
  n: FleetFile["n_weekly"];
  coverage: Coverage[];
  lastIsPartial: boolean;
}

export function fleetBuckets(f: FleetFile, cadence: Cadence): FleetBuckets {
  const labels = cadence === "month" ? f.months : f.weeks;
  return {
    cadence,
    labels,
    ranges: labels.map((l) => bucketRange(cadence, l)),
    series: cadence === "month" ? f.series_monthly : f.series_weekly,
    n: cadence === "month" ? f.n_monthly : f.n_weekly,
    coverage: cadence === "month" ? f.coverage_monthly : f.coverage_weekly,
    lastIsPartial: cadence === "month" ? f.partial_month : f.partial_week,
  };
}

/** Without the engine, the fleet row of a range that is exactly one precomputed week or month. */
export function pooledBucket(f: FleetFile, range: DateRange): Pooled | null {
  for (const cadence of ["month", "week"] as const) {
    const b = fleetBuckets(f, cadence);
    const i = b.ranges.findIndex((r) => sameRange(r, range));
    if (i < 0) continue;
    const values: Pooled["values"] = {};
    const n: Pooled["n"] = {};
    for (const [k, v] of Object.entries(b.series)) values[k] = v[i] ?? null;
    for (const [k, v] of Object.entries(b.n)) n[k] = v[i] ?? null;
    const c = b.coverage[i] ?? { fed: {}, notes: {} };
    return { values, n, fed: c.fed, notes: c.notes };
  }
  return null;
}
