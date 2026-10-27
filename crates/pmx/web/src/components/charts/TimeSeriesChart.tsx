import { useMemo, type ReactNode } from "react";
import { nOf, seriesOf } from "../../data/project";
import { formatDate, type DateRange } from "../../lib/dates";
import { metricDef, type MetricId } from "../../metrics/registry";
import { useChartBuckets, useDashboard, useLowN } from "../../state/hooks";
import { useChartTheme } from "./chartTheme";
import { EChart } from "./EChart";
import { buildTimeSeriesOption, chartMarks, type SeriesSpec } from "./timeSeriesOption";
import styles from "./charts.module.css";

export type MetricSeries = Omit<SeriesSpec, "values" | "n" | "label"> &
  (
    | { metric: MetricId; label?: string }
    /** A value the dashboard works out per bucket (an input such as FTE, not a collected metric). */
    | { label: string; of: (bucket: DateRange) => number | null }
  );

/** A chart of metrics over the selected cadence's buckets, declared by metric id. */
export function TimeSeriesChart(props: {
  title: string;
  hint?: string;
  series: MetricSeries[];
  unit?: "percent" | "count";
  decimals?: number;
  band?: [number, number];
  max?: number;
  /** Draw the project's phase starts and markers (spec §4.1). */
  marks?: boolean;
}) {
  const { buckets, indices, inRange } = useChartBuckets();
  const { timeline } = useDashboard().project;
  const isLowN = useLowN();
  const theme = useChartTheme();
  const { series, unit, decimals, band, max } = props;
  const showMarks = props.marks ?? false;
  const option = useMemo(() => {
    const pick = <T,>(xs: T[]) => indices.map((i) => xs[i] ?? null);
    const marks = showMarks
      ? chartMarks(
          [
            ...timeline.phases.map((p) => ({ date: p.from, label: p.name, kind: "phase" as const })),
            ...timeline.markers.map((m) => ({ date: m.date, label: m.name, kind: "marker" as const })),
          ],
          (date) =>
            indices.findIndex((i) => {
              const r = buckets.ranges[i]!;
              return formatDate(r.start) <= date && date <= formatDate(r.end);
            }),
        )
      : [];
    return buildTimeSeriesOption({
      labels: indices.map((i) => buckets.labels[i]!),
      spans: indices.map((i) => {
        const r = buckets.ranges[i]!;
        return `${formatDate(r.start)} – ${formatDate(r.end)}`;
      }),
      inRange,
      partialLast: buckets.lastIsPartial && indices.at(-1) === buckets.labels.length - 1,
      isLowN,
      theme,
      unit,
      decimals,
      band,
      max,
      marks,
      series: series.map((s) =>
        "of" in s
          ? { ...s, values: indices.map((i) => s.of(buckets.ranges[i]!)) }
          : {
              ...s,
              label: s.label ?? metricDef(s.metric).label,
              values: pick(seriesOf(buckets, s.metric)),
              n: metricDef(s.metric).noLowN ? undefined : pick(nOf(buckets, s.metric)),
            },
      ),
    });
  }, [buckets, indices, inRange, isLowN, theme, series, unit, decimals, band, max, showMarks, timeline]);
  return (
    <ChartCard title={props.title} hint={props.hint}>
      <EChart option={option} />
    </ChartCard>
  );
}

export function ChartCard({ title, hint, children }: { title: string; hint?: string; children: ReactNode }) {
  return (
    <figure className={styles.card} aria-label={title} style={{ margin: 0 }}>
      <h3>{title}</h3>
      {hint && <div className={styles.hint}>{hint}</div>}
      {children}
    </figure>
  );
}

export function ChartGrid({ children }: { children: ReactNode }) {
  return <div className={styles.grid}>{children}</div>;
}
