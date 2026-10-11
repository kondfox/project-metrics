import { formatDate, parseDate } from "./dates";
import { parseHash, toHash } from "./urlState";

describe("url state", () => {
  it("round-trips", () => {
    const range = { start: parseDate("2026-06-15"), end: parseDate("2026-09-30") };
    const s = parseHash(toHash("month", range));
    expect(s.cadence).toBe("month");
    expect(s.project).toBeUndefined();
    expect(formatDate(s.range!.start)).toBe("2026-06-15");
  });

  it("carries a fleet page's project", () => {
    const range = { start: parseDate("2026-09-01"), end: parseDate("2026-09-30") };
    expect(toHash("week", range, "shop")).toBe("#p=shop&weekly&2026-09-01..2026-09-30");
    expect(parseHash("#p=shop&weekly&2026-09-01..2026-09-30")).toMatchObject({
      project: "shop",
      cadence: "week",
    });
  });

  it("ignores junk", () => {
    expect(parseHash("#nope&2026-13-99..x")).toEqual({});
    expect(parseHash("#weekly&2026-09-30..2026-09-01")).toEqual({ cadence: "week" });
    expect(parseHash("#p=")).toEqual({});
    expect(parseHash("")).toEqual({});
  });
});
