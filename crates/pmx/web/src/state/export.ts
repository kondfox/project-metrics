import { useCallback } from "react";
import { bucketMetrics } from "../data/project";
import { projectExport } from "../lib/export/project";
import type { PageExport } from "../lib/export/sheet";
import { useChartBuckets, useDashboard, useRangeMetrics } from "./hooks";

/** What the project page exports: built on click, from the same state the page shows. */
export function useProjectExport(): () => PageExport {
  const { project, leads, range, cadence } = useDashboard();
  const r = useRangeMetrics();
  const { buckets, indices, inRange } = useChartBuckets();
  return useCallback(() => {
    const label = bucketMetrics(project, range)?.label;
    return projectExport({
      project,
      range,
      cadence,
      metrics: r?.metrics ?? null,
      leaders: r?.leaders ?? null,
      points: leads && label ? (leads.points[label] ?? null) : null,
      buckets,
      indices,
      inRange,
    });
  }, [project, leads, range, cadence, r, buckets, indices, inRange]);
}
