import { useMemo, type ReactNode } from "react";
import type { FleetEngine } from "../data/engine";
import type { FleetFile, ProjectFile } from "../data/types";
import { parseDate } from "../lib/dates";
import type { RangeContext } from "../lib/ranges";
import { useViewState, ViewContext } from "../state/view";
import { FleetContext, type FleetState } from "./context";

export function FleetProvider(props: {
  fleet: FleetFile;
  projects: Record<string, ProjectFile>;
  engine: FleetEngine | null;
  children: ReactNode;
}) {
  const { fleet, projects, engine } = props;
  const rangeContext = useMemo<RangeContext>(
    () => ({
      start: parseDate(fleet.range_start),
      asOf: parseDate(fleet.as_of),
      headlineMonth: fleet.headline_month,
    }),
    [fleet],
  );
  const view = useViewState(rangeContext, engine != null);
  const value = useMemo<FleetState>(
    () => ({ ...view, fleet, projects, engine }),
    [view, fleet, projects, engine],
  );
  return (
    <ViewContext.Provider value={view}>
      <FleetContext.Provider value={value}>{props.children}</FleetContext.Provider>
    </ViewContext.Provider>
  );
}
