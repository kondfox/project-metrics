import { addDays, DAY_MS, monthRange, type DateRange } from "./dates";

export type PresetId = "month" | "week" | "30d" | "3m" | "6m" | "12m" | "all" | "custom";

export interface RangeContext {
  /** First day measured (`range_start`). */
  start: Date;
  asOf: Date;
  /** The last complete month (`meta.headline_month`). */
  headlineMonth: string;
}

export const PRESETS: ReadonlyArray<{ id: PresetId; label: string }> = [
  { id: "month", label: "Last complete month" },
  { id: "week", label: "Last complete week" },
  { id: "30d", label: "Last 30 days" },
  { id: "3m", label: "Last 3 months" },
  { id: "6m", label: "Last 6 months" },
  { id: "12m", label: "Last 12 months" },
  { id: "all", label: "Everything" },
  { id: "custom", label: "Custom…" },
];

const clampStart = (d: Date, ctx: RangeContext) => (d < ctx.start ? ctx.start : d);

/** The range a preset stands for; `custom` has none. */
export function presetRange(id: PresetId, ctx: RangeContext): DateRange | null {
  const headline = monthRange(ctx.headlineMonth);
  const lastMonths = (n: number): DateRange => {
    const e = headline.end;
    const s = new Date(Date.UTC(e.getUTCFullYear(), e.getUTCMonth() - (n - 1), 1));
    return { start: clampStart(s, ctx), end: e };
  };
  switch (id) {
    case "month":
      return headline;
    case "week": {
      // The last Sunday on or before as_of ends the last complete week.
      const end = addDays(ctx.asOf, -ctx.asOf.getUTCDay());
      return { start: clampStart(addDays(end, -6), ctx), end };
    }
    case "30d":
      return { start: clampStart(addDays(ctx.asOf, -29), ctx), end: ctx.asOf };
    case "3m":
      return lastMonths(3);
    case "6m":
      return lastMonths(6);
    case "12m":
      return lastMonths(12);
    case "all":
      return { start: ctx.start, end: ctx.asOf };
    case "custom":
      return null;
  }
}

/** Charts show the trend: at least the year before the range's end. */
export function chartWindow(range: DateRange, ctx: RangeContext): DateRange {
  const yearBack = addDays(range.end, -365);
  return { start: clampStart(range.start < yearBack ? range.start : yearBack, ctx), end: range.end };
}

/**
 * The period just before a range, to show what changed: as many whole months when the range is whole
 * months, else as many days. Null when it would start before the first day measured.
 */
export function previousRange(range: DateRange, ctx: RangeContext): DateRange | null {
  const { start, end } = range;
  const wholeMonths = start.getUTCDate() === 1 && addDays(end, 1).getUTCDate() === 1;
  let prev: DateRange;
  if (wholeMonths) {
    const months =
      (end.getUTCFullYear() - start.getUTCFullYear()) * 12 + end.getUTCMonth() - start.getUTCMonth() + 1;
    prev = {
      start: new Date(Date.UTC(start.getUTCFullYear(), start.getUTCMonth() - months, 1)),
      end: addDays(start, -1),
    };
  } else {
    const days = Math.round((end.getTime() - start.getTime()) / DAY_MS) + 1;
    prev = { start: addDays(start, -days), end: addDays(start, -1) };
  }
  return prev.start < ctx.start ? null : prev;
}
