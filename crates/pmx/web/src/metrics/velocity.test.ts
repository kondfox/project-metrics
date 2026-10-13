import { parseDate } from "../lib/dates";
import { fteOver } from "./velocity";

const range = (s: string, e: string) => ({ start: parseDate(s), end: parseDate(e) });

// The same cases as pm-metrics `fte_days_over_business_days`.
describe("fteOver", () => {
  const o = {
    since: "2026-01-01",
    as_of: "2026-09-10",
    fte: [
      { from: null, to: null, fte: 2 },
      { from: "2026-09-01", to: null, fte: 4 },
    ],
  };

  it("sums FTE over business days, the last matching period winning", () => {
    expect(fteOver(o, range("2026-08-01", "2026-08-31"))).toEqual({ mean: 2, fteDays: 42, businessDays: 21 });
  });

  it("counts up to as_of", () => {
    expect(fteOver(o, range("2026-09-01", "2026-09-30"))).toEqual({ mean: 4, fteDays: 32, businessDays: 8 });
  });

  it("is null when FTE is unknown or a business day has none", () => {
    expect(fteOver({ ...o, fte: [] }, range("2026-08-01", "2026-08-31"))).toBeNull();
    const gap = { ...o, fte: [{ from: "2026-08-15", to: null, fte: 1 }] };
    expect(fteOver(gap, range("2026-08-01", "2026-08-31"))).toBeNull();
    expect(fteOver(gap, range("2026-08-17", "2026-08-23"))?.fteDays).toBe(5);
  });
});
