import { buildTimeSeriesOption, type TimeSeriesInput } from "./timeSeriesOption";

const theme = { ink: "#000", muted: "#666", line: "#ddd", faint: "#eee", good: "#0a0", series: ["#a", "#b"] };

function input(over: Partial<TimeSeriesInput> = {}): TimeSeriesInput {
  return {
    labels: ["2026-W38", "2026-W39", "2026-W40"],
    series: [{ label: "Rework", values: [10, null, 20], n: [100, 100, 3] }],
    inRange: [false, true, true],
    partialLast: true,
    isLowN: (n) => n != null && n < 10,
    theme,
    unit: "percent",
    ...over,
  };
}

type Opt = {
  xAxis: { data: string[] };
  yAxis: unknown[];
  series: Array<{ data: unknown[]; markArea?: { data: unknown[] }; markLine?: { data: unknown[] } }>;
};

describe("buildTimeSeriesOption", () => {
  it("keeps gaps, hollows low-n points and stars the running bucket", () => {
    const o = buildTimeSeriesOption(input()) as unknown as Opt;
    expect(o.xAxis.data).toEqual(["2026-W38", "2026-W39", "2026-W40*"]);
    const data = o.series[0]!.data;
    expect(data[0]).toBe(10);
    expect(data[1]).toBeNull();
    expect(data[2]).toMatchObject({ value: 20, symbol: "emptyCircle" });
  });

  it("shades the selected range and the target band", () => {
    const o = buildTimeSeriesOption(input({ band: [15, 25] })) as unknown as Opt;
    const areas = o.series[0]!.markArea!.data as Array<Array<Record<string, unknown>>>;
    expect(areas[0]![0]).toMatchObject({ yAxis: 15 });
    expect(areas[1]![0]).toMatchObject({ xAxis: "2026-W39" });
  });

  it("draws phase starts solid and markers dashed, on the first series only", () => {
    const o = buildTimeSeriesOption(
      input({
        series: [
          { label: "a", values: [1, 2, 3] },
          { label: "b", values: [1, 2, 3] },
        ],
        marks: [
          { index: 0, label: "development", kind: "phase" },
          { index: 2, label: "CLAUDE.md added", kind: "marker" },
        ],
      }),
    ) as unknown as Opt;
    const lines = o.series[0]!.markLine!.data as Array<
      Record<string, { type?: string; formatter?: string; show?: boolean }>
    >;
    expect(lines.every((l) => l.label!.show === false)).toBe(true); // named on hover
    expect(lines.map((l) => [l.xAxis, l.lineStyle!.type, l.label!.formatter])).toEqual([
      ["2026-W38", "solid", "development"],
      ["2026-W40*", "dashed", "CLAUDE.md added"],
    ]);
    expect(o.series[1]!.markLine).toBeUndefined();
    expect((buildTimeSeriesOption(input()) as unknown as Opt).series[0]!.markLine).toBeUndefined();
  });

  it("adds a right axis only when a series uses it", () => {
    expect((buildTimeSeriesOption(input()) as unknown as Opt).yAxis).toHaveLength(1);
    const two = input({
      series: [
        { label: "a", values: [1, 2, 3] },
        { label: "b", axis: 1, values: [4, 5, 6] },
      ],
    });
    expect((buildTimeSeriesOption(two) as unknown as Opt).yAxis).toHaveLength(2);
  });
});
