import type { Metrics } from "../data/types";
import { formatNumber } from "../lib/format";

/** Under the Velocity value: the denominator, marked `~` when FTE is unknown (spec §4). */
export function velocityNote(m: Metrics): string {
  const per =
    m.values.velocity_approx === 1 ? "~ per active engineer-day (FTE unknown)" : "points per FTE-day";
  return `${per} · ${formatNumber(m.values.velocity_packages)} work packages`;
}
