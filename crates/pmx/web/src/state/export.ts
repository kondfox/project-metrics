import { useCallback } from "react";
import { projectExport } from "../lib/export/project";
import type { PageExport } from "../lib/export/sheet";
import { useChartBuckets, useDashboard, useRangeMetrics } from "./hooks";

/** What the project page exports: built on click, from the same state the page shows. */
export function useProjectExport(): () => PageExport {
  const { project, range, cadence } = useDashboard();
  const r = useRangeMetrics();
  const { buckets, indices, inRange } = useChartBuckets();
  return useCallback(
    () =>
      projectExport({
        project,
        range,
        cadence,
        metrics: r?.metrics ?? null,
        leaders: r?.leaders ?? null,
        buckets,
        indices,
        inRange,
      }),
    [project, range, cadence, r, buckets, indices, inRange],
  );
}
