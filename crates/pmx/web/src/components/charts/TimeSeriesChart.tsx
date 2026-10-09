import { useMemo, type ReactNode } from "react";
import { nOf, seriesOf } from "../../data/project";
import { metricDef, type MetricId } from "../../metrics/registry";
import { useChartBuckets, useLowN } from "../../state/hooks";
import { useChartTheme } from "./chartTheme";
import { EChart } from "./EChart";
import { buildTimeSeriesOption, type SeriesSpec } from "./timeSeriesOption";
import styles from "./charts.module.css";

export interface MetricSeries extends Omit<SeriesSpec, "values" | "n" | "label"> {
  metric: MetricId;
  label?: string;
}

/** A chart of metrics over the selected cadence's buckets, declared by metric id. */
export function TimeSeriesChart(props: {
  title: string;
  hint?: string;
  series: MetricSeries[];
  unit?: "percent" | "count";
  decimals?: number;
  band?: [number, number];
  max?: number;
}) {
  const { buckets, indices, inRange } = useChartBuckets();
  const isLowN = useLowN();
  const theme = useChartTheme();
  const { series, unit, decimals, band, max } = props;
  const option = useMemo(() => {
    const pick = <T,>(xs: T[]) => indices.map((i) => xs[i] ?? null);
    return buildTimeSeriesOption({
      labels: indices.map((i) => buckets.labels[i]!),
      inRange,
      partialLast: buckets.lastIsPartial && indices.at(-1) === buckets.labels.length - 1,
      isLowN,
      theme,
      unit,
      decimals,
      band,
      max,
      series: series.map(({ metric, label, ...rest }) => ({
        ...rest,
        label: label ?? metricDef(metric).label,
        values: pick(seriesOf(buckets, metric)),
        n: pick(nOf(buckets, metric)),
      })),
    });
  }, [buckets, indices, inRange, isLowN, theme, series, unit, decimals, band, max]);
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
