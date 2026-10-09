import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import type { MetricsEngine } from "../data/engine";
import type { LeadsFile, ProjectFile } from "../data/types";
import { parseDate, type Cadence, type DateRange } from "../lib/dates";
import { presetRange, type PresetId, type RangeContext } from "../lib/ranges";
import { parseHash, toHash } from "../lib/urlState";
import { DashboardContext, type DashboardState } from "./context";

const STORAGE_KEY = "pmx:cadence";

function initialCadence(): Cadence {
  const fromHash = parseHash(window.location.hash).cadence;
  if (fromHash) return fromHash;
  try {
    const saved = window.localStorage.getItem(STORAGE_KEY);
    if (saved === "week" || saved === "month") return saved;
  } catch {
    // Storage can be unavailable (private windows, file:// in some browsers).
  }
  return "week"; // spec §1.1: weekly by default
}

export function DashboardProvider(props: {
  project: ProjectFile;
  leads: LeadsFile | null;
  engine: MetricsEngine | null;
  children: ReactNode;
}) {
  const { project, leads, engine } = props;
  const rangeContext = useMemo<RangeContext>(
    () => ({
      start: parseDate(project.project.range_start),
      asOf: parseDate(project.meta.as_of),
      headlineMonth: project.meta.headline_month,
    }),
    [project],
  );
  const [cadence, setCadenceState] = useState<Cadence>(initialCadence);
  const [{ preset, range }, setView] = useState<{ preset: PresetId; range: DateRange }>(() => {
    const fromHash = parseHash(window.location.hash).range;
    return fromHash
      ? { preset: "custom", range: fromHash }
      : { preset: "month", range: presetRange("month", rangeContext)! };
  });

  useEffect(() => {
    const h = toHash(cadence, range);
    if (window.location.hash !== h) window.history.replaceState(null, "", h);
  }, [cadence, range]);

  const setCadence = useCallback((c: Cadence) => {
    setCadenceState(c);
    try {
      window.localStorage.setItem(STORAGE_KEY, c);
    } catch {
      // ignore
    }
  }, []);
  const setPreset = useCallback(
    (p: PresetId) => {
      const r = presetRange(p, rangeContext);
      setView((v) => ({ preset: p, range: r ?? v.range }));
    },
    [rangeContext],
  );
  const setRange = useCallback((r: DateRange) => setView({ preset: "custom", range: r }), []);

  const value = useMemo<DashboardState>(
    () => ({ project, leads, engine, rangeContext, cadence, preset, range, setCadence, setPreset, setRange }),
    [project, leads, engine, rangeContext, cadence, preset, range, setCadence, setPreset, setRange],
  );
  return <DashboardContext.Provider value={value}>{props.children}</DashboardContext.Provider>;
}
