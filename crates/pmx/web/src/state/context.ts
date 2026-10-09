import { createContext } from "react";
import type { MetricsEngine } from "../data/engine";
import type { LeadsFile, ProjectFile } from "../data/types";
import type { Cadence, DateRange } from "../lib/dates";
import type { PresetId, RangeContext } from "../lib/ranges";

export interface DashboardState {
  project: ProjectFile;
  leads: LeadsFile | null;
  engine: MetricsEngine | null;
  rangeContext: RangeContext;
  cadence: Cadence;
  preset: PresetId;
  /** The range the tiles summarize. */
  range: DateRange;
  setCadence(c: Cadence): void;
  setPreset(p: PresetId): void;
  setRange(r: DateRange): void;
}

export const DashboardContext = createContext<DashboardState | null>(null);
