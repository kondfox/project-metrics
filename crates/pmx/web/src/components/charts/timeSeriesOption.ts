import type { EChartsCoreOption } from "echarts/core";
import { formatNumber } from "../../lib/format";
import type { ChartTheme } from "./chartTheme";

export interface SeriesSpec {
  label: string;
  kind?: "line" | "bar";
  /** 0 = left axis, 1 = right axis. */
  axis?: 0 | 1;
  stack?: string;
  /** Drawn thicker (the headline series). */
  emphasis?: boolean;
  /** Palette index; defaults to the series' position. */
  color?: number;
  values: Array<number | null>;
  /** Denominators, for low-n points. */
  n?: Array<number | null>;
}

export interface TimeSeriesInput {
  labels: string[];
  series: SeriesSpec[];
  /** Which buckets fall in the selected range (shaded). */
  inRange: boolean[];
  /** The last bucket is still running on as_of (marked `*`). */
  partialLast: boolean;
  isLowN(n: number | null | undefined): boolean;
  theme: ChartTheme;
  unit?: "percent" | "count";
  decimals?: number;
  /** A target band on the left axis, e.g. [15, 25]. */
  band?: [number, number];
  max?: number;
}

/**
 * The ECharts option of a bucketed time series. Pure, so its conventions are unit tested: null is a
 * gap (no data ≠ zero), low-n points are hollow and faded, the running bucket is starred, and the
 * selected range is shaded.
 */
export function buildTimeSeriesOption(input: TimeSeriesInput): EChartsCoreOption {
  const { theme } = input;
  const labels = input.labels.map((l, i) =>
    input.partialLast && i === input.labels.length - 1 ? `${l}*` : l,
  );
  const first = input.inRange.indexOf(true);
  const last = input.inRange.lastIndexOf(true);
  const shadeRange = first >= 0 && last - first + 1 < labels.length;
  const fmt = (v: unknown) =>
    typeof v !== "number"
      ? "no data"
      : input.unit === "percent"
        ? `${v.toFixed(input.decimals ?? 1)}%`
        : formatNumber(v, input.decimals ?? 0);
  const axis = (max?: number) => ({
    type: "value",
    max,
    axisLabel: { color: theme.muted },
    splitLine: { lineStyle: { color: theme.line } },
  });
  const twoAxes = input.series.some((s) => s.axis === 1);

  return {
    animation: false,
    // The right padding leaves room for half of the last centered x label.
    grid: { left: 8, right: 30, top: 32, bottom: 4, containLabel: true },
    legend: { top: 0, left: "center", textStyle: { color: theme.ink }, itemWidth: 14, itemHeight: 8 },
    tooltip: { trigger: "axis", valueFormatter: fmt },
    xAxis: {
      type: "category",
      data: labels,
      axisLabel: { color: theme.muted },
      axisLine: { lineStyle: { color: theme.line } },
    },
    yAxis: twoAxes ? [axis(input.max), axis()] : [axis(input.max)],
    series: input.series.map((s, k) => ({
      name: s.label,
      type: s.kind ?? "line",
      stack: s.stack,
      yAxisIndex: s.axis ?? 0,
      connectNulls: false,
      showSymbol: true,
      symbolSize: 5,
      barMaxWidth: 18,
      lineStyle: { width: s.emphasis ? 3 : 1.6 },
      color: theme.series[(s.color ?? k) % theme.series.length],
      data: s.values.map((v, i) =>
        v == null
          ? null
          : input.isLowN(s.n?.[i])
            ? { value: v, symbol: "emptyCircle", symbolSize: 7, itemStyle: { opacity: 0.45 } }
            : v,
      ),
      markArea:
        k === 0 && (shadeRange || input.band)
          ? {
              silent: true,
              data: [
                ...(input.band
                  ? [
                      [
                        { yAxis: input.band[0], itemStyle: { color: theme.good, opacity: 0.1 } },
                        { yAxis: input.band[1] },
                      ],
                    ]
                  : []),
                ...(shadeRange
                  ? [
                      [
                        { xAxis: labels[first], itemStyle: { color: theme.faint, opacity: 0.7 } },
                        { xAxis: labels[last] },
                      ],
                    ]
                  : []),
              ],
            }
          : undefined,
    })),
  };
}
