import { useCallback, useMemo } from "react";
import { bucketMetrics } from "../data/project";
import { pooledBucket } from "../data/fleet";
import type { Metrics, Pooled } from "../data/types";
import { useFleet } from "./context";

export interface FleetRange {
  /** The pooled fleet row; null when the range needs the engine. */
  pooled: Pooled | null;
  /** Key → that project's metrics; null when the range needs the engine. */
  rows: Record<string, Metrics | null>;
}

/** The selected range for every project and the fleet: from the engine, or precomputed buckets. */
export function useFleetRange(): FleetRange {
  const { fleet, projects, engine, range } = useFleet();
  return useMemo(() => {
    const rows: Record<string, Metrics | null> = {};
    for (const p of fleet.projects) {
      const e = engine?.projects[p.key];
      rows[p.key] = e ? e.range(range) : (bucketMetrics(projects[p.key]!, range)?.metrics ?? null);
    }
    return { pooled: engine ? engine.pool(range) : pooledBucket(fleet, range), rows };
  }, [fleet, projects, engine, range]);
}

export function useFleetLowN(): (n: number | null | undefined) => boolean {
  const threshold = useFleet().fleet.low_n_threshold;
  return useCallback((n) => n != null && n < threshold, [threshold]);
}
