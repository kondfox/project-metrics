import { useCallback, useMemo } from "react";
import { bucketMetrics, buckets as projectBuckets, type Buckets } from "../data/project";
import { fleetBuckets, pooledBucket } from "../data/fleet";
import type { Metrics, Pooled } from "../data/types";
import type { DateRange } from "../lib/dates";
import { chartWindow, previousRange } from "../lib/ranges";
import type { MetricId } from "../metrics/registry";
import { useFleet } from "./context";

export interface FleetRange {
  /** The pooled fleet row; null when the range needs the engine. */
  pooled: Pooled | null;
  /** Key → that project's metrics; null when the range needs the engine. */
  rows: Record<string, Metrics | null>;
  /** The same for the period just before, to show what changed; null when there is none. */
  previous: { pooled: Pooled | null; rows: Record<string, Metrics | null> } | null;
}

/** The selected range for every project and the fleet: from the engine, or precomputed buckets. */
export function useFleetRange(): FleetRange {
  const { fleet, projects, engine, range, rangeContext } = useFleet();
  return useMemo(() => {
    const at = (r: DateRange) => {
      const rows: Record<string, Metrics | null> = {};
      for (const p of fleet.projects) {
        const e = engine?.projects[p.key];
        rows[p.key] = e ? e.range(r) : (bucketMetrics(projects[p.key]!, r)?.metrics ?? null);
      }
      return { pooled: engine ? engine.pool(r) : pooledBucket(fleet, r), rows };
    };
    const prev = previousRange(range, rangeContext);
    return { ...at(range), previous: prev && at(prev) };
  }, [fleet, projects, engine, range, rangeContext]);
}

/** Projects cover different spans: line a project's buckets up with the fleet's by label. */
function alignTo(labels: string[], pb: Buckets) {
  const at = new Map(pb.labels.map((l, i) => [l, i]));
  return (xs: Array<number | null> | undefined) =>
    labels.map((l) => {
      const i = at.get(l);
      return i == null ? null : (xs?.[i] ?? null);
    });
}

/**
 * A headline's values over the chart window (at least a year back to the range end): a project's
 * (by key) or the fleet row's (null).
 */
export function useFleetTrend(): (key: string | null, id: MetricId) => Array<number | null> {
  const { fleet, projects, cadence, range, rangeContext } = useFleet();
  const trend = useMemo(() => {
    const b = fleetBuckets(fleet, cadence);
    const window = chartWindow(range, rangeContext);
    const indices = b.ranges.flatMap((r, i) => (r.end >= window.start && r.start <= window.end ? [i] : []));
    const labels = indices.map((i) => b.labels[i]!);
    const perProject: Record<string, { pick: ReturnType<typeof alignTo>; series: Buckets["series"] }> = {};
    for (const [key, p] of Object.entries(projects)) {
      const pb = projectBuckets(p, cadence);
      perProject[key] = { pick: alignTo(labels, pb), series: pb.series };
    }
    return { b, indices, perProject };
  }, [fleet, projects, cadence, range, rangeContext]);
  return useCallback(
    (key, id) => {
      const { b, indices, perProject } = trend;
      if (key == null) return indices.map((i) => b.series[id]?.[i] ?? null);
      const p = perProject[key];
      return p ? p.pick(p.series[id]) : [];
    },
    [trend],
  );
}
