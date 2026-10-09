import { useCallback, useContext, useMemo } from "react";
import { bucketMetrics, buckets, type Buckets } from "../data/project";
import type { Leader, Metrics } from "../data/types";
import { chartWindow } from "../lib/ranges";
import { DashboardContext, type DashboardState } from "./context";

export function useDashboard(): DashboardState {
  const s = useContext(DashboardContext);
  if (!s) throw new Error("useDashboard must be used inside <DashboardProvider>");
  return s;
}

export interface RangeResult {
  metrics: Metrics;
  /** Multi-stack leaders with real names; only with the private lead view. */
  leaders: Leader[] | null;
}

/** Metrics for the selected range: from the engine, or a precomputed bucket when the range is one. */
export function useRangeMetrics(): RangeResult | null {
  const { project, leads, engine, range } = useDashboard();
  return useMemo(() => {
    if (engine) {
      const metrics = engine.range(range);
      const leaders = leads
        ? metrics.leaders.map((l) => ({ ...l, name: leads.people[l.name] ?? l.name }))
        : null;
      return { metrics, leaders };
    }
    const b = bucketMetrics(project, range);
    if (!b) return null;
    return { metrics: b.metrics, leaders: leads ? (leads.leaders[b.label] ?? []) : null };
  }, [project, leads, engine, range]);
}

/** The buckets charts show: the selected cadence over the chart window. */
export function useChartBuckets(): { buckets: Buckets; indices: number[]; inRange: boolean[] } {
  const { project, cadence, range, rangeContext } = useDashboard();
  return useMemo(() => {
    const b = buckets(project, cadence);
    const window = chartWindow(range, rangeContext);
    const indices = b.ranges.flatMap((r, i) => (r.end >= window.start && r.start <= window.end ? [i] : []));
    const inRange = indices.map((i) => b.ranges[i]!.end >= range.start && b.ranges[i]!.start <= range.end);
    return { buckets: b, indices, inRange };
  }, [project, cadence, range, rangeContext]);
}

/** Buckets with fewer denominator events are flagged low-n (spec §1.1). */
export function useLowN(): (n: number | null | undefined) => boolean {
  const threshold = useDashboard().project.meta.low_n_threshold;
  return useCallback((n) => n != null && n < threshold, [threshold]);
}
