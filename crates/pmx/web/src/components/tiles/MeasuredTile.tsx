import type { ReactNode } from "react";
import type { Metrics } from "../../data/types";
import type { MetricId, NotMeasuredId } from "../../metrics/registry";
import { useMeasured } from "../../state/hooks";
import { MetricTile } from "./MetricTile";
import { NotMeasuredTile } from "./NotMeasuredTile";

/** A metric that depends on an input the run may not have measured (a snapshot tool). */
export function MeasuredTile(props: {
  input: NotMeasuredId;
  id: MetricId;
  label?: string;
  note?: (m: Metrics) => ReactNode;
}) {
  const measured = useMeasured();
  if (!measured(props.input)) return <NotMeasuredTile id={props.input} />;
  return <MetricTile id={props.id} label={props.label} note={props.note} />;
}
