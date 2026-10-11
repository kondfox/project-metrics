import { createContext } from "react";
import type { MetricsEngine } from "../data/engine";
import type { LeadsFile, ProjectFile } from "../data/types";
import type { ViewState } from "./view";

export interface DashboardState extends ViewState {
  project: ProjectFile;
  leads: LeadsFile | null;
  engine: MetricsEngine | null;
  /** Set when this dashboard is a project of a fleet page (its key there). */
  fleetKey: string | null;
}

export const DashboardContext = createContext<DashboardState | null>(null);
