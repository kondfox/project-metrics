import { formatDate, parseDate } from "./dates";
import { parseHash, toHash } from "./urlState";

describe("url state", () => {
  it("round-trips", () => {
    const range = { start: parseDate("2026-06-15"), end: parseDate("2026-09-30") };
    const s = parseHash(toHash("month", range));
    expect(s.cadence).toBe("month");
    expect(formatDate(s.range!.start)).toBe("2026-06-15");
  });

  it("ignores junk", () => {
    expect(parseHash("#nope&2026-13-99..x")).toEqual({});
    expect(parseHash("#weekly&2026-09-30..2026-09-01")).toEqual({ cadence: "week" });
    expect(parseHash("")).toEqual({});
  });
});
