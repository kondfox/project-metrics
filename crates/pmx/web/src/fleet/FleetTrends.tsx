import { useMemo } from "react";
import { ChartCard, ChartGrid } from "../components/charts/TimeSeriesChart";
import { useChartTheme } from "../components/charts/chartTheme";
import { EChart } from "../components/charts/EChart";
import { buildTimeSeriesOption, type SeriesSpec } from "../components/charts/timeSeriesOption";
import { fleetBuckets } from "../data/fleet";
import { buckets as projectBuckets } from "../data/project";
import { chartWindow } from "../lib/ranges";
import { useFleet } from "./context";
import { HEADLINES, projectMeasures, type Headline } from "./headlines";
import { useFleetLowN } from "./hooks";

/** The fleet's weeks or months in the chart window, and which fall in the selected range. */
function useFleetChartBuckets() {
  const { fleet, cadence, range, rangeContext } = useFleet();
  return useMemo(() => {
    const b = fleetBuckets(fleet, cadence);
    const window = chartWindow(range, rangeContext);
    const indices = b.ranges.flatMap((r, i) => (r.end >= window.start && r.start <= window.end ? [i] : []));
    const inRange = indices.map((i) => b.ranges[i]!.end >= range.start && b.ranges[i]!.start <= range.end);
    return { buckets: b, indices, inRange };
  }, [fleet, cadence, range, rangeContext]);
}

/** One headline over time: a line per project, the fleet row emphasized. */
function HeadlineTrend({ h }: { h: Headline }) {
  const { fleet, projects, cadence } = useFleet();
  const { buckets, indices, inRange } = useFleetChartBuckets();
  const isLowN = useFleetLowN();
  const theme = useChartTheme();
  const option = useMemo(() => {
    const labels = indices.map((i) => buckets.labels[i]!);
    const series: SeriesSpec[] = [
      {
        label: "Fleet",
        emphasis: true,
        color: 6,
        values: indices.map((i) => buckets.series[h.id]?.[i] ?? null),
        n: indices.map((i) => buckets.n[h.id]?.[i] ?? null),
      },
    ];
    fleet.projects.forEach((fp, k) => {
      const p = projects[fp.key]!;
      if (!projectMeasures(p, h.input)) return;
      // Projects cover different spans: line their buckets up by label.
      const pb = projectBuckets(p, cadence);
      const at = new Map(pb.labels.map((l, i) => [l, i]));
      const pick = (xs: Array<number | null> | undefined) =>
        labels.map((l) => {
          const i = at.get(l);
          return i == null ? null : (xs?.[i] ?? null);
        });
      series.push({ label: fp.name, color: k % 6, values: pick(pb.series[h.id]), n: pick(pb.n[h.id]) });
    });
    return buildTimeSeriesOption({
      labels,
      inRange,
      partialLast: buckets.lastIsPartial && indices.at(-1) === buckets.labels.length - 1,
      isLowN,
      theme,
      unit: h.id.endsWith("_pct") ? "percent" : undefined,
      decimals: h.id === "velocity" ? 2 : undefined,
      max: h.score || h.id.endsWith("_pct") ? 100 : undefined,
      series,
    });
  }, [h, fleet, projects, cadence, buckets, indices, inRange, isLowN, theme]);
  return (
    <ChartCard title={h.label} hint={`fleet: ${h.pooling}`}>
      <EChart option={option} />
    </ChartCard>
  );
}

/** A chart per headline that at least one project measures. */
export function FleetTrends() {
  const { projects } = useFleet();
  const shown = HEADLINES.filter(
    (h) => !h.context && Object.values(projects).some((p) => projectMeasures(p, h.input)),
  );
  return (
    <ChartGrid>
      {shown.map((h) => (
        <HeadlineTrend key={h.id} h={h} />
      ))}
    </ChartGrid>
  );
}
