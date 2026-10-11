import { useMemo, type ReactNode } from "react";
import type { MetricsEngine } from "../data/engine";
import type { LeadsFile, ProjectFile } from "../data/types";
import { parseDate } from "../lib/dates";
import type { RangeContext } from "../lib/ranges";
import { DashboardContext, type DashboardState } from "./context";
import { useViewState, ViewContext } from "./view";

export function DashboardProvider(props: {
  project: ProjectFile;
  leads: LeadsFile | null;
  engine: MetricsEngine | null;
  /** The project's key on a fleet page. */
  fleetKey?: string;
  children: ReactNode;
}) {
  const { project, leads, engine } = props;
  const fleetKey = props.fleetKey ?? null;
  const rangeContext = useMemo<RangeContext>(
    () => ({
      start: parseDate(project.project.range_start),
      asOf: parseDate(project.meta.as_of),
      headlineMonth: project.meta.headline_month,
    }),
    [project],
  );
  const view = useViewState(rangeContext, engine != null, fleetKey);
  const value = useMemo<DashboardState>(
    () => ({ ...view, project, leads, engine, fleetKey }),
    [view, project, leads, engine, fleetKey],
  );
  return (
    <ViewContext.Provider value={view}>
      <DashboardContext.Provider value={value}>{props.children}</DashboardContext.Provider>
    </ViewContext.Provider>
  );
}
