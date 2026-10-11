import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import { sameRange, type Cadence, type DateRange } from "../lib/dates";
import { PRESETS, presetRange, type PresetId, type RangeContext } from "../lib/ranges";
import { parseHash, toHash } from "../lib/urlState";

/** What a page shows: cadence and range. Shared by the project dashboard and the fleet page. */
export interface ViewState {
  rangeContext: RangeContext;
  cadence: Cadence;
  preset: PresetId;
  /** The range the tiles summarize. */
  range: DateRange;
  /** Custom ranges need the metrics engine. */
  hasEngine: boolean;
  setCadence(c: Cadence): void;
  setPreset(p: PresetId): void;
  setRange(r: DateRange): void;
}

export const ViewContext = createContext<ViewState | null>(null);

export function useView(): ViewState {
  const s = useContext(ViewContext);
  if (!s) throw new Error("useView must be used inside a dashboard or fleet provider");
  return s;
}

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

/**
 * Cadence and range, kept in the URL hash. `project` is the fleet page's project key, kept in the
 * hash so the fleet's links and the back button work.
 */
export function useViewState(
  rangeContext: RangeContext,
  hasEngine: boolean,
  project?: string | null,
): ViewState {
  const [cadence, setCadenceState] = useState<Cadence>(initialCadence);
  const [{ preset, range }, setView] = useState<{ preset: PresetId; range: DateRange }>(() => {
    const fromHash = parseHash(window.location.hash).range;
    if (!fromHash) return { preset: "month", range: presetRange("month", rangeContext)! };
    // A link from the fleet page (or a reload) keeps the preset it was made with.
    const preset = PRESETS.find((p) => {
      const r = presetRange(p.id, rangeContext);
      return r != null && sameRange(r, fromHash);
    });
    return { preset: preset?.id ?? "custom", range: fromHash };
  });

  useEffect(() => {
    const h = toHash(cadence, range, project);
    if (window.location.hash !== h) window.history.replaceState(null, "", h);
  }, [cadence, range, project]);

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

  return useMemo(
    () => ({ rangeContext, cadence, preset, range, hasEngine, setCadence, setPreset, setRange }),
    [rangeContext, cadence, preset, range, hasEngine, setCadence, setPreset, setRange],
  );
}
