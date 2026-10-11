import { createContext, useContext } from "react";
import type { FleetEngine } from "../data/engine";
import type { FleetFile, ProjectFile } from "../data/types";
import type { ViewState } from "../state/view";

export interface FleetState extends ViewState {
  fleet: FleetFile;
  projects: Record<string, ProjectFile>;
  engine: FleetEngine | null;
}

export const FleetContext = createContext<FleetState | null>(null);

export function useFleet(): FleetState {
  const s = useContext(FleetContext);
  if (!s) throw new Error("useFleet must be used inside <FleetProvider>");
  return s;
}
