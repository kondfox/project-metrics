import { band, formatNumber, formatPercent } from "./format";

describe("format", () => {
  it("formats numbers and gaps", () => {
    expect(formatNumber(12418)).toBe("12,418");
    expect(formatNumber(2.456, 1)).toBe("2.5");
    expect(formatNumber(null)).toBe("–");
    expect(formatPercent(18.4211)).toBe("18.4%");
    expect(formatPercent(undefined)).toBe("–");
  });

  it("uses the spec's score bands", () => {
    expect([95, 90, 89.9, 75, 55, 30, 29.9].map(band)).toEqual(["A", "A", "B", "B", "C", "D", "E"]);
  });
});
