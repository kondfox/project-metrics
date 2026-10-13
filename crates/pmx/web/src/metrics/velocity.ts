import type { Metrics, VelocityOptions } from "../data/types";
import { addDays, formatDate, parseDate, type DateRange } from "../lib/dates";
import { formatNumber } from "../lib/format";

/** Under the Velocity value: the denominator, marked `~` when FTE is unknown (spec §4). */
export function velocityNote(m: Metrics): string {
  const per =
    m.values.velocity_approx === 1 ? "~ per active engineer-day (FTE unknown)" : "points per FTE-day";
  return `${per} · ${formatNumber(m.values.velocity_packages)} work packages`;
}

export interface FteOver {
  /** Mean FTE over the business days (spec §10.2). */
  mean: number;
  /** Σ FTE over the business days: Velocity's denominator (spec §4). */
  fteDays: number;
  businessDays: number;
}

/**
 * FTE over a range's business days (Mon–Fri), counted up to `as_of`, as pm-metrics `fte_days` does.
 * Null when FTE is unknown or a business day has none: Velocity then uses active engineer-days.
 */
export function fteOver(o: VelocityOptions, range: DateRange): FteOver | null {
  if (o.fte.length === 0) return null;
  const asOf = parseDate(o.as_of);
  const end = range.end < asOf ? range.end : asOf;
  let fteDays = 0;
  let businessDays = 0;
  for (let d = range.start; d <= end; d = addDays(d, 1)) {
    if (d.getUTCDay() === 0 || d.getUTCDay() === 6) continue;
    const day = formatDate(d);
    // The last matching period wins.
    const fte = [...o.fte]
      .reverse()
      .find((p) => (p.from == null || p.from <= day) && (p.to == null || day <= p.to))?.fte;
    if (fte == null) return null;
    fteDays += fte;
    businessDays += 1;
  }
  return businessDays === 0 ? null : { mean: fteDays / businessDays, fteDays, businessDays };
}
