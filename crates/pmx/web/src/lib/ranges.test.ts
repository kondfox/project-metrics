import { formatDate, parseDate } from "./dates";
import { chartWindow, presetRange, type RangeContext } from "./ranges";

const ctx: RangeContext = {
  start: parseDate("2025-09-01"),
  asOf: parseDate("2026-10-09"),
  headlineMonth: "2026-09",
};
const fmt = (r: { start: Date; end: Date } | null) => (r ? [formatDate(r.start), formatDate(r.end)] : null);

describe("ranges", () => {
  it("resolves presets relative to as_of", () => {
    expect(fmt(presetRange("month", ctx))).toEqual(["2026-09-01", "2026-09-30"]);
    expect(fmt(presetRange("week", ctx))).toEqual(["2026-09-28", "2026-10-04"]);
    expect(fmt(presetRange("3m", ctx))).toEqual(["2026-07-01", "2026-09-30"]);
    expect(fmt(presetRange("all", ctx))).toEqual(["2025-09-01", "2026-10-09"]);
    expect(presetRange("custom", ctx)).toBeNull();
  });

  it("never starts before range_start", () => {
    expect(fmt(presetRange("12m", { ...ctx, start: parseDate("2026-01-15") }))).toEqual([
      "2026-01-15",
      "2026-09-30",
    ]);
  });

  it("charts at least a year back", () => {
    const r = { start: parseDate("2026-09-01"), end: parseDate("2026-09-30") };
    expect(fmt(chartWindow(r, ctx))).toEqual(["2025-09-30", "2026-09-30"]);
  });
});
