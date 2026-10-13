import type { ReactNode } from "react";
import { metricDef, type MetricId } from "../../metrics/registry";
import { useDashboard, useLowN, useRangeMetrics } from "../../state/hooks";
import { formatMetric } from "./formatMetric";
import { Tile } from "./Tile";
import type { Metrics } from "../../data/types";

/** A metric of the selected range, presented from the registry. */
export function MetricTile(props: {
  id: MetricId;
  label?: string;
  /** Replaces the registry description; gets the range's metrics. */
  note?: (m: Metrics) => ReactNode;
  extra?: (m: Metrics) => ReactNode;
  /** Explains the metric behind a "?" next to its label. */
  help?: ReactNode;
}) {
  const r = useRangeMetrics();
  const lowN = useLowN();
  const threshold = useDashboard().project.meta.low_n_threshold;
  const def = metricDef(props.id);
  const label = props.label ?? def.label;
  if (!r)
    return (
      <Tile
        label={label}
        help={props.help}
        doc={def.spec}
        state="empty"
        note="needs the metrics engine for this range"
      />
    );
  const v = r.metrics.values[props.id] ?? null;
  const n = r.metrics.n[props.id] ?? null;
  const note = props.note ? props.note(r.metrics) : def.description;
  if (v == null)
    return (
      <Tile
        label={label}
        help={props.help}
        doc={def.spec}
        state="empty"
        note={def.nCounts ? `no ${def.nCounts} in this range` : note}
      />
    );
  return (
    <Tile
      label={label}
      help={props.help}
      doc={def.spec}
      value={
        <>
          {formatMetric(def, v)}
          {props.extra?.(r.metrics)}
        </>
      }
      note={note}
      lowN={!def.noLowN && lowN(n) ? { n: n!, threshold } : null}
    />
  );
}
