/** Dates are calendar days in UTC (`YYYY-MM-DD`), as in project.json. */

export const DAY_MS = 86_400_000;

export type Cadence = "week" | "month";

export interface DateRange {
  start: Date;
  /** Inclusive. */
  end: Date;
}

export const parseDate = (s: string): Date => new Date(`${s}T00:00:00Z`);

export const formatDate = (d: Date): string => d.toISOString().slice(0, 10);

export const addDays = (d: Date, n: number): Date => new Date(d.getTime() + n * DAY_MS);

export const isValidDate = (d: Date): boolean => !Number.isNaN(d.getTime());

/** `2026-09` → 1st–30th of September. */
export function monthRange(label: string): DateRange {
  const [y, m] = label.split("-").map(Number);
  return { start: new Date(Date.UTC(y!, m! - 1, 1)), end: new Date(Date.UTC(y!, m!, 0)) };
}

/** `2026-W41` → its Monday–Sunday (ISO 8601: week 1 holds January 4th). */
export function weekRange(label: string): DateRange {
  const [y, w] = label.split("-W").map(Number);
  const jan4 = new Date(Date.UTC(y!, 0, 4));
  const monday = addDays(jan4, -((jan4.getUTCDay() + 6) % 7) + (w! - 1) * 7);
  return { start: monday, end: addDays(monday, 6) };
}

export const bucketRange = (cadence: Cadence, label: string): DateRange =>
  cadence === "month" ? monthRange(label) : weekRange(label);

export const overlaps = (a: DateRange, b: DateRange): boolean => a.end >= b.start && a.start <= b.end;

export const sameRange = (a: DateRange, b: DateRange): boolean =>
  formatDate(a.start) === formatDate(b.start) && formatDate(a.end) === formatDate(b.end);
