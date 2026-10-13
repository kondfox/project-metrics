import { useCallback } from "react";
import type { PageExport } from "../lib/export/sheet";
import { useFleet } from "./context";
import { fleetExport } from "./exportFleet";
import { useFleetRange, useFleetTrend, useFleetTrendPeriods } from "./hooks";

/** What the fleet page exports: the table as shown, plus each headline's trend. */
export function useFleetExport(): () => PageExport {
  const { fleet, range, cadence } = useFleet();
  const current = useFleetRange();
  const trend = useFleetTrend();
  const periods = useFleetTrendPeriods();
  return useCallback(
    () => fleetExport({ ...current, fleet, range, cadence, trend, periods }),
    [current, fleet, range, cadence, trend, periods],
  );
}
