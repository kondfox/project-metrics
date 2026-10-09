import { bucketRange, formatDate, monthRange, overlaps, parseDate, weekRange } from "./dates";

const r = (x: { start: Date; end: Date }) => [formatDate(x.start), formatDate(x.end)];

describe("dates", () => {
  it("resolves month buckets, including leap years", () => {
    expect(r(monthRange("2024-02"))).toEqual(["2024-02-01", "2024-02-29"]);
    expect(r(monthRange("2026-12"))).toEqual(["2026-12-01", "2026-12-31"]);
  });

  it("resolves ISO weeks across year boundaries", () => {
    expect(r(weekRange("2025-W01"))).toEqual(["2024-12-30", "2025-01-05"]);
    expect(r(weekRange("2026-W41"))).toEqual(["2026-10-05", "2026-10-11"]);
    expect(r(weekRange("2020-W53"))).toEqual(["2020-12-28", "2021-01-03"]);
    expect(r(bucketRange("week", "2026-W01"))).toEqual(["2025-12-29", "2026-01-04"]);
  });

  it("tests overlap inclusively", () => {
    const a = { start: parseDate("2026-01-01"), end: parseDate("2026-01-31") };
    expect(overlaps(a, { start: parseDate("2026-01-31"), end: parseDate("2026-02-05") })).toBe(true);
    expect(overlaps(a, { start: parseDate("2026-02-01"), end: parseDate("2026-02-05") })).toBe(false);
  });
});
